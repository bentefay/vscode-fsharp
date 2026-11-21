//! Bounded one-shot decompilation. Successful documents are cached only while
//! their concrete input certificates remain valid. No managed process persists.
use crate::cancellation::CancellationToken;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_STDOUT: usize = 512 * 1024;
const MAX_STDERR: usize = 64 * 1024;
const MAX_REQUEST: usize = 64 * 1024;
const RSS_LIMIT: u64 = 128 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(5);
const CACHE_BYTES: i64 = 8 * 1024 * 1024;
const CACHE_ENTRIES: i64 = 64;

#[derive(Debug)]
pub struct DecompileResponse {
    pub result: Value,
    pub started: bool,
    pub cache_hit: bool,
    pub peak_rss_bytes: u64,
}

pub fn invoke_cached(
    conn: &Connection,
    executable: &Path,
    args: &[String],
    request: &Value,
    token: &CancellationToken,
) -> Result<DecompileResponse> {
    token.check()?;
    let key = cache_key(executable, args, request, token)?;
    conn.execute_batch("CREATE TABLE IF NOT EXISTS decompiler_cache(key TEXT PRIMARY KEY, response BLOB NOT NULL, touched INTEGER NOT NULL)")?;
    let stored: Option<Vec<u8>> = conn
        .query_row(
            "SELECT response FROM decompiler_cache WHERE key=?1 AND length(response)<=?2",
            params![key, MAX_STDOUT],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(bytes) = stored {
        if let Ok(result) = serde_json::from_slice::<Value>(&bytes) {
            if valid_document(&result) && certificate_valid(&result, token)? {
                conn.execute(
                    "UPDATE decompiler_cache SET touched=?2 WHERE key=?1",
                    params![key, stamp()],
                )?;
                return Ok(DecompileResponse {
                    result,
                    started: false,
                    cache_hit: true,
                    peak_rss_bytes: 0,
                });
            }
        }
        conn.execute("DELETE FROM decompiler_cache WHERE key=?1", [&key])?;
    }
    let response = invoke(executable, args, request, token)?;
    if response.result.get("completeness").and_then(Value::as_str) == Some("exact") {
        if !valid_document(&response.result) || !certificate_valid(&response.result, token)? {
            bail!("Decompiler returned an invalid or changed document certificate");
        }
        let data = serde_json::to_vec(&response.result)?;
        conn.execute(
            "INSERT OR REPLACE INTO decompiler_cache(key,response,touched) VALUES(?1,?2,?3)",
            params![key, data, stamp()],
        )?;
        loop {
            let (count, bytes): (i64, i64) = conn.query_row(
                "SELECT count(*),coalesce(sum(length(response)),0) FROM decompiler_cache",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if count <= CACHE_ENTRIES && bytes <= CACHE_BYTES {
                break;
            }
            conn.execute("DELETE FROM decompiler_cache WHERE key=(SELECT key FROM decompiler_cache ORDER BY touched LIMIT 1)", [])?;
        }
    }
    Ok(response)
}

pub fn invoke(
    executable: &Path,
    args: &[String],
    request: &Value,
    token: &CancellationToken,
) -> Result<DecompileResponse> {
    token.check()?;
    #[cfg(not(target_os = "linux"))]
    bail!("Bounded decompiler process ownership is currently implemented only on Linux");
    let mut input = serde_json::to_vec(request)?;
    input.push(b'\n');
    if input.len() > MAX_REQUEST {
        bail!("Decompiler request exceeds size budget");
    }
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("DOTNET_GCHeapHardLimit", "0x8000000");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command.spawn().context("Start one-shot decompiler")?;
    let mut owned = OwnedChild::new(child);
    let stdin = owned
        .child
        .stdin
        .take()
        .context("Decompiler stdin missing")?;
    let stdout = owned
        .child
        .stdout
        .take()
        .context("Decompiler stdout missing")?;
    let stderr = owned
        .child
        .stderr
        .take()
        .context("Decompiler stderr missing")?;
    enum Event {
        Output(Result<Vec<u8>>),
        Error(Result<Vec<u8>>),
        Input(Result<()>),
    }
    let (tx, rx) = mpsc::channel();
    let tx_out = tx.clone();
    thread::spawn(move || {
        let _ = tx_out.send(Event::Output(read_bounded(stdout, MAX_STDOUT)));
    });
    let tx_err = tx.clone();
    thread::spawn(move || {
        let _ = tx_err.send(Event::Error(read_bounded(stderr, MAX_STDERR)));
    });
    thread::spawn(move || {
        let mut pipe = stdin;
        let _ = tx.send(Event::Input(pipe.write_all(&input).map_err(Into::into)));
    });
    let started = Instant::now();
    let mut output = None;
    let mut errors = None;
    let mut exit = None;
    let mut peak = 0;
    loop {
        token.check()?;
        if started.elapsed() > DEADLINE {
            bail!("Decompiler exceeded five-second process deadline");
        }
        let rss = owned.sample();
        peak = peak.max(rss);
        if rss > RSS_LIMIT {
            bail!("Decompiler process tree exceeded sampled 128MiB RSS guard");
        }
        while let Ok(event) = rx.try_recv() {
            match event {
                Event::Output(value) => output = Some(value?),
                Event::Error(value) => errors = Some(value?),
                Event::Input(value) => {
                    if let Err(e) = value {
                        if exit.is_none() {
                            return Err(e.context("Write decompiler request"));
                        }
                    }
                }
            }
        }
        if exit.is_none() {
            exit = owned.child.try_wait()?;
            if exit.is_some() {
                owned.kill_descendants();
            }
        }
        if let (Some(status), Some(stdout), Some(stderr)) = (&exit, &output, &errors) {
            if !status.success() {
                bail!(
                    "Decompiler failed: {}",
                    String::from_utf8_lossy(stderr)
                        .chars()
                        .take(400)
                        .collect::<String>()
                );
            }
            let result: Value =
                serde_json::from_slice(stdout).context("Invalid decompiler JSON response")?;
            if result.get("completeness").and_then(Value::as_str) == Some("exact")
                && !valid_document(&result)
            {
                bail!("Decompiler document protocol is invalid");
            }
            return Ok(DecompileResponse {
                result,
                started: true,
                cache_hit: false,
                peak_rss_bytes: peak,
            });
        }
        thread::sleep(Duration::from_millis(10));
    }
}
fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("Decompiler pipe exceeded output size budget");
    }
    Ok(bytes)
}
fn stamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .min(i64::MAX as u128) as i64
}
fn hash_file(path: &Path, token: &CancellationToken) -> Result<String> {
    token.check()?;
    let mut input = fs::File::open(path)?;
    if input.metadata()?.len() > 128 * 1024 * 1024 {
        bail!("Decompiler input exceeds hash budget");
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        token.check()?;
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn cache_key(
    executable: &Path,
    args: &[String],
    request: &Value,
    token: &CancellationToken,
) -> Result<String> {
    let executable = fs::canonicalize(executable)?;
    let mut hash = Sha256::new();
    hash.update(b"decompiler-cache-v1\0");
    hash.update(include_bytes!("decompiler.rs"));
    hash.update(serde_json::to_vec(&(
        executable.to_string_lossy(),
        args,
        request,
    ))?);
    hash.update(hash_file(&executable, token)?);
    for arg in args {
        let path = Path::new(arg);
        if path.is_file() {
            hash.update(hash_file(path, token)?);
            if path.extension().and_then(|p| p.to_str()) == Some("dll") {
                for extension in ["deps.json", "runtimeconfig.json"] {
                    let sidecar = path.with_extension(extension);
                    hash.update(sidecar.to_string_lossy().as_bytes());
                    hash.update(if sidecar.exists() {
                        hash_file(&sidecar, token)?
                    } else {
                        "missing".into()
                    });
                }
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn certificate_valid(result: &Value, token: &CancellationToken) -> Result<bool> {
    let Some(deps) = result
        .get("dependencies")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty() && a.len() <= 1024)
    else {
        return Ok(false);
    };
    let mut paths = HashSet::new();
    let mut bytes = 0u64;
    for dep in deps {
        token.check()?;
        let (Some(path), Some(expected)) = (
            dep.get("path").and_then(Value::as_str),
            dep.get("sha256").and_then(Value::as_str),
        ) else {
            return Ok(false);
        };
        let path = Path::new(path);
        if !path.is_absolute() || !paths.insert(path) {
            return Ok(false);
        }
        if expected == "missing" {
            if path.exists() {
                return Ok(false);
            }
            continue;
        }
        let Ok(info) = path.metadata() else {
            return Ok(false);
        };
        bytes = bytes.saturating_add(info.len());
        if bytes > 512 * 1024 * 1024 || expected.len() != 64 {
            return Ok(false);
        }
        match hash_file(path, token) {
            Ok(actual) if actual == expected => (),
            Err(error) if error.is::<crate::cancellation::Cancelled>() => return Err(error),
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn valid_document(result: &Value) -> bool {
    if result.get("completeness").and_then(Value::as_str) != Some("exact")
        || result.get("targetKind").and_then(Value::as_str) != Some("decompiled-implementation")
    {
        return false;
    }
    let Some(doc) = result
        .get("documents")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        .and_then(|a| a.first())
    else {
        return false;
    };
    let (Some(uri), Some(text)) = (
        doc.get("uri").and_then(Value::as_str),
        doc.get("text").and_then(Value::as_str),
    ) else {
        return false;
    };
    let Some(key) = uri
        .strip_prefix("fsharp-navigator://metadata/")
        .and_then(|s| s.strip_suffix(".cs"))
    else {
        return false;
    };
    if key.len() != 64
        || !key.bytes().all(|c| c.is_ascii_hexdigit())
        || text.len() > 256 * 1024
        || doc.get("language").and_then(Value::as_str) != Some("csharp")
    {
        return false;
    }
    let Some(location) = result
        .get("locations")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 1)
        .and_then(|a| a.first())
    else {
        return false;
    };
    if location.get("uri").and_then(Value::as_str) != Some(uri) {
        return false;
    }
    let range = &location["range"];
    let (Some(line), Some(endline), Some(start), Some(end)) = (
        range["start"]["line"].as_u64(),
        range["end"]["line"].as_u64(),
        range["start"]["character"].as_u64(),
        range["end"]["character"].as_u64(),
    ) else {
        return false;
    };
    line == endline
        && start < end
        && text
            .lines()
            .nth(line as usize)
            .is_some_and(|line| end <= line.encode_utf16().count() as u64)
}

struct OwnedChild {
    child: Child,
    root: u32,
    root_start: Option<u64>,
    owned: HashMap<u32, u64>,
}
impl OwnedChild {
    fn new(child: Child) -> Self {
        let root = child.id();
        let root_start = identity(root).map(|x| x.1);
        Self {
            child,
            root,
            root_start,
            owned: HashMap::new(),
        }
    }
    fn sample(&mut self) -> u64 {
        let mut identities = HashMap::new();
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() {
                    if let Some(info) = identity(pid) {
                        identities.insert(pid, info);
                    }
                }
            }
        }
        self.owned
            .retain(|pid, start| identities.get(pid).is_some_and(|x| x.1 == *start));
        let mut queue: Vec<_> = self.owned.keys().copied().collect();
        if identities
            .get(&self.root)
            .is_some_and(|x| Some(x.1) == self.root_start)
        {
            queue.push(self.root);
        }
        let mut seen = HashSet::new();
        let mut rss = 0;
        while let Some(pid) = queue.pop() {
            if !seen.insert(pid) {
                continue;
            }
            let Some(&(_, start)) = identities.get(&pid) else {
                continue;
            };
            self.owned.insert(pid, start);
            if identity(pid).is_some_and(|x| x.1 == start) {
                rss += resident(pid);
            }
            queue.extend(
                identities
                    .iter()
                    .filter(|(_, x)| x.0 == pid)
                    .map(|(pid, _)| *pid),
            );
        }
        rss
    }
    fn kill_descendants(&mut self) {
        self.sample();
        #[cfg(unix)]
        unsafe {
            kill(-(self.root as i32), 9);
        }
        for (&pid, &start) in &self.owned {
            if identity(pid).is_some_and(|x| x.1 == start) {
                #[cfg(unix)]
                unsafe {
                    kill(pid as i32, 9);
                }
            }
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        self.kill_descendants();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}
fn identity(pid: u32) -> Option<(u32, u64)> {
    let s = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = s.rsplit_once(')')?.1.split_whitespace().collect();
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}
fn resident(pid: u32) -> u64 {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|s| s.split_whitespace().next())
                    .and_then(|s| s.parse::<u64>().ok())
            })
        })
        .unwrap_or(0)
        * 1024
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "decompiler-test-{}-{}",
                std::process::id(),
                stamp()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn script(&self, source: &str) -> Vec<String> {
            let path = self.0.join("helper.py");
            fs::write(&path, source).unwrap();
            vec![path.to_string_lossy().into_owned()]
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn python() -> &'static Path {
        Path::new("/usr/bin/python3")
    }
    #[test]
    #[ignore = "requires the separately built pinned .NET decompiler helper"]
    fn actual_helper_framework_and_package_documents_are_cacheable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let executable = root.join(".tools/dotnet/dotnet");
        let dll =
            root.join("semantic/Decompiler/bin/Release/net10.0/FSharpNavigator.Decompiler.dll");
        let args = vec![dll.to_string_lossy().into_owned(), "--once".into()];
        let package=PathBuf::from(std::env::var_os("HOME").unwrap()).join(".nuget/packages/icsharpcode.decompiler/11.1.0.9782/lib/netstandard2.0/ICSharpCode.Decompiler.dll");
        let cases=[(root.join(".tools/dotnet/packs/Microsoft.NETCore.App.Ref/10.0.11/ref/net10.0/System.Runtime.dll"),"P:System.String.Length"),(package,"P:ICSharpCode.Decompiler.DecompilerSettings.UsingDeclarations")];
        let conn = Connection::open_in_memory().unwrap();
        let token = CancellationToken::new();
        for (assembly, id) in cases {
            let request = serde_json::json!({"assemblyPath":assembly,"xmlDocId":id});
            let result = invoke_cached(&conn, &executable, &args, &request, &token).unwrap();
            assert!(result.started);
            assert_eq!(result.result["completeness"], "exact", "{}", result.result);
            assert!(result.peak_rss_bytes > 0 && result.peak_rss_bytes <= RSS_LIMIT);
            assert!(
                invoke_cached(&conn, &executable, &args, &request, &token)
                    .unwrap()
                    .cache_hit
            );
            println!("{id}: peak_tree_rss_bytes={}", result.peak_rss_bytes);
        }
    }
    #[test]
    fn bounds_stdout_and_sampled_descendant_rss() {
        let temp = Temp::new();
        for (source, expected) in [
            (
                "import sys,time\nsys.stdout.write('x'*600000);sys.stdout.flush();time.sleep(30)\n",
                "output size",
            ),
            (
                "import time\nx=bytearray(160*1024*1024)\ntime.sleep(30)\n",
                "RSS guard",
            ),
        ] {
            let args = temp.script(source);
            let started = Instant::now();
            let err = invoke(python(), &args, &json!({}), &CancellationToken::new()).unwrap_err();
            assert!(err.to_string().contains(expected), "{err:#}");
            assert!(started.elapsed() < Duration::from_secs(3));
        }
    }
    #[test]
    fn deadline_includes_a_child_that_never_reads_stdin() {
        let temp = Temp::new();
        let args = temp.script("import time\ntime.sleep(30)\n");
        let started = Instant::now();
        let err = invoke(
            python(),
            &args,
            &json!({"payload":"x".repeat(63000)}),
            &CancellationToken::new(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("deadline"), "{err:#}");
        assert!(started.elapsed() >= DEADLINE && started.elapsed() < Duration::from_secs(7));
    }
    #[test]
    fn cancellation_kills_an_observed_grandchild_even_after_setsid() {
        let temp = Temp::new();
        let pid_file = temp.0.join("grandchild.pid");
        let child_source = format!(
            "from pathlib import Path;import os,time;os.setsid();dest=Path({});tmp=dest.with_suffix('.tmp');tmp.write_text(str(os.getpid()));tmp.replace(dest);time.sleep(30)",
            serde_json::to_string(&pid_file.to_string_lossy()).unwrap()
        );
        let source=format!("import subprocess,sys,time\nsubprocess.Popen([sys.executable,'-c',{}])\ntime.sleep(30)\n",serde_json::to_string(&child_source).unwrap());
        let args = temp.script(&source);
        let token = CancellationToken::new();
        let background = token.clone();
        let worker = thread::spawn(move || invoke(python(), &args, &json!({}), &background));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !pid_file.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let pid: u32 = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
        thread::sleep(Duration::from_millis(100));
        token.cancel();
        assert!(worker
            .join()
            .unwrap()
            .unwrap_err()
            .is::<crate::cancellation::Cancelled>());
        for _ in 0..100 {
            let live = fs::read_to_string(format!("/proc/{pid}/stat"))
                .ok()
                .and_then(|s| {
                    s.rsplit_once(')')
                        .map(|(_, tail)| !tail.trim_start().starts_with('Z'))
                })
                .unwrap_or(false);
            if !live {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("Grandchild survived cancellation");
    }
    #[test]
    fn cache_invalidates_content_and_missing_sidecar_dependencies() {
        let temp = Temp::new();
        let assembly = temp.0.join("assembly.dll");
        let pdb = temp.0.join("assembly.pdb");
        let calls = temp.0.join("calls");
        fs::write(&assembly, b"first").unwrap();
        let script = format!(
            r#"import json,sys,hashlib,pathlib
request=json.load(sys.stdin)
assembly=pathlib.Path({assembly});pdb=pathlib.Path({pdb})
with open({calls},'a') as stream:stream.write('x')
uri='fsharp-navigator://metadata/'+('a'*64)+'.cs'
deps=[{{'path':str(p),'sha256':hashlib.sha256(p.read_bytes()).hexdigest() if p.exists() else 'missing'}} for p in [assembly,pdb]]
print(json.dumps({{'completeness':'exact','targetKind':'decompiled-implementation','documents':[{{'uri':uri,'language':'csharp','text':'public int Value => 1;'}}],'locations':[{{'uri':uri,'range':{{'start':{{'line':0,'character':11}},'end':{{'line':0,'character':16}}}}}}],'dependencies':deps}}))
"#,
            assembly = serde_json::to_string(&assembly.to_string_lossy()).unwrap(),
            pdb = serde_json::to_string(&pdb.to_string_lossy()).unwrap(),
            calls = serde_json::to_string(&calls.to_string_lossy()).unwrap()
        );
        let args = temp.script(&script);
        let conn = Connection::open_in_memory().unwrap();
        let token = CancellationToken::new();
        let request = json!({"assemblyPath":assembly});
        assert!(
            invoke_cached(&conn, python(), &args, &request, &token)
                .unwrap()
                .started
        );
        assert!(
            invoke_cached(&conn, python(), &args, &request, &token)
                .unwrap()
                .cache_hit
        );
        fs::write(&assembly, b"changed").unwrap();
        assert!(
            invoke_cached(&conn, python(), &args, &request, &token)
                .unwrap()
                .started
        );
        fs::write(&pdb, b"new").unwrap();
        assert!(
            invoke_cached(&conn, python(), &args, &request, &token)
                .unwrap()
                .started
        );
        assert_eq!(fs::read_to_string(calls).unwrap(), "xxx");
    }
    #[test]
    fn invalid_document_ranges_and_languages_are_not_cacheable() {
        let uri = format!("fsharp-navigator://metadata/{}.cs", "a".repeat(64));
        let mut result = json!({"completeness":"exact","targetKind":"decompiled-implementation","documents":[{"uri":uri,"language":"csharp","text":"😀 Value"}],"locations":[{"uri":uri,"range":{"start":{"line":0,"character":3},"end":{"line":0,"character":8}}}]});
        assert!(valid_document(&result));
        result["locations"][0]["range"]["end"]["character"] = json!(9);
        assert!(!valid_document(&result));
        result["locations"][0]["range"]["end"]["character"] = json!(8);
        result["documents"][0]["language"] = json!("fsharp");
        assert!(!valid_document(&result));
    }
}
