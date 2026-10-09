//! Standalone Pi RPC-mode (`pi --mode rpc`) child-process plumbing for the web
//! session adapter. This module owns only process lifetime and transport: it
//! spawns the child with piped stdio (no shell), streams its stdout as strict
//! LF-delimited JSONL onto an `std::sync::mpsc` channel, keeps a bounded tail
//! of stderr, and writes id-correlated command lines to stdin. It does not
//! interpret records (`pi_rpc_map` owns that) and it does not run a connection
//! loop — the later wiring task builds on this surface.
//!
//! Framing rules: stdout splits on LF only; one optional preceding CR is
//! stripped per line (CRLF children work). U+2028 and U+2029 inside string
//! values are valid JSONL content and must not split a record. A line that is
//! not valid JSON is delivered as [`PiRpcRecord::Error`] rather than dropped.

// Not yet reachable from the crate surface: nothing wires this in until the
// connection-loop task lands, so every public item is dead code for now.
#![allow(dead_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

/// Bounded tail of the child's stderr kept for diagnostics.
const STDERR_TAIL_BYTES: usize = 4 * 1024;

/// How long `Drop` gives an already-closing child before killing it.
const DROP_GRACE: Duration = Duration::from_secs(2);

/// Maximum bytes (not including the terminating LF) one stdout line may hold
/// before the reader discards it as an over-sized record. Bounds the reader's
/// in-flight buffer so a child that never sends an LF cannot grow memory.
const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

/// One item delivered by the stdout reader thread of [`PiRpcProcess`].
#[derive(Debug)]
pub enum PiRpcRecord {
    /// A stdout line that parsed as JSON.
    Record(Value),
    /// A stdout line that was not valid JSON; carries the raw (lossily decoded)
    /// text with the trailing LF (and one optional CR) already stripped.
    Error(String),
    /// The child's stdout reached EOF; delivered exactly once per process.
    Exited,
}

/// A spawned `pi --mode rpc` child with its JSONL transport wiring in place.
pub struct PiRpcProcess {
    // `Option` so `close_stdin` can take (and drop) the handle: `ChildStdin`
    // has no `close()`, dropping it closes the pipe.
    stdin: Option<ChildStdin>,
    records: mpsc::Receiver<PiRpcRecord>,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    child: Child,
    next_id: u64,
}

impl PiRpcProcess {
    /// Spawn `program` with `args` (for the real pi this is `--mode rpc …`) in
    /// `cwd`, piped on all three std streams and without any shell. Stdout
    /// starts feeding [`PiRpcRecord`]s as soon as the child writes lines.
    pub fn spawn(program: &Path, args: &[String], cwd: &Path) -> io::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        let (sender, records) = mpsc::channel::<PiRpcRecord>();
        std::thread::Builder::new()
            .name("pi-rpc-stdout".to_owned())
            .spawn(move || {
                read_stdout_lines(stdout, &sender);
            })?;

        let stderr_tail: Arc<Mutex<Vec<u8>>> = Arc::default();
        let tail_guard = Arc::clone(&stderr_tail);
        std::thread::Builder::new()
            .name("pi-rpc-stderr".to_owned())
            .spawn(move || {
                drain_stderr(stderr, &tail_guard);
            })?;

        Ok(Self {
            stdin: Some(stdin),
            records,
            stderr_tail,
            child,
            next_id: 0,
        })
    }

    /// OS process id of the child; useful for verifying it was reaped.
    pub fn child_id(&self) -> u32 {
        self.child.id()
    }

    /// Wait for the next stdout record. A `RecvError` means the reader thread
    /// finished (EOF already delivered [`PiRpcRecord::Exited`]).
    pub fn recv(&self) -> Result<PiRpcRecord, mpsc::RecvError> {
        self.records.recv()
    }

    /// Poll for the next stdout record without blocking.
    pub fn try_recv(&self) -> Result<PiRpcRecord, mpsc::TryRecvError> {
        self.records.try_recv()
    }

    /// The stderr bytes retained so far (bounded to the last 4 KiB), decoded
    /// lossily. Never mixes into the stdout record channel.
    pub fn stderr_tail(&self) -> String {
        let tail = self.stderr_tail.lock().expect("stderr tail lock poisoned");
        String::from_utf8_lossy(&tail).into_owned()
    }

    /// Write one command: a fresh unique string `id`, `type` set to
    /// `command_type`, plus the extra fields from `fields`, serialized as one
    /// JSON object followed by LF. Returns the id; the caller correlates it
    /// against the child's `{ "type": "response", "id": … }` record itself.
    pub fn send(&mut self, command_type: &str, fields: Value) -> Result<String, String> {
        let mut object = match fields {
            Value::Object(extra) => extra,
            Value::Null => serde_json::Map::new(),
            other => {
                return Err(format!(
                    "fields must be a JSON object, got {}",
                    value_kind(&other)
                ))
            }
        };

        self.next_id += 1;
        let id = format!("ajax-pi-rpc-{}", self.next_id);
        // Ours wins: never let the caller clobber the framing keys.
        object.insert("type".to_owned(), Value::String(command_type.to_owned()));
        object.insert("id".to_owned(), Value::String(id.clone()));

        let mut line = serde_json::to_string(&Value::Object(object))
            .map_err(|error| format!("serialize command: {error}"))?;
        line.push('\n');

        let stdin = match self.stdin.as_mut() {
            Some(stdin) => stdin,
            None => return Err("pi child stdin is already closed".to_owned()),
        };
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("write to pi child stdin: {error}"))?;

        Ok(id)
    }

    /// Close the child's stdin to request orderly shutdown. The real pi exits
    /// cleanly with code 0 once its stdin closes; the reader thread then
    /// delivers [`PiRpcRecord::Exited`] exactly once. Safe to call more than
    /// once: after the first call the stdin handle is simply gone.
    pub fn close_stdin(&mut self) {
        // Dropping the handle closes the pipe; there is no explicit `close()`.
        self.stdin.take();
    }
}

impl Drop for PiRpcProcess {
    fn drop(&mut self) {
        // Close stdin (idempotent), give a child that is already exiting a
        // brief grace period, then kill. `Child::wait` reaps the process in
        // every path, so no zombie survives this struct's lifetime.
        self.stdin.take();

        let deadline = Instant::now() + DROP_GRACE;
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return, // already reaped here
                Ok(None) => {}
                Err(_) => break, // status no longer available; kill and fall through
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let _ = self.child.kill();
        let _ = self.child.wait(); // reaps; errors (e.g. double wait) are fine here
    }
}

/// Stream stdout as LF-delimited JSONL onto `sender`, delivering exactly one
/// [`PiRpcRecord::Exited`] at EOF, then dropping the sender so receivers see
/// a disconnected channel. Line length is bounded by [`MAX_RECORD_BYTES`].
fn read_stdout_lines(stdout: impl Read + Send, sender: &mpsc::Sender<PiRpcRecord>) {
    read_stdout_lines_bounded(stdout, sender, MAX_RECORD_BYTES);
}

/// The bounded form of [`read_stdout_lines`] with an explicit per-line limit,
/// exposed for tests that exercise the over-sized path with a small cap.
///
/// The bound is applied *on the read*: each line is read through
/// `take(max_bytes + 1)`, so the buffer can never grow past one byte more than
/// the limit. A line with no LF within the first `max_bytes + 1` bytes yields
/// exactly one [`PiRpcRecord::Error`]; the remainder of that line is drained
/// from the buffered reader without being buffered, so normal parsing resumes
/// on the next line.
pub(super) fn read_stdout_lines_bounded(
    stdout: impl Read + Send,
    sender: &mpsc::Sender<PiRpcRecord>,
    max_bytes: usize,
) {
    let mut reader = BufReader::new(stdout);
    let mut bytes: Vec<u8> = Vec::new();

    loop {
        bytes.clear();
        match (&mut reader)
            .take(max_bytes as u64 + 1)
            .read_until(b'\n', &mut bytes)
        {
            Ok(0) => break, // EOF: nothing more will arrive on this pipe
            Ok(_) if bytes.last() == Some(&b'\n') || bytes.len() <= max_bytes => {
                // A complete line within the limit — `take` counts the LF in
                // `max_bytes + 1`, so at most `max_bytes` content bytes are
                // read — or a final line at EOF without a trailing LF.
                // Byte-level framing: split on LF only, then strip one optional
                // preceding CR. Decoding with `from_utf8_lossy` means U+2028 /
                // U+2029 and invalid bytes can never break the frame.
                let mut line: &[u8] = bytes.as_slice();
                if line.last() == Some(&b'\n') {
                    line = &line[..line.len() - 1];
                }
                if line.last() == Some(&b'\r') {
                    line = &line[..line.len() - 1];
                }
                let text = String::from_utf8_lossy(line);

                let record = match serde_json::from_str::<Value>(&text) {
                    Ok(value) => PiRpcRecord::Record(value),
                    Err(_) => {
                        // Non-JSON output is a protocol anomaly, not data loss:
                        // deliver it as an Error record for the caller to judge.
                        PiRpcRecord::Error(text.into_owned())
                    }
                };
                if sender.send(record).is_err() {
                    break; // receiver gone: stop draining this pipe
                }
            }
            Ok(_) => {
                // Over-sized: no LF within the first `max_bytes + 1` bytes.
                // Deliver exactly one Error, then discard the rest of this
                // line without buffering it, so the buffer never grows.
                let oversized = format!("pi rpc line exceeds {max_bytes} bytes; discarded");
                if sender.send(PiRpcRecord::Error(oversized)).is_err() {
                    break; // receiver gone: stop draining this pipe
                }
                let drained_to_eof = loop {
                    match reader.fill_buf() {
                        Ok(buf) => {
                            if buf.is_empty() {
                                break true; // EOF while discarding
                            }
                            if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                                reader.consume(pos + 1);
                                break false; // line discarded; resume normal reads
                            }
                            let len = buf.len();
                            reader.consume(len); // no LF in view: consume and keep draining
                        }
                        Err(_) => break true, // read error while discarding: like EOF
                    }
                };
                if drained_to_eof {
                    break;
                }
            }
            Err(_) => break, // underlying read error: treat like EOF below
        }
    }

    // EOF delivers Exited exactly once; if the receiver is already gone this
    // send fails and that is fine.
    let _ = sender.send(PiRpcRecord::Exited);
}

/// Drain the child's stderr into `tail`, retaining only the last
/// [`STDERR_TAIL_BYTES`] bytes. Stderr never enters the record channel.
fn drain_stderr(stderr: impl Read + Send, tail: &Mutex<Vec<u8>>) {
    let mut reader = BufReader::new(stderr);
    let mut chunk = [0_u8; 1024];

    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(len) => {
                let mut kept = tail.lock().expect("stderr tail lock poisoned");
                kept.extend_from_slice(&chunk[..len]);
                let excess = kept.len().saturating_sub(STDERR_TAIL_BYTES);
                if excess > 0 {
                    kept.drain(..excess);
                }
            }
            Err(_) => break, // nothing more to retain once the pipe errors out
        }
    }
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
