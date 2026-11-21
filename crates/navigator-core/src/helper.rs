use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

pub struct Helper {
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<String, String>>,
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
        let mut child = Command::new(path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("start explicitly selected helper {path}"))?;
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                if tx.send(line.map_err(|e| e.to_string())).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            next_id: 1,
            peak_rss_bytes: 0,
            memory_limit_mb,
            timeout_ms,
        })
    }
    pub fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"id":id,"method":method,"params":params})
        )?;
        self.input.flush()?;
        let start = Instant::now();
        loop {
            let used = tree_rss(self.child.id());
            self.peak_rss_bytes = self.peak_rss_bytes.max(used);
            if self.memory_limit_mb > 0 && used > self.memory_limit_mb * 1024 * 1024 {
                self.stop();
                bail!("Explicit semantic helper exceeded sampled {} MiB RSS budget; no answer was returned",self.memory_limit_mb)
            }
            if start.elapsed() > Duration::from_millis(self.timeout_ms) {
                self.stop();
                bail!("Explicit helper exceeded {} ms timeout", self.timeout_ms)
            }
            match self.output.recv_timeout(Duration::from_millis(20)) {
                Ok(Ok(line)) => {
                    let reply: Value = serde_json::from_str(&line)
                        .with_context(|| "Helper emitted non-JSON stdout")?;
                    if reply.get("id") == Some(&json!(id)) {
                        if let Some(err) = reply.get("error") {
                            bail!("Semantic helper: {}", err)
                        }
                        return Ok(reply.get("result").cloned().unwrap_or(Value::Null));
                    }
                }
                Ok(Err(e)) => bail!("Helper output failed: {e}"),
                Err(mpsc::RecvTimeoutError::Disconnected) => bail!("Helper exited without a reply"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
    pub fn stop(&mut self) {
        // Kill descendants as well: project evaluation may have spawned MSBuild.
        fn kill_descendants(pid: u32) {
            let children = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
                .unwrap_or_default();
            for child in children
                .split_whitespace()
                .filter_map(|x| x.parse::<u32>().ok())
            {
                kill_descendants(child);
                let _ = Command::new("kill")
                    .arg("-KILL")
                    .arg(child.to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            }
        }
        kill_descendants(self.child.id());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for Helper {
    fn drop(&mut self) {
        self.stop()
    }
}
