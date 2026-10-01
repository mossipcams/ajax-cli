//! Bounded stdio transport. Nonblocking pipes make shutdown and writes cancellable.
use super::{JudgeError, LayaCommand, POLL};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
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
            .stderr(Stdio::null())
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
        };
        let stdin = sidecar.child.stdin.take().ok_or(JudgeError::Unavailable)?;
        let stdout = sidecar.child.stdout.take().ok_or(JudgeError::Unavailable)?;
        for result in [
            fcntl(&stdin, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
            fcntl(&stdout, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)),
        ] {
            result.map_err(|_| JudgeError::Unavailable)?;
        }
        sidecar.stdin = Some(stdin);
        let reader_stop = sidecar.reader_stop.clone();
        sidecar.reader = Some(
            thread::Builder::new()
                .name("ajax-laya-reader".into())
                .spawn(move || read_lines(stdout, tx, reader_stop))
                .map_err(|_| JudgeError::Unavailable)?,
        );
        Ok(sidecar)
    }

    pub(super) fn idle(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
            && matches!(self.lines.try_recv(), Err(mpsc::TryRecvError::Empty))
    }

    pub(super) fn exchange(
        &mut self,
        id: u64,
        snapshot: Value,
        deadline: Instant,
    ) -> Result<Value, JudgeError> {
        let mut bytes = serde_json::to_vec(&json!({"id": id, "snapshot": snapshot}))
            .map_err(|_| JudgeError::Malformed)?;
        bytes.push(b'\n');
        let stdin = self.stdin.as_mut().ok_or(JudgeError::Unavailable)?;
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            check_deadline(&self.stop, deadline)?;
            match stdin.write(remaining) {
                Ok(0) => return Err(JudgeError::Unavailable),
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(JudgeError::Unavailable),
            }
        }
        self.receive(deadline)
    }

    pub(super) fn receive(&self, deadline: Instant) -> Result<Value, JudgeError> {
        loop {
            check_deadline(&self.stop, deadline)?;
            match self
                .lines
                .recv_timeout(POLL.min(deadline.saturating_duration_since(Instant::now())))
            {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(JudgeError::Unavailable),
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
    while !stop.load(Ordering::Acquire) {
        match stdout.read(&mut buffer) {
            Ok(0) => {
                let error = if line.is_empty() {
                    JudgeError::Unavailable
                } else {
                    JudgeError::Malformed
                };
                let _ = tx.try_send(Err(error));
                return;
            }
            Ok(count) => {
                for byte in &buffer[..count] {
                    line.push(*byte);
                    if line.len() > MAX_REPLY_BYTES {
                        let _ = tx.try_send(Err(JudgeError::Malformed));
                        return;
                    }
                    if *byte == b'\n' {
                        let result =
                            serde_json::from_slice(&line).map_err(|_| JudgeError::Malformed);
                        let malformed = result.is_err();
                        if tx.try_send(result).is_err() || malformed {
                            return;
                        }
                        line.clear();
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => thread::sleep(POLL),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => {
                let _ = tx.try_send(Err(JudgeError::Unavailable));
                return;
            }
        }
    }
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
