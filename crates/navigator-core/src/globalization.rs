//! Lazy validation of the bounded, observed Linux/.NET globalization profile.
//! Recorded culture names are provenance. Selector values, actual binary/data
//! bytes and loader selection domains are the live reuse certificate.
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub const PROFILE: &str = "linux-x64-dotnet10-icu-v1";
type Snapshot = Vec<(PathBuf, String)>;
static VERIFIED: OnceLock<Mutex<HashMap<String, Snapshot>>> = OnceLock::new();

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Globalization certificate lacks {key}"))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .with_context(|| format!("Globalization certificate lacks {key}"))
}
fn identity(path: &Path) -> Result<String> {
    crate::foreground::certificate_visit()?;
    fn describe(m: &fs::Metadata) -> String {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            format!(
                "{}:{}:{}:{}:{}:{}:{}:{}",
                m.dev(),
                m.ino(),
                m.mode(),
                m.len(),
                m.mtime(),
                m.mtime_nsec(),
                m.ctime(),
                m.ctime_nsec()
            )
        }
        #[cfg(not(unix))]
        {
            format!("{:?}:{}", m.modified(), m.len())
        }
    }
    let link = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok("missing".into())
        }
        Err(e) => return Err(e).context("Inspect globalization input"),
    };
    let mut result = describe(&link);
    if link.file_type().is_symlink() {
        result.push_str(
            fs::read_link(path)?
                .to_str()
                .context("Non-UTF8 globalization link")?,
        );
        match fs::metadata(path) {
            Ok(m) => result.push_str(&describe(&m)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => result.push_str("missing-target"),
            Err(e) => return Err(e).context("Inspect globalization link target"),
        }
    }
    Ok(result)
}
fn link_state(path: &Path) -> Result<Option<String>> {
    crate::foreground::certificate_visit()?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(Some(
            fs::read_link(path)?
                .into_os_string()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 globalization path link"))?,
        )),
        Ok(_) => Ok(None),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error).context("Inspect globalization path link"),
    }
}
fn snapshot(paths: &[PathBuf], links: &[PathBuf]) -> Result<Snapshot> {
    let mut result: Snapshot = paths
        .iter()
        .map(|path| {
            crate::cancellation::check()?;
            Ok((path.clone(), identity(path)?))
        })
        .collect::<Result<_>>()?;
    for path in links {
        crate::cancellation::check()?;
        result.push((path.clone(), serde_json::to_string(&link_state(path)?)?));
    }
    Ok(result)
}
/// Byte-lexicographic name + NUL + kind + NUL + raw symlink target + NUL.
/// Every immediate entry, including directories, is a selection witness.
fn directory_state(path: &Path) -> Result<(bool, String, u64)> {
    match fs::metadata(path) {
        Ok(m) => ensure!(
            m.is_dir(),
            "Globalization selection root is not a directory"
        ),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok((false, format!("{:x}", Sha256::digest([])), 0))
        }
        Err(e) => return Err(e).context("Read globalization selection root"),
    };
    let mut rows = Vec::new();
    for entry in fs::read_dir(path)? {
        crate::foreground::certificate_visit()?;
        crate::cancellation::check()?;
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("Non-UTF8 globalization selection entry"))?;
        let metadata = fs::symlink_metadata(entry.path())?;
        let kind = if metadata.file_type().is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };
        let target = if kind == "symlink" {
            fs::read_link(entry.path())?
                .into_os_string()
                .into_string()
                .map_err(|_| anyhow::anyhow!("Non-UTF8 globalization selection target"))?
        } else {
            String::new()
        };
        rows.push(format!("{name}\0{kind}\0{target}\0").into_bytes());
        ensure!(
            rows.len() <= 100000,
            "Globalization selection directory is too large"
        );
    }
    rows.sort();
    let mut hash = Sha256::new();
    for row in &rows {
        hash.update(row)
    }
    Ok((true, format!("{:x}", hash.finalize()), rows.len() as u64))
}
pub fn validate(value: &Value) -> Result<()> {
    validate_with(value, &mut crate::validation::Generation::default())
}
pub fn validate_with(value: &Value, generation: &mut crate::validation::Generation) -> Result<()> {
    let result = validate_inner(value, generation);
    generation.finish(result)
}
fn validate_inner(value: &Value, generation: &mut crate::validation::Generation) -> Result<()> {
    generation.ensure_active()?;
    ensure!(
        cfg!(all(target_os = "linux", target_arch = "x86_64")),
        "Observed globalization profile supports Linux x64 only"
    );
    ensure!(
        text(value, "profile")? == PROFILE
            && text(value, "runtimeVersion")? == "10.0.11"
            && text(value, "loaderVersion")? == "2.41",
        "Unsupported observed globalization profile/runtime"
    );
    ensure!(
        matches!(text(value, "libcMessagesLocale")?, "C" | "POSIX"),
        "Unsupported libc LC_MESSAGES bootstrap state"
    );
    for field in ["cultureName", "uiCultureName"] {
        ensure!(
            text(value, field)?.len() <= 128,
            "Invalid globalization culture provenance"
        );
    }
    ensure!(
        value.get("sortVersion").and_then(Value::as_i64).is_some()
            && text(value, "sortId")?.len() == 36,
        "Invalid globalization sort provenance"
    );
    let files = array(value, "files")?;
    let directories = array(value, "selectionDirectories")?;
    let links = array(value, "pathLinks")?;
    ensure!(
        !files.is_empty()
            && files.len() <= 4096
            && !directories.is_empty()
            && directories.len() <= 4096
            && !links.is_empty()
            && links.len() <= 16384,
        "Incomplete or oversized globalization selection certificate"
    );
    let mut paths = Vec::new();
    for input in files.iter().chain(directories) {
        let path = PathBuf::from(text(input, "path")?);
        ensure!(
            path.is_absolute(),
            "Globalization witness path must be absolute"
        );
        paths.push(path);
    }
    paths.sort();
    paths.dedup();
    let mut link_paths = Vec::new();
    for link in links {
        let path = PathBuf::from(text(link, "path")?);
        ensure!(
            path.is_absolute(),
            "Globalization path link must be absolute"
        );
        ensure!(
            link.get("target")
                .is_some_and(|v| v.is_null() || v.is_string()),
            "Invalid globalization link witness"
        );
        link_paths.push(path);
    }
    let key = format!("{:x}", Sha256::digest(serde_json::to_vec(value)?));
    let before = snapshot(&paths, &link_paths)?;
    if VERIFIED
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .get(&key)
        == Some(&before)
    {
        return Ok(());
    }
    for input in files {
        let path = Path::new(text(input, "path")?);
        generation
            .verify_file(path, text(input, "sha256")?, false)
            .with_context(|| {
                format!(
                    "Evaluated project globalization changed: {}; prepare the project again",
                    path.display()
                )
            })?;
    }
    for input in directories {
        let path = Path::new(text(input, "path")?);
        let (exists, hash, entries) = directory_state(path)?;
        ensure!(
            input.get("exists").and_then(Value::as_bool) == Some(exists)
                && text(input, "sha256")? == hash
                && input.get("entries").and_then(Value::as_u64) == Some(entries),
            "Evaluated project globalization selection changed: {}; prepare the project again",
            path.display()
        );
    }
    for link in links {
        let actual = link_state(Path::new(text(link, "path")?))?;
        ensure!(
            actual.as_deref() == link.get("target").and_then(Value::as_str),
            "Evaluated project globalization path selection changed: {}; prepare the project again",
            text(link, "path")?
        );
    }
    ensure!(
        snapshot(&paths, &link_paths)? == before,
        "Globalization inputs changed during validation"
    );
    let mut cache = VERIFIED.get().unwrap().lock().unwrap();
    if cache.len() >= 16 {
        cache.clear()
    }
    cache.insert(key, before);
    Ok(())
}
pub fn roots(certificate: &Value) -> Vec<(String, bool)> {
    let Some(g) = certificate.get("globalization") else {
        return Vec::new();
    };
    let mut roots: Vec<_> = g
        .get("selectionDirectories")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            v.get("path")
                .and_then(Value::as_str)
                .map(|p| (p.to_owned(), false))
        })
        .collect();
    for key in ["pathLinks", "files"] {
        for row in g.get(key).and_then(Value::as_array).into_iter().flatten() {
            if let Some(parent) = row
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| Path::new(path).parent())
                .and_then(Path::to_str)
            {
                roots.push((parent.to_owned(), false));
            }
        }
    }
    roots.sort();
    roots.dedup();
    roots
}
pub fn affects(certificate: &Value, path: &Path) -> bool {
    certificate
        .get("globalization")
        .and_then(|g| g.get("selectionDirectories"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("path").and_then(Value::as_str))
        .any(|root| path.starts_with(root) || Path::new(root).starts_with(path))
        || certificate
            .get("globalization")
            .and_then(|g| g.get("files"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.get("path").and_then(Value::as_str))
            .any(|file| path == Path::new(file) || Path::new(file).starts_with(path))
        || certificate
            .get("globalization")
            .and_then(|g| g.get("pathLinks"))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|row| row.get("path").and_then(Value::as_str))
            .any(|link| path == Path::new(link) || Path::new(link).starts_with(path))
}

#[cfg(test)]
fn file_state(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
#[cfg(test)]
pub fn fixture() -> Value {
    static VALUE: OnceLock<Value> = OnceLock::new();
    VALUE.get_or_init(||{
        let root=std::env::temp_dir().join(format!("fsharp-globalization-fixture-{}",std::process::id()));let dir=root.join("empty");fs::create_dir_all(&dir).unwrap();let file=root.join("runtime-marker");fs::write(&file,b"synthetic test runtime").unwrap();
        serde_json::json!({"profile":PROFILE,"runtimeVersion":"10.0.11","loaderVersion":"2.41","cultureName":"","uiCultureName":"","libcMessagesLocale":"C","sortVersion":0,"sortId":"00000000-0000-0000-0000-000000000000","files":[{"path":file,"sha256":file_state(&file).unwrap()}],"selectionDirectories":[{"path":dir,"exists":true,"sha256":format!("{:x}",Sha256::digest([])),"entries":0}],"pathLinks":[{"path":root,"target":null}]})
    }).clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn isolated() -> (PathBuf, Value) {
        let root = std::env::temp_dir().join(format!(
            "fn-globalization-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("selection")).unwrap();
        fs::write(root.join("icu"), b"first").unwrap();
        let mut cert = fixture();
        cert["files"] =
            json!([{"path":root.join("icu"),"sha256":file_state(&root.join("icu")).unwrap()}]);
        cert["selectionDirectories"] = json!([{"path":root.join("selection"),"exists":true,"entries":0,"sha256":format!("{:x}",Sha256::digest([]))}]);
        (root, cert)
    }
    #[test]
    fn loaded_binary_changes_reject_cached_and_fresh_certificate_then_recover() {
        let (root, mut cert) = isolated();
        validate(&cert).unwrap();
        validate(&cert).unwrap();
        let file = root.join("icu");
        let modified = fs::metadata(&file).unwrap().modified().unwrap();
        fs::write(&file, b"other").unwrap();
        fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert!(validate(&cert).is_err());
        let mut fresh = cert.clone();
        fresh["sortVersion"] = json!(1);
        assert!(validate(&fresh).is_err());
        cert["files"][0]["sha256"] = json!(file_state(&file).unwrap());
        validate(&cert).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn new_directory_and_icu_candidate_are_selection_dependencies() {
        let (root, mut cert) = isolated();
        validate(&cert).unwrap();
        fs::create_dir(root.join("selection/glibc-hwcaps")).unwrap();
        assert!(validate(&cert).is_err());
        let (exists, sha, entries) = directory_state(&root.join("selection")).unwrap();
        cert["selectionDirectories"][0] =
            json!({"path":root.join("selection"),"exists":exists,"sha256":sha,"entries":entries});
        validate(&cert).unwrap();
        fs::write(root.join("selection/libicuuc.so.99"), b"candidate").unwrap();
        assert!(validate(&cert).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn directory_digest_is_byte_lexicographic_includes_kind_and_link_target() {
        let (root, _) = isolated();
        let dir = root.join("selection");
        fs::write(dir.join("long-name"), b"a").unwrap();
        fs::create_dir(dir.join("z")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("long-name", dir.join("a-link")).unwrap();
        let mut rows = vec![
            b"long-name\0file\0\0".to_vec(),
            b"z\0directory\0\0".to_vec(),
        ];
        #[cfg(unix)]
        rows.push(b"a-link\0symlink\0long-name\0".to_vec());
        rows.sort();
        let expected = format!("{:x}", Sha256::digest(rows.concat()));
        assert_eq!(
            directory_state(&dir).unwrap(),
            (true, expected, rows.len() as u64)
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_selection_directory_creation_and_unknown_profiles_reject() {
        let (root, mut cert) = isolated();
        let missing = root.join("new-root");
        cert["selectionDirectories"][0] = json!({"path":missing,"exists":false,"sha256":format!("{:x}",Sha256::digest([])),"entries":0});
        validate(&cert).unwrap();
        fs::create_dir(&missing).unwrap();
        assert!(validate(&cert).is_err());
        cert["selectionDirectories"][0]["exists"] = json!(true);
        validate(&cert).unwrap();
        cert["profile"] = json!("unknown");
        assert!(validate(&cert).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn ancestor_path_becoming_a_symlink_rejects_without_unrelated_directory_churn() {
        let (root, mut cert) = isolated();
        let ancestor = root.join("ancestor");
        fs::create_dir(&ancestor).unwrap();
        cert["pathLinks"] = json!([{"path":ancestor,"target":null}]);
        validate(&cert).unwrap();
        fs::write(ancestor.join("unrelated"), b"x").unwrap();
        validate(&cert).unwrap();
        fs::rename(&ancestor, root.join("replacement")).unwrap();
        std::os::unix::fs::symlink("replacement", &ancestor).unwrap();
        assert!(validate(&cert).is_err());
        cert["pathLinks"][0]["target"] = json!("replacement");
        validate(&cert).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
