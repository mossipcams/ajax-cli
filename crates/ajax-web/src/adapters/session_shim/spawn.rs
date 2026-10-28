use super::process::SHUTDOWN_GRACE;
use super::shim_paths;
use crate::adapters::web_session_acp::process_group::kill_tree;
use std::cell::Cell;
use std::io::{self, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_RETRY: Duration = Duration::from_millis(25);
const RETIRE_TIMEOUT: Duration = Duration::from_secs(SHUTDOWN_GRACE.as_secs() + 3);

struct SpawnConfig {
    exe: PathBuf,
    state_dir: PathBuf,
}

static CONFIG: OnceLock<SpawnConfig> = OnceLock::new();

thread_local! {
    static BYPASS: Cell<bool> = const { Cell::new(false) };
}

pub struct ShimSpawn {
    pub child: Child,
    pub stream: UnixStream,
}

impl ShimSpawn {
    pub(crate) fn into_child_with_pipes(self) -> io::Result<Child> {
        let Self { mut child, stream } = self;
        child.stdin = Some(ChildStdin::from(OwnedFd::from(stream.try_clone()?)));
        child.stdout = Some(ChildStdout::from(OwnedFd::from(stream)));
        Ok(child)
    }
}

pub fn enable(exe: PathBuf, state_dir: PathBuf) {
    let _ = CONFIG.set(SpawnConfig { exe, state_dir });
}

pub fn without_shim<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            BYPASS.with(|bypass| bypass.set(self.0));
        }
    }
    let _restore = Restore(BYPASS.with(|bypass| bypass.replace(true)));
    f()
}

pub fn shim_spawn(command: &Command) -> io::Result<Option<ShimSpawn>> {
    shim_spawn_with(CONFIG.get(), command)
}

fn shim_spawn_with(
    config: Option<&SpawnConfig>,
    command: &Command,
) -> io::Result<Option<ShimSpawn>> {
    let Some(config) = config.filter(|_| !BYPASS.with(Cell::get)) else {
        return Ok(None);
    };
    let cwd = command.get_current_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "session shim needs a harness working directory",
        )
    })?;
    let handle = cwd.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "session shim handle must be valid UTF-8",
        )
    })?;
    let mut shim = Command::new(&config.exe);
    shim.args(["session-shim", "--handle", handle, "--state-dir"])
        .arg(&config.state_dir)
        .arg("--")
        .arg(command.get_program())
        .args(command.get_args())
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => shim.env(key, value),
            None => shim.env_remove(key),
        };
    }
    let socket = shim_paths(&config.state_dir, handle).socket;
    start_shim(&mut shim, &socket).map(Some)
}

pub(super) fn start_shim(shim: &mut Command, socket: &Path) -> io::Result<ShimSpawn> {
    retire_live_shim(socket)?;
    let mut child = shim.spawn()?;
    let mut stream = connect_to_child(&mut child, socket)?;
    if let Err(error) = stream.write_all(b"REPLAY 0\n") {
        kill_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    Ok(ShimSpawn { child, stream })
}

pub(super) fn retire_live_shim(socket: &Path) -> io::Result<()> {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return Ok(());
    };
    let mut replies = stream.try_clone()?;
    thread::spawn(move || io::copy(&mut replies, &mut io::sink()));
    stream.write_all(b"REPLAY 0\nSHUTDOWN\n")?;
    let deadline = Instant::now() + RETIRE_TIMEOUT;
    while UnixStream::connect(socket).is_ok() {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("session shim at {} did not shut down", socket.display()),
            ));
        }
        thread::sleep(CONNECT_RETRY);
    }
    Ok(())
}

pub(super) fn connect_to_child(child: &mut Child, socket: &Path) -> io::Result<UnixStream> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "session shim exited before serving: {status}"
            )));
        }
        match UnixStream::connect(socket) {
            Ok(stream) => match peer_pid(&stream) {
                Ok(pid) if pid == child.id() => return Ok(stream),
                owner => {
                    kill_tree(child);
                    let _ = child.wait();
                    return Err(match owner {
                        Ok(pid) => io::Error::other(format!(
                            "session shim socket {} is served by pid {pid}, not spawned pid {}",
                            socket.display(),
                            child.id()
                        )),
                        Err(error) => error,
                    });
                }
            },
            Err(error) if Instant::now() >= deadline => {
                kill_tree(child);
                let _ = child.wait();
                return Err(error);
            }
            Err(_) => thread::sleep(CONNECT_RETRY),
        }
    }
}

#[cfg(target_os = "macos")]
fn peer_pid(stream: &UnixStream) -> io::Result<u32> {
    use std::os::fd::AsRawFd;
    let mut pid: nix::libc::pid_t = 0;
    let mut len = std::mem::size_of::<nix::libc::pid_t>() as nix::libc::socklen_t;
    let status = unsafe {
        nix::libc::getsockopt(
            stream.as_raw_fd(),
            nix::libc::SOL_LOCAL,
            nix::libc::LOCAL_PEERPID,
            std::ptr::addr_of_mut!(pid).cast(),
            &mut len,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    u32::try_from(pid).map_err(io::Error::other)
}

#[cfg(target_os = "linux")]
fn peer_pid(stream: &UnixStream) -> io::Result<u32> {
    use std::os::fd::AsRawFd;
    let mut credentials: nix::libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<nix::libc::ucred>() as nix::libc::socklen_t;
    let status = unsafe {
        nix::libc::getsockopt(
            stream.as_raw_fd(),
            nix::libc::SOL_SOCKET,
            nix::libc::SO_PEERCRED,
            std::ptr::addr_of_mut!(credentials).cast(),
            &mut len,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    u32::try_from(credentials.pid).map_err(io::Error::other)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_pid(_stream: &UnixStream) -> io::Result<u32> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(test)]
mod tests {
    use super::{shim_spawn_with, without_shim, SpawnConfig};
    use std::path::PathBuf;
    use std::process::Command;
    use std::thread;

    #[test]
    fn without_shim_skips_the_shim_only_on_the_calling_thread() {
        let config = SpawnConfig {
            exe: PathBuf::from("../../target/no-such-session-shim"),
            state_dir: PathBuf::from("../../target/shim-bypass"),
        };
        let spawn_probe = || {
            let mut command = Command::new("true");
            command.current_dir("../../target/shim-bypass/worktree");
            shim_spawn_with(Some(&config), &command)
        };

        let (inside, concurrent) = without_shim(|| {
            let inside = spawn_probe();
            let concurrent = thread::scope(|scope| scope.spawn(spawn_probe).join());
            (inside, concurrent)
        });

        assert!(matches!(inside, Ok(None)));
        let concurrent = concurrent.expect("probe thread");
        assert!(concurrent.is_err(), "other threads must still use the shim");
        assert!(spawn_probe().is_err(), "bypass must end with the scope");
    }
}
