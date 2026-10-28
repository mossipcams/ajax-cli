#![allow(dead_code)]

use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::adapters::session_shim::shim_spawn;

const STDERR_TAIL_BYTES: usize = 4 * 1024;

const DROP_GRACE: Duration = Duration::from_secs(2);

const MAX_RECORD_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub enum JsonlRecord {
    Record(Value),
    Error(String),
    Exited,
}

pub struct JsonlProcess {
    stdin: Option<Box<dyn Write + Send>>,
    records: mpsc::Receiver<JsonlRecord>,
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    child: Option<Child>,
    pid: u32,
    next_id: u64,
}

impl JsonlProcess {
    pub fn spawn(program: &Path, args: &[String], cwd: &Path) -> io::Result<Self> {
        let (program, args) = crate::adapters::program::sandboxed_program(program, args, cwd)?;
        let command = piped_command(&program, &args, cwd);
        if let Some(shim) = shim_spawn(&command)? {
            let writer = shim.stream.try_clone()?;
            return Self::from_parts(shim.child, Box::new(writer), Box::new(shim.stream), None);
        }
        Self::spawn_command(command)
    }

    pub fn spawn_unsandboxed(program: &Path, args: &[String], cwd: &Path) -> io::Result<Self> {
        Self::spawn_command(piped_command(program, args, cwd))
    }

    fn spawn_command(mut command: Command) -> io::Result<Self> {
        super::process_group::own_group(&mut command);
        let mut child = command.spawn()?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");

        Self::from_parts(
            child,
            Box::new(stdin),
            Box::new(stdout),
            Some(Box::new(stderr)),
        )
    }

    pub(crate) fn from_parts(
        child: Child,
        stdin: Box<dyn Write + Send>,
        stdout: Box<dyn Read + Send>,
        stderr: Option<Box<dyn Read + Send>>,
    ) -> io::Result<Self> {
        let pid = child.id();
        Self::start(Some(child), pid, stdin, stdout, stderr)
    }

    pub(crate) fn attach(
        reader: Box<dyn Read + Send>,
        writer: Box<dyn Write + Send>,
        shim_pid: u32,
    ) -> io::Result<Self> {
        Self::start(None, shim_pid, writer, reader, None)
    }

    fn start(
        child: Option<Child>,
        pid: u32,
        stdin: Box<dyn Write + Send>,
        stdout: Box<dyn Read + Send>,
        stderr: Option<Box<dyn Read + Send>>,
    ) -> io::Result<Self> {
        let (sender, records) = mpsc::channel::<JsonlRecord>();
        std::thread::Builder::new()
            .name("jsonl-stdout".to_owned())
            .spawn(move || {
                read_stdout_lines(stdout, &sender);
            })?;

        let stderr_tail: Arc<Mutex<Vec<u8>>> = Arc::default();
        if let Some(stderr) = stderr {
            let tail_guard = Arc::clone(&stderr_tail);
            std::thread::Builder::new()
                .name("jsonl-stderr".to_owned())
                .spawn(move || {
                    drain_stderr(stderr, &tail_guard);
                })?;
        }

        Ok(Self {
            stdin: Some(stdin),
            records,
            stderr_tail,
            child,
            pid,
            next_id: 0,
        })
    }

    pub fn child_id(&self) -> u32 {
        self.pid
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn recv(&self) -> Result<JsonlRecord, mpsc::RecvError> {
        self.records.recv()
    }

    pub fn try_recv(&self) -> Result<JsonlRecord, mpsc::TryRecvError> {
        self.records.try_recv()
    }

    pub fn stderr_tail(&self) -> String {
        let tail = self.stderr_tail.lock().expect("stderr tail lock poisoned");
        String::from_utf8_lossy(&tail).into_owned()
    }

    pub fn send(&mut self, command_type: &str, fields: Value) -> Result<String, String> {
        let mut object = Self::command_fields(fields)?;

        self.next_id += 1;
        let id = format!("ajax-rpc-{}", self.next_id);
        object.insert("type".to_owned(), Value::String(command_type.to_owned()));
        object.insert("id".to_owned(), Value::String(id.clone()));

        self.write_record(object)?;
        Ok(id)
    }

    pub fn send_with_id(
        &mut self,
        command_type: &str,
        id: &str,
        fields: Value,
    ) -> Result<(), String> {
        let mut object = Self::command_fields(fields)?;

        object.insert("type".to_owned(), Value::String(command_type.to_owned()));
        object.insert("id".to_owned(), Value::String(id.to_owned()));

        self.write_record(object)
    }

    fn command_fields(fields: Value) -> Result<serde_json::Map<String, Value>, String> {
        match fields {
            Value::Object(extra) => Ok(extra),
            Value::Null => Ok(serde_json::Map::new()),
            other => Err(format!(
                "fields must be a JSON object, got {}",
                value_kind(&other)
            )),
        }
    }

    fn write_record(&mut self, object: serde_json::Map<String, Value>) -> Result<(), String> {
        let mut line = serde_json::to_string(&Value::Object(object))
            .map_err(|error| format!("serialize command: {error}"))?;
        line.push('\n');

        let stdin = match self.stdin.as_mut() {
            Some(stdin) => stdin,
            None => return Err("child stdin is already closed".to_owned()),
        };
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|error| format!("write to child stdin: {error}"))?;

        Ok(())
    }

    pub fn close_stdin(&mut self) {
        self.stdin.take();
    }
}

impl Drop for JsonlProcess {
    fn drop(&mut self) {
        let Some(child) = self.child.as_mut() else {
            if let Some(stdin) = self.stdin.as_mut() {
                let _ = stdin.write_all(b"SHUTDOWN\n").and_then(|()| stdin.flush());
            }
            return;
        };
        let pid = self.pid;
        self.stdin.take();

        let deadline = Instant::now() + DROP_GRACE;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    super::process_group::kill_group(pid);
                    return;
                }
                Ok(None) => {}
                Err(_) => break,
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        super::process_group::kill_group(pid);
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn read_stdout_lines(stdout: impl Read + Send, sender: &mpsc::Sender<JsonlRecord>) {
    read_stdout_lines_bounded(stdout, sender, MAX_RECORD_BYTES);
}

pub(super) fn read_stdout_lines_bounded(
    stdout: impl Read + Send,
    sender: &mpsc::Sender<JsonlRecord>,
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
            Ok(0) => break,
            Ok(_) if bytes.last() == Some(&b'\n') || bytes.len() <= max_bytes => {
                let mut line: &[u8] = bytes.as_slice();
                if line.last() == Some(&b'\n') {
                    line = &line[..line.len() - 1];
                }
                if line.last() == Some(&b'\r') {
                    line = &line[..line.len() - 1];
                }
                let text = String::from_utf8_lossy(line);

                let record = match serde_json::from_str::<Value>(&text) {
                    Ok(value) => JsonlRecord::Record(value),
                    Err(_) => JsonlRecord::Error(text.into_owned()),
                };
                if sender.send(record).is_err() {
                    break;
                }
            }
            Ok(_) => {
                let oversized = format!("jsonl line exceeds {max_bytes} bytes; discarded");
                if sender.send(JsonlRecord::Error(oversized)).is_err() {
                    break;
                }
                let drained_to_eof = loop {
                    match reader.fill_buf() {
                        Ok(buf) => {
                            if buf.is_empty() {
                                break true;
                            }
                            if let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                                reader.consume(pos + 1);
                                break false;
                            }
                            let len = buf.len();
                            reader.consume(len);
                        }
                        Err(_) => break true,
                    }
                };
                if drained_to_eof {
                    break;
                }
            }
            Err(_) => break,
        }
    }

    let _ = sender.send(JsonlRecord::Exited);
}

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
            Err(_) => break,
        }
    }
}

fn piped_command(program: &Path, args: &[String], cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
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
