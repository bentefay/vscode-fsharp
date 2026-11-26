//! Explicit experimental helper transport. Budgets cover both pipes and waiting
//! for a helper that never reads stdin; these are not a hard native RSS limit.
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

const MAX_MESSAGE_BYTES: usize = 12 * 1024 * 1024;
type Reply = Result<Vec<u8>, String>;
pub struct Helper {
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    output: Option<Receiver<Reply>>,
    written: Option<Receiver<Result<(), String>>>,
    stopped: bool,
    next_id: u64,
    pub peak_rss_bytes: u64,
    pub memory_limit_mb: u64,
    pub timeout_ms: u64,
}
pub fn rss(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|x| x.parse::<u64>().ok())
        })
        .unwrap_or(0)
        * 1024
}
pub fn tree_rss(pid: u32) -> u64 {
    let children =
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    rss(pid)
        + children
            .split_whitespace()
            .filter_map(|s| s.parse::<u32>().ok())
            .map(tree_rss)
            .sum::<u64>()
}
impl Helper {
    pub fn spawn(
        path: &str,
        args: &[String],
        memory_limit_mb: u64,
        timeout_ms: u64,
    ) -> Result<Self> {
        #[cfg(not(target_os = "linux"))]
        bail!("Semantic helper process ownership and RSS sampling are currently Linux-only");
        let mut command = Command::new(path);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("start explicitly selected helper {path}"))?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        // At most one queued event plus one bounded line in the reader. A
        // flooding helper encounters pipe backpressure, not an unbounded queue.
        let (events, output) = mpsc::sync_channel(1);
        let (written, writes) = mpsc::sync_channel(1);
        let (input, requests) = mpsc::sync_channel::<Vec<u8>>(1);
        std::thread::spawn(move || {
            while let Ok(bytes) = requests.recv() {
                let result = stdin
                    .write_all(&bytes)
                    .and_then(|()| stdin.flush())
                    .map_err(|e| e.to_string());
                let failed = result.is_err();
                if written.send(result).is_err() || failed {
                    break;
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let result = bounded_line(&mut reader).map_err(|e| e.to_string());
                let failed = result.is_err();
                if events.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input: Some(input),
            output: Some(output),
            written: Some(writes),
            stopped: false,
            next_id: 1,
            peak_rss_bytes: 0,
            memory_limit_mb,
            timeout_ms,
        })
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let result = self.call_inner(method, params);
        if result.is_err() {
            self.stop();
        }
        result
    }
    fn call_inner(&mut self, method: &str, params: Value) -> Result<Value> {
        ensure!(
            !self.stopped,
            "Semantic helper has stopped; restart the engine"
        );
        let start = Instant::now();
        crate::cancellation::check()?;
        let id = self.next_id;
        self.next_id += 1;
        // Serialize through a capped writer: escaped input can be larger than
        // the document budget, and must not allocate an unbounded second copy.
        let mut bytes = LimitedBytes(Vec::new());
        serde_json::to_writer(
            &mut bytes,
            &json!({"id":id,"method":method,"params":params}),
        )?;
        bytes.write_all(b"\n")?;
        self.input
            .as_ref()
            .unwrap()
            .try_send(bytes.0)
            .context("Helper input queue unavailable")?;
        let mut written = false;
        let mut answer = None;
        loop {
            crate::cancellation::check()?;
            if start.elapsed() >= Duration::from_millis(self.timeout_ms) {
                bail!("Explicit helper exceeded {} ms timeout", self.timeout_ms);
            }
            let used = tree_rss(self.child.id());
            self.peak_rss_bytes = self.peak_rss_bytes.max(used);
            if self.memory_limit_mb > 0 && used > self.memory_limit_mb.saturating_mul(1024 * 1024) {
                bail!("Explicit semantic helper exceeded sampled {} MiB RSS budget; no answer was returned", self.memory_limit_mb);
            }
            if !written {
                match self.written.as_ref().unwrap().try_recv() {
                    Ok(result) => {
                        result.map_err(anyhow::Error::msg)?;
                        written = true;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => bail!("Helper input writer exited"),
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if written && answer.is_some() {
                return Ok(answer.take().unwrap());
            }
            if answer.is_some() {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            match self
                .output
                .as_ref()
                .unwrap()
                .recv_timeout(Duration::from_millis(20))
            {
                Ok(result) => {
                    let line = result.map_err(anyhow::Error::msg)?;
                    let reply: Value =
                        serde_json::from_slice(&line).context("Helper emitted non-JSON stdout")?;
                    ensure!(
                        reply.get("id") == Some(&json!(id)),
                        "Helper returned an unexpected response ID"
                    );
                    if let Some(error) = reply.get("error") {
                        bail!("Semantic helper: {error}");
                    }
                    ensure!(answer.is_none(), "Helper returned duplicate replies");
                    answer = Some(reply.get("result").cloned().unwrap_or(Value::Null));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Helper exited without a reply"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if written {
                if let Some(result) = answer.take() {
                    return Ok(result);
                }
            }
        }
    }
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        // Drop queues before killing so blocked sender threads can finish. The
        // child has not been reaped: its PID/process-group ID cannot be reused.
        self.input.take();
        self.output.take();
        self.written.take();
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        self.stop();
    }
}

fn bounded_line(reader: &mut impl BufRead) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf()?;
        ensure!(!buffer.is_empty(), "Helper exited without a complete reply");
        let end = buffer.iter().position(|b| *b == b'\n');
        let count = end.map_or(buffer.len(), |n| n + 1);
        ensure!(
            bytes.len() + count <= MAX_MESSAGE_BYTES,
            "Helper response byte limit exceeded"
        );
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(bytes);
        }
    }
}
struct LimitedBytes(Vec<u8>);
impl Write for LimitedBytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_MESSAGE_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("Helper request byte limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    fn python(source: &str, timeout: u64) -> Helper {
        Helper::spawn(
            "python3",
            &["-u".into(), "-c".into(), source.into()],
            128,
            timeout,
        )
        .unwrap()
    }

    #[test]
    fn deadline_includes_a_helper_that_does_not_read_stdin() {
        // Finite child lifetime also makes this regression terminate before
        // the fix: the old blocking write only returns after the child exits.
        let mut helper = python("import time; time.sleep(2)", 100);
        let started = Instant::now();
        assert!(helper
            .call("probe", json!({"text":"x".repeat(1024 * 1024)}))
            .is_err());
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "stdin write bypassed deadline"
        );
    }

    #[test]
    fn oversized_helper_line_is_rejected() {
        // Keep the deliberate multi-MiB allocation out of parallel parser
        // tests, which intentionally sample the whole test process's RSS.
        if std::env::var_os("FSNAV_ISOLATED_HELPER_TEST").is_none() {
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "helper::tests::oversized_helper_line_is_rejected",
                ])
                .env("FSNAV_ISOLATED_HELPER_TEST", "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let mut helper = python("import sys; sys.stdin.readline(); print('{\"id\":1,\"result\":\"' + 'x' * (13*1024*1024) + '\"}')", 5000);
        let result = helper.call("probe", json!({}));
        assert!(result.is_err(), "Oversized helper reply was accepted");
        let error = result.err().unwrap();
        assert!(error.to_string().contains("byte limit"), "{error:#}");
    }

    #[test]
    fn helper_protocol_remains_reusable_for_valid_replies() {
        let mut helper = python("import sys,json\nfor line in sys.stdin:\n r=json.loads(line); print(json.dumps({'id':r['id'],'result':r['params']}))", 2000);
        for n in 0..3 {
            assert_eq!(
                helper.call("probe", json!({"n":n})).unwrap(),
                json!({"n":n})
            );
        }
    }

    #[test]
    fn cancellation_interrupts_blocked_input() {
        let mut helper = python("import time; time.sleep(2)", 5000);
        let token = crate::cancellation::CancellationToken::new();
        let scope = token.enter();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            token.cancel();
        });
        let started = Instant::now();
        let result = helper.call("probe", json!({"text":"x".repeat(1024*1024)}));
        drop(scope);
        assert!(result.err().unwrap().is::<crate::cancellation::Cancelled>());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn unexpected_output_flood_stops_the_helper() {
        let mut helper = python(
            "import sys\nsys.stdin.readline()\nwhile True: print('{\"id\":99,\"result\":null}')",
            2000,
        );
        let error = helper.call("probe", json!({})).unwrap_err();
        assert!(error.to_string().contains("unexpected response ID"));
        assert!(helper.stopped);
        assert!(helper.child.try_wait().unwrap().is_some());
    }

    #[test]
    fn stopping_helper_kills_its_process_group() {
        let mut helper = python("import sys,json,subprocess,time\nsys.stdin.readline()\np=subprocess.Popen([sys.executable,'-c','import time; time.sleep(30)'])\nprint(json.dumps({'id':1,'result':p.pid}))\ntime.sleep(30)", 2000);
        let pid = helper.call("probe", json!({})).unwrap().as_u64().unwrap();
        helper.stop();
        let path = format!("/proc/{pid}/stat");
        for _ in 0..100 {
            let stopped = std::fs::read_to_string(&path).map_or(true, |s| {
                s.split(") ")
                    .nth(1)
                    .is_some_and(|state| state.starts_with('Z'))
            });
            if stopped {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Helper descendant survived process-group termination");
    }
}
