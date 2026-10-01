//! Bounded stdio transport. Nonblocking pipes make shutdown and writes cancellable.
use super::{parse_reply, JudgeError, LayaCommand, Verdict, POLL};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    process::{Child, ChildStderr, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

const MAX_REPLY_BYTES: usize = 4096;

pub(super) struct Sidecar {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<Result<Value, JudgeError>>,
    reader: Option<JoinHandle<()>>,
    reader_stop: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    stderr: Option<ChildStderr>,
    diagnostics: Vec<u8>,
    error_message: String,
    malformed: u8,
    retired: bool,
    pending_write: Vec<u8>,
}

impl Sidecar {
    pub(super) fn spawn(command: &LayaCommand, stop: Arc<AtomicBool>) -> Result<Self, JudgeError> {
        // Match STT's direct argv spawning, with arrays for paths containing spaces.
        // An absolute script path or a command on PATH works in installed builds.
        let args = match command {
            LayaCommand::String(value) => value.split_whitespace().map(str::to_owned).collect(),
            LayaCommand::Argv(value) => value.clone(),
        };
        let (program, args) = args.split_first().ok_or(JudgeError::Unavailable)?;
        let child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| JudgeError::Unavailable)?;
        let (tx, lines) = mpsc::sync_channel(2);
        let mut sidecar = Self {
            child,
            stdin: None,
            lines,
            reader: None,
            reader_stop: Arc::new(AtomicBool::new(false)),
            stop,
            stderr: None,
            diagnostics: Vec::new(),
            error_message: String::new(),
            malformed: 0,
            retired: false,
            pending_write: Vec::new(),
        };
        let stdin = sidecar.child.stdin.take().ok_or(JudgeError::Unavailable)?;
        let stdout = sidecar.child.stdout.take().ok_or(JudgeError::Unavailable)?;
        let stderr = sidecar.child.stderr.take().ok_or(JudgeError::Unavailable)?;
        for result in [
            fcntl(&stdin, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
            fcntl(&stdout, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
            fcntl(&stderr, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
        ] {
            result.map_err(|_| JudgeError::Unavailable)?;
        }
        sidecar.stdin = Some(stdin);
        sidecar.stderr = Some(stderr);
        let reader_stop = sidecar.reader_stop.clone();
        sidecar.reader = Some(
            thread::Builder::new()
                .name("ajax-laya-reader".into())
                .spawn(move || read_lines(stdout, tx, reader_stop))
                .map_err(|_| JudgeError::Unavailable)?,
        );
        Ok(sidecar)
    }

    pub(super) fn healthy(&mut self) -> bool {
        !self.retired && matches!(self.child.try_wait(), Ok(None))
    }

    pub(super) fn idle(&mut self) -> bool {
        self.drain_stderr();
        // Late replies belong to expired requests, including while idle.
        for _ in 0..16 {
            let Ok(message) = self.lines.try_recv() else {
                break;
            };
            let _ = self.message(message);
            if self.retired {
                break;
            }
        }
        self.healthy()
    }

    fn note_malformed(&mut self) -> JudgeError {
        self.malformed += 1;
        self.retired = self.malformed >= 2;
        JudgeError::Malformed
    }

    fn message(&mut self, message: Result<Value, JudgeError>) -> Result<Option<Value>, JudgeError> {
        match message {
            Ok(value) if value["type"] == "error" => {
                if let Some(message) = value["message"].as_str() {
                    self.error_message = message.chars().take(MAX_REPLY_BYTES).collect();
                }
                Ok(None)
            }
            Ok(value) => Ok(Some(value)),
            Err(JudgeError::Malformed) => Err(self.note_malformed()),
            Err(error) => {
                self.retired = true;
                Err(error)
            }
        }
    }

    pub(super) fn diagnostics(&mut self) -> String {
        self.drain_stderr();
        format!(
            "{}\n{}",
            std::mem::take(&mut self.error_message),
            String::from_utf8_lossy(&std::mem::take(&mut self.diagnostics))
        )
    }

    fn drain_stderr(&mut self) {
        let Some(stderr) = self.stderr.as_mut() else {
            return;
        };
        let mut buffer = [0; 1024];
        // Bound work as well as memory if a broken child continuously writes.
        for _ in 0..16 {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    self.diagnostics.extend_from_slice(&buffer[..count]);
                    if self.diagnostics.len() > MAX_REPLY_BYTES {
                        self.diagnostics
                            .drain(..self.diagnostics.len() - MAX_REPLY_BYTES);
                    }
                }
            }
        }
    }

    fn flush_request(&mut self, deadline: Instant) -> Result<(), JudgeError> {
        while !self.pending_write.is_empty() {
            check_deadline(&self.stop, deadline)?;
            self.drain_stderr();
            match self
                .stdin
                .as_mut()
                .ok_or(JudgeError::Unavailable)?
                .write(&self.pending_write)
            {
                Ok(0) => {
                    self.retired = true;
                    return Err(JudgeError::Unavailable);
                }
                Ok(count) => {
                    self.pending_write.drain(..count);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => {
                    self.retired = true;
                    return Err(JudgeError::Unavailable);
                }
            }
        }
        Ok(())
    }

    pub(super) fn exchange(&mut self, id: u64, snapshot: Value, deadline: Instant) -> Verdict {
        // Complete a timed-out partial write before starting the next JSON line.
        self.flush_request(deadline)?;
        self.pending_write = serde_json::to_vec(&json!({"id": id, "snapshot": snapshot}))
            .map_err(|_| JudgeError::Malformed)?;
        self.pending_write.push(b'\n');
        self.flush_request(deadline)?;
        loop {
            match self.receive(deadline) {
                Ok(value) => {
                    if value
                        .get("id")
                        .and_then(Value::as_u64)
                        .is_some_and(|reply_id| reply_id != id)
                    {
                        continue;
                    }
                    match parse_reply(value, id) {
                        Ok(verdict) => {
                            self.malformed = 0;
                            return Ok(verdict);
                        }
                        Err(_) => {
                            self.note_malformed();
                        }
                    }
                }
                Err(JudgeError::Malformed) => {}
                Err(JudgeError::Timeout) if self.malformed > 0 => {
                    return Err(JudgeError::Malformed)
                }
                Err(error) => return Err(error),
            }
            if self.retired {
                return Err(JudgeError::Malformed);
            }
        }
    }

    pub(super) fn receive(&mut self, deadline: Instant) -> Result<Value, JudgeError> {
        loop {
            check_deadline(&self.stop, deadline)?;
            self.drain_stderr();
            match self
                .lines
                .recv_timeout(POLL.min(deadline.saturating_duration_since(Instant::now())))
            {
                Ok(message) => {
                    if let Some(value) = self.message(message)? {
                        return Ok(value);
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.retired = true;
                    return Err(JudgeError::Unavailable);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

fn check_deadline(stop: &AtomicBool, deadline: Instant) -> Result<(), JudgeError> {
    if stop.load(Ordering::Acquire) {
        return Err(JudgeError::Unavailable);
    }
    if Instant::now() >= deadline {
        return Err(JudgeError::Timeout);
    }
    Ok(())
}

fn read_lines(
    mut stdout: impl Read,
    tx: mpsc::SyncSender<Result<Value, JudgeError>>,
    stop: Arc<AtomicBool>,
) {
    let mut line = Vec::new();
    let mut buffer = [0; 1024];
    let mut oversized = false;
    while !stop.load(Ordering::Acquire) {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                let error = if line.is_empty() {
                    JudgeError::Unavailable
                } else {
                    JudgeError::Malformed
                };
                let _ = send_line(&tx, Err(error), &stop);
                return;
            }
            Ok(count) => {
                for byte in &buffer[..count] {
                    if oversized {
                        if *byte == b'\n' {
                            oversized = false;
                        }
                        continue;
                    }
                    line.push(*byte);
                    if line.len() > MAX_REPLY_BYTES {
                        if !send_line(&tx, Err(JudgeError::Malformed), &stop) {
                            return;
                        }
                        line.clear();
                        oversized = *byte != b'\n';
                    } else if *byte == b'\n' {
                        let result =
                            serde_json::from_slice(&line).map_err(|_| JudgeError::Malformed);
                        if !send_line(&tx, result, &stop) {
                            return;
                        }
                        line.clear();
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                let _ = send_line(&tx, Err(JudgeError::Unavailable), &stop);
                return;
            }
        }
    }
}

fn send_line(
    tx: &mpsc::SyncSender<Result<Value, JudgeError>>,
    mut message: Result<Value, JudgeError>,
    stop: &AtomicBool,
) -> bool {
    while !stop.load(Ordering::Acquire) {
        match tx.try_send(message) {
            Ok(()) => return true,
            Err(mpsc::TrySendError::Full(value)) => {
                message = value;
                thread::sleep(POLL);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => return false,
        }
    }
    false
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        self.reader_stop.store(true, Ordering::Release);
        self.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn diagnostics_include_protocol_error_and_stderr() {
        let command = LayaCommand::Argv(vec!["python3".into(), "-c".into(),
            "import sys; print('missing laya stderr', file=sys.stderr); print('{\"type\":\"error\",\"message\":\"missing laya protocol\"}')".into()]);
        let mut child = Sidecar::spawn(&command, Arc::new(AtomicBool::new(false))).unwrap();
        assert_eq!(
            child.receive(Instant::now() + Duration::from_secs(2)),
            Err(JudgeError::Unavailable)
        );
        let diagnostics = child.diagnostics();
        assert!(diagnostics.contains("missing laya stderr"), "{diagnostics}");
        assert!(
            diagnostics.contains("missing laya protocol"),
            "{diagnostics}"
        );
    }
}
