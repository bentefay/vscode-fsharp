use navigator_core::{
    cancellation::{CancellationToken, Cancelled},
    Workspace,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
};

const MAX_REQUEST_BYTES: usize = 12 * 1024 * 1024;
const MAX_ADMITTED_BYTES: usize = 16 * 1024 * 1024;
const MAX_ADMITTED_REQUESTS: usize = 32;
type Output = Arc<Mutex<Box<dyn Write + Send>>>;
#[derive(Default)]
struct Admission {
    bytes: usize,
    count: usize,
    tokens: HashMap<String, CancellationToken>,
}
struct Request {
    id: Option<Value>,
    method: String,
    params: Value,
    token: CancellationToken,
    key: Option<String>,
    wire_bytes: usize,
}
fn emit(output: &Output, value: Value) -> anyhow::Result<()> {
    let mut writer = output
        .lock()
        .map_err(|_| anyhow::anyhow!("Output lock poisoned"))?;
    writeln!(writer, "{value}")?;
    writer.flush()?;
    Ok(())
}
fn reject(output: &Output, id: Option<Value>, code: &str, message: &str) -> anyhow::Result<()> {
    emit(
        output,
        json!({"id":id,"error":{"code":code,"message":message}}),
    )
}
enum Line {
    Data(Vec<u8>),
    Oversized,
}
// Bound line allocation before parsing. This bounds wire bytes, not process RSS.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<Line>> {
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let buffer = reader.fill_buf()?;
        if buffer.is_empty() {
            return Ok(if oversized {
                Some(Line::Oversized)
            } else if line.is_empty() {
                None
            } else {
                Some(Line::Data(line))
            });
        }
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(buffer.len(), |at| at + 1);
        if !oversized {
            if line.len().saturating_add(count) > MAX_REQUEST_BYTES {
                oversized = true;
                line.clear();
            } else {
                line.extend_from_slice(&buffer[..count]);
            }
        }
        reader.consume(count);
        if newline.is_some() {
            return Ok(Some(if oversized {
                Line::Oversized
            } else {
                Line::Data(line)
            }));
        }
    }
}
fn id_key(id: &Value) -> Option<String> {
    match id {
        Value::Number(_) => Some(id.to_string()),
        Value::String(text) if text.len() <= 128 => Some(id.to_string()),
        _ => None,
    }
}
fn release(admission: &Arc<Mutex<Admission>>, request: &Request) {
    let mut state = admission.lock().unwrap();
    release_locked(&mut state, request);
}
fn release_locked(state: &mut Admission, request: &Request) {
    state.bytes -= request.wire_bytes;
    state.count -= 1;
    if let Some(key) = &request.key {
        state.tokens.remove(key);
    }
}
fn read_requests(
    mut reader: impl BufRead,
    sender: SyncSender<Request>,
    admission: Arc<Mutex<Admission>>,
    output: Output,
) -> anyhow::Result<()> {
    while let Some(line) = read_line(&mut reader)? {
        let line = match line {
            Line::Data(line) if line.iter().all(u8::is_ascii_whitespace) => continue,
            Line::Data(line) => line,
            Line::Oversized => {
                reject(&output, None, "requestTooLarge", "Request exceeds 12 MiB")?;
                continue;
            }
        };
        let wire_bytes = line.len();
        let value: Value = match serde_json::from_slice(&line) {
            Ok(value) => value,
            Err(error) => {
                reject(&output, None, "invalidJson", &error.to_string())?;
                continue;
            }
        };
        let Value::Object(mut object) = value else {
            reject(&output, None, "invalidRequest", "Expected a request object")?;
            continue;
        };
        let id = object.remove("id").filter(|id| !id.is_null());
        let key = id.as_ref().and_then(id_key);
        if id.is_some() && key.is_none() {
            reject(
                &output,
                id,
                "invalidRequest",
                "Request ID must be a number or a string of at most 128 bytes",
            )?;
            continue;
        }
        let method = object
            .remove("method")
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        let params = object.remove("params").unwrap_or(json!({}));
        if method == "$/cancelRequest" {
            let cancelled = params
                .get("id")
                .and_then(id_key)
                .map(|key| {
                    let state = admission.lock().unwrap();
                    state.tokens.get(&key).is_some_and(|token| {
                        token.cancel();
                        true
                    })
                })
                .unwrap_or(false);
            if let Some(id) = id {
                emit(&output, json!({"id":id,"result":{"cancelled":cancelled}}))?;
            }
            continue;
        }
        let token = CancellationToken::new();
        let mut state = admission.lock().unwrap();
        if key
            .as_ref()
            .is_some_and(|key| state.tokens.contains_key(key))
        {
            drop(state);
            reject(
                &output,
                id,
                "duplicateRequest",
                "Request ID is already active",
            )?;
            continue;
        }
        if state.count >= MAX_ADMITTED_REQUESTS
            || state.bytes.saturating_add(wire_bytes) > MAX_ADMITTED_BYTES
        {
            drop(state);
            reject(
                &output,
                id,
                "queueFull",
                "Native request queue budget exceeded; no complete result is available",
            )?;
            continue;
        }
        state.count += 1;
        state.bytes += wire_bytes;
        if let Some(key) = &key {
            state.tokens.insert(key.clone(), token.clone());
        }
        drop(state);
        let request = Request {
            id,
            method,
            params,
            token,
            key,
            wire_bytes,
        };
        // Blocking here would hide cancellation behind a saturated normal queue.
        if let Err(error) = sender.try_send(request) {
            let request = match error {
                mpsc::TrySendError::Full(request) | mpsc::TrySendError::Disconnected(request) => {
                    request
                }
            };
            release(&admission, &request);
            reject(
                &output,
                request.id,
                "queueFull",
                "Native request queue is unavailable",
            )?;
        }
    }
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--version") {
        println!("fsharp-navigator {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("metadata") {
        anyhow::ensure!(args.len() == 5 && args[1] == "prepare",
            "Usage: fsharp-navigator metadata prepare <catalog|environment> <source.json> <output.fnmeta>");
        let kind = match args[2].as_str() {
            "catalog" => navigator_core::prepared::Kind::Catalog,
            "environment" => navigator_core::prepared::Kind::Environment,
            _ => anyhow::bail!("Prepared metadata kind must be catalog or environment"),
        };
        let result = navigator_core::prepared::prepare_file(&args[3], &args[4], kind)?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if !args.is_empty() && !matches!(args[0].as_str(), "daemon" | "serve") {
        anyhow::bail!("Usage: fsharp-navigator daemon (JSON-lines stdin/stdout)")
    }
    let output: Output = Arc::new(Mutex::new(Box::new(io::BufWriter::new(io::stdout()))));
    let admission = Arc::new(Mutex::new(Admission::default()));
    let (sender, receiver) = mpsc::sync_channel(MAX_ADMITTED_REQUESTS);
    let reader_output = output.clone();
    let reader_admission = admission.clone();
    std::thread::spawn(move || {
        if let Err(error) =
            read_requests(io::stdin().lock(), sender, reader_admission, reader_output)
        {
            eprintln!("Input transport: {error:#}");
        }
    });
    let mut workspace: Option<Workspace> = None;
    for request in receiver {
        let _scope = request.token.enter();
        let result = request.token.check().and_then(|()| {
            if request.method == "workspace/open" {
                Workspace::open(&request.params).and_then(|mut ws| {
                    ws.set_cancellation(request.token.clone());
                    let result = ws.stats()?;
                    workspace = Some(ws);
                    Ok(result)
                })
            } else if request.method == "shutdown" {
                workspace = None;
                Ok(json!({"shutdown":true}))
            } else if let Some(ws) = workspace.as_mut() {
                ws.set_cancellation(request.token.clone());
                ws.dispatch(&request.method, &request.params)
            } else {
                Err(anyhow::anyhow!("Call workspace/open first"))
            }
        });
        // Cancellation racing with completion must not be published as exact.
        let result = {
            let mut state = admission.lock().unwrap();
            let result = result.and_then(|value| request.token.check().map(|()| value));
            // The same lock serializes cancellation acceptance and completion.
            // After this point the operation is complete, even if stdout waits.
            release_locked(&mut state, &request);
            result
        };
        if let Some(id) = &request.id {
            let response = match result {
                Ok(value) => json!({"id":id,"result":value}),
                Err(error) => {
                    json!({"id":id,"error":{"code":if error.is::<Cancelled>() {"cancelled"} else {"requestFailed"},"message":format!("{error:#}")}})
                }
            };
            emit(&output, response)?;
        } else if let Err(error) = result {
            eprintln!("{}: {error:#}", request.method);
        }
        if request.method == "shutdown" {
            break;
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);
    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn saturated_queue_still_reads_cancellation_and_rejects_excess_requests() {
        let mut input = String::new();
        for id in 0..MAX_ADMITTED_REQUESTS + 2 {
            input += &format!("{{\"id\":{id},\"method\":\"workspace/stats\"}}\n");
        }
        input += "{\"method\":\"$/cancelRequest\",\"params\":{\"id\":0}}\n";
        let (sender, receiver) = mpsc::sync_channel(MAX_ADMITTED_REQUESTS);
        let admission = Arc::new(Mutex::new(Admission::default()));
        let capture = Capture::default();
        let output: Output = Arc::new(Mutex::new(Box::new(capture.clone())));
        read_requests(io::Cursor::new(input), sender, admission.clone(), output).unwrap();
        assert_eq!(admission.lock().unwrap().count, MAX_ADMITTED_REQUESTS);
        assert!(receiver.try_recv().unwrap().token.is_cancelled());
        let errors = String::from_utf8(capture.0.lock().unwrap().clone()).unwrap();
        assert_eq!(errors.lines().count(), 2);
        assert!(errors
            .lines()
            .all(
                |line| serde_json::from_str::<Value>(line).unwrap()["error"]["code"] == "queueFull"
            ));
    }
    #[test]
    fn oversized_line_does_not_consume_the_following_request() {
        let mut bytes = vec![b'x'; MAX_REQUEST_BYTES + 1];
        bytes.extend_from_slice(b"\n{}\n");
        let mut reader = io::Cursor::new(bytes);
        assert!(matches!(
            read_line(&mut reader).unwrap(),
            Some(Line::Oversized)
        ));
        assert!(
            matches!(read_line(&mut reader).unwrap(), Some(Line::Data(value)) if value == b"{}\n")
        );
        assert!(read_line(&mut reader).unwrap().is_none());
    }
    #[test]
    fn aggregate_byte_budget_rejects_large_requests_before_count_limit() {
        let text = "x".repeat(9 * 1024 * 1024);
        let input = format!(
            "{}\n{}\n",
            json!({"id":1,"method":"document/open","params":{"text":text}}),
            json!({"id":2,"method":"document/open","params":{"text":text}})
        );
        let (sender, receiver) = mpsc::sync_channel(MAX_ADMITTED_REQUESTS);
        let admission = Arc::new(Mutex::new(Admission::default()));
        let capture = Capture::default();
        let output: Output = Arc::new(Mutex::new(Box::new(capture.clone())));
        read_requests(io::Cursor::new(input), sender, admission.clone(), output).unwrap();
        assert_eq!(admission.lock().unwrap().count, 1);
        assert!(admission.lock().unwrap().bytes <= MAX_ADMITTED_BYTES);
        let request = receiver.try_recv().unwrap();
        release(&admission, &request);
        assert_eq!(admission.lock().unwrap().bytes, 0);
        let error: Value = serde_json::from_slice(&capture.0.lock().unwrap()).unwrap();
        assert_eq!(error["id"], 2);
        assert_eq!(error["error"]["code"], "queueFull");
    }
}
