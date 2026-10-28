use super::log::{inbound_log_path, json_rpc_response_id, ShimLog};
use super::relay::{classify_client_line, parse_client_hello, ClientLine, ClientToken, ShimRelay};
use super::shim_paths;
use crate::adapters::web_session_store::WEB_SESSION_DIR;
use nix::errno::Errno;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::{getpgrp, setsid};
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const STDOUT_DRAIN_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct ShimArgs {
    pub handle: String,
    pub state_dir: PathBuf,
    pub argv: Vec<OsString>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Teardown {
    #[cfg_attr(not(test), allow(dead_code))]
    Child,
    ProcessGroup,
}

pub fn run_session_shim(args: ShimArgs) -> io::Result<i32> {
    match setsid() {
        Ok(_) | Err(Errno::EPERM) => serve(args, Teardown::ProcessGroup),
        Err(error) => Err(io::Error::from(error)),
    }
}

pub(crate) fn serve(args: ShimArgs, teardown: Teardown) -> io::Result<i32> {
    let paths = shim_paths(&args.state_dir, &args.handle);
    fs::create_dir_all(args.state_dir.join(WEB_SESSION_DIR))?;
    let listener = bind_socket(&paths.socket)?;
    let _socket = SocketFile(paths.socket.clone());
    listener.set_nonblocking(true)?;
    fs::write(&paths.log, [])?;
    let log = ShimLog::open(&paths.log)?;
    let relay = ShimRelay::new(log);
    let inbound_path = inbound_log_path(&paths.log);
    fs::write(&inbound_path, [])?;
    let inbound = ShimLog::open(&inbound_path)?;
    let mut child = spawn_harness(&args.argv, &paths.stderr)?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(io::Error::other("harness pipes were not created"));
    };
    let state = Arc::new(ShimState {
        relay,
        inbound: Mutex::new(inbound),
        attached: Mutex::new(None),
        stdin: Mutex::new(Some(stdin)),
        shutdown: AtomicBool::new(false),
    });
    let pump = {
        let state = Arc::clone(&state);
        thread::spawn(move || pump_harness_stdout(stdout, &state))
    };
    let status = match supervise(&mut child, &listener, &state, &paths.socket, teardown) {
        Ok(status) => status,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            close_attached(&state);
            return Err(error);
        }
    };
    let drained = wait_until(|| pump.is_finished(), STDOUT_DRAIN_GRACE);
    close_attached(&state);
    if drained {
        pump.join()
            .map_err(|_| io::Error::other("harness stdout pump panicked"))??;
    }
    Ok(exit_code(status))
}

struct ShimState {
    relay: ShimRelay,
    inbound: Mutex<ShimLog>,
    attached: Mutex<Option<UnixStream>>,
    stdin: Mutex<Option<ChildStdin>>,
    shutdown: AtomicBool,
}

struct SocketFile(PathBuf);

impl Drop for SocketFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn bind_socket(socket: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(socket) {
        Ok(listener) => Ok(listener),
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).is_ok() {
                return Err(io::Error::new(
                    io::ErrorKind::AddrInUse,
                    format!("session shim already serves {}", socket.display()),
                ));
            }
            fs::remove_file(socket)?;
            UnixListener::bind(socket)
        }
        Err(error) => Err(error),
    }
}

fn spawn_harness(argv: &[OsString], stderr: &Path) -> io::Result<Child> {
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "session shim needs a harness command",
        ));
    };
    let stderr = OpenOptions::new().create(true).append(true).open(stderr)?;
    Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(stderr))
        .spawn()
}

fn supervise(
    child: &mut Child,
    listener: &UnixListener,
    state: &Arc<ShimState>,
    socket: &Path,
    teardown: Teardown,
) -> io::Result<ExitStatus> {
    let mut shutdown_requested_at = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        accept_pending(listener, state)?;
        if state.shutdown.load(Ordering::SeqCst) {
            let requested_at = *shutdown_requested_at.get_or_insert_with(Instant::now);
            if requested_at.elapsed() >= SHUTDOWN_GRACE {
                kill_harness(child, socket, teardown)?;
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn kill_harness(child: &mut Child, socket: &Path, teardown: Teardown) -> io::Result<()> {
    match teardown {
        Teardown::Child => child.kill(),
        Teardown::ProcessGroup => {
            let _ = fs::remove_file(socket);
            killpg(getpgrp(), Signal::SIGKILL).map_err(io::Error::from)
        }
    }
}

fn accept_pending(listener: &UnixListener, state: &Arc<ShimState>) -> io::Result<()> {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false)?;
                let state = Arc::clone(state);
                thread::spawn(move || handle_client(stream, &state));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}

fn handle_client(stream: UnixStream, state: &ShimState) -> io::Result<()> {
    let _ = stream.set_read_timeout(Some(HELLO_TIMEOUT));
    let mut reader = BufReader::new(stream.try_clone()?);
    let hello = read_client_line(&mut reader)?.unwrap_or_default();
    let offset = parse_client_hello(&hello).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid session shim hello: {error:?}"),
        )
    })?;
    let _ = stream.set_read_timeout(None);
    let attached = attach(state, &stream, offset);
    let result = forward_client_lines(&mut reader, state);
    if let Ok(token) = attached.as_ref() {
        state.relay.detach(*token);
    }
    result?;
    attached.map(|_| ())
}

fn attach(state: &ShimState, stream: &UnixStream, offset: u64) -> io::Result<ClientToken> {
    let mut attached = locked(&state.attached);
    if let Some(previous) = attached.take() {
        let _ = previous.shutdown(Shutdown::Both);
    }
    let token = state.relay.attach(offset, Box::new(stream.try_clone()?))?;
    *attached = Some(stream.try_clone()?);
    Ok(token)
}

fn forward_client_lines(reader: &mut impl BufRead, state: &ShimState) -> io::Result<()> {
    while let Some(line) = read_client_line(reader)? {
        match classify_client_line(&line) {
            ClientLine::Shutdown => {
                locked(&state.stdin).take();
                state.shutdown.store(true, Ordering::SeqCst);
                return Ok(());
            }
            ClientLine::Inbound(bytes) => {
                write_to_harness(state, &bytes)?;
                if json_rpc_response_id(&bytes).is_some() {
                    locked(&state.inbound).append_line(&bytes)?;
                }
            }
        }
    }
    Ok(())
}

fn write_to_harness(state: &ShimState, line: &[u8]) -> io::Result<()> {
    let mut guard = locked(&state.stdin);
    let stdin = guard
        .as_mut()
        .ok_or_else(|| io::Error::from(io::ErrorKind::BrokenPipe))?;
    stdin.write_all(line)?;
    stdin.write_all(b"\n")?;
    stdin.flush()
}

fn read_client_line(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    if reader.read_until(b'\n', &mut line)? == 0 {
        return Ok(None);
    }
    if line.last() == Some(&b'\n') {
        line.pop();
    }
    Ok(Some(line))
}

fn pump_harness_stdout(stdout: ChildStdout, state: &ShimState) -> io::Result<()> {
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            return Ok(());
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        state.relay.append_and_deliver(&line)?;
    }
}

fn close_attached(state: &ShimState) {
    if let Some(stream) = locked(&state.attached).take() {
        let _ = stream.shutdown(Shutdown::Both);
    }
}

fn wait_until(mut done: impl FnMut() -> bool, timeout: Duration) -> bool {
    let started = Instant::now();
    while !done() {
        if started.elapsed() >= timeout {
            return false;
        }
        thread::sleep(POLL_INTERVAL);
    }
    true
}

fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .or_else(|| status.signal().map(|signal| 128 + signal))
        .unwrap_or(1)
}
