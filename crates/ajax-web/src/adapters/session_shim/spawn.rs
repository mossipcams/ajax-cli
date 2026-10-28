use super::shim_paths;
use crate::adapters::web_session_acp::process_group::kill_tree;
use std::fs;
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

struct SpawnConfig {
    exe: PathBuf,
    state_dir: PathBuf,
}

static CONFIG: OnceLock<SpawnConfig> = OnceLock::new();

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

pub fn shim_spawn(command: &Command) -> io::Result<Option<ShimSpawn>> {
    let Some(config) = CONFIG.get() else {
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
    let mut child = shim.spawn()?;
    let socket = shim_paths(&config.state_dir, handle).socket;
    let mut stream = connect(&mut child, &socket)?;
    if let Err(error) = stream.write_all(b"REPLAY 0\n") {
        kill_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    Ok(Some(ShimSpawn { child, stream }))
}

pub fn log_offset(cwd: &Path) -> Option<u64> {
    let config = CONFIG.get()?;
    let handle = cwd.to_str()?;
    fs::metadata(shim_paths(&config.state_dir, handle).log)
        .ok()
        .map(|metadata| metadata.len())
}

pub fn probe_live(state_dir: &Path, cwd: &Path) -> bool {
    let Some(handle) = cwd.to_str() else {
        return false;
    };
    UnixStream::connect(shim_paths(state_dir, handle).socket).is_ok()
}

#[cfg(test)]
pub(crate) fn spawn_test_shim(
    state_dir: &Path,
    handle: &str,
) -> thread::JoinHandle<io::Result<i32>> {
    let args = super::ShimArgs {
        handle: handle.to_owned(),
        state_dir: state_dir.to_path_buf(),
        argv: ["sh", "-c", "cat"]
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect(),
    };
    thread::spawn(move || super::process::serve(args, super::process::Teardown::Child))
}

fn connect(child: &mut Child, socket: &Path) -> io::Result<UnixStream> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(io::Error::other(format!(
                "session shim exited before serving: {status}"
            )));
        }
        match UnixStream::connect(socket) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() >= deadline => {
                kill_tree(child);
                let _ = child.wait();
                return Err(error);
            }
            Err(_) => thread::sleep(CONNECT_RETRY),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::log_offset;
    use std::path::Path;

    #[test]
    fn log_offset_is_none_when_shim_is_not_enabled() {
        assert_eq!(log_offset(Path::new("/repo/worktree")), None);
    }
}
