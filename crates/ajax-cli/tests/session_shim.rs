use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use ajax_web::adapters::session_shim::{enable, shim_paths, shim_spawn, ShimSpawn};
use nix::{
    errno::Errno,
    sys::signal::killpg,
    unistd::{getsid, Pid},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(20);

struct ShimProcess(Child);

impl Drop for ShimProcess {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(POLL);
    }
    condition()
}

fn spawn_shim(state_dir: &Path, handle: &str) -> ShimProcess {
    let child = Command::new(env!("CARGO_BIN_EXE_ajax-cli"))
        .arg("session-shim")
        .args(["--handle", handle, "--state-dir"])
        .arg(state_dir)
        .args(["--", "sh", "-c", "while read l; do echo got:$l; done"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn session shim");
    ShimProcess(child)
}

#[test]
fn session_shim_relays_client_lines_and_exits_on_shutdown() {
    let state_dir = std::env::temp_dir().join(format!("ajs-{}", std::process::id()));
    fs::create_dir_all(&state_dir).expect("create state dir");
    let handle = "t/h";
    let socket = shim_paths(&state_dir, handle).socket;

    let mut shim = spawn_shim(&state_dir, handle);
    let pid = Pid::from_raw(i32::try_from(shim.0.id()).expect("pid fits in i32"));

    assert!(
        wait_until(|| socket.exists()),
        "shim socket never appeared at {}",
        socket.display()
    );
    assert_eq!(
        getsid(Some(pid)).expect("getsid for shim"),
        pid,
        "session shim must lead its own session"
    );

    let stream = UnixStream::connect(&socket).expect("connect to shim socket");
    stream
        .set_read_timeout(Some(TIMEOUT))
        .expect("set read timeout");
    let mut writer = stream.try_clone().expect("clone socket");
    writer
        .write_all(b"REPLAY 0\nx\n")
        .expect("send replay and input");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read harness output");
    assert_eq!(line, "got:x\n");

    writer.write_all(b"SHUTDOWN\n").expect("send shutdown");
    assert!(
        wait_until(|| matches!(shim.0.try_wait(), Ok(Some(_)))),
        "session shim did not exit within {TIMEOUT:?} of SHUTDOWN"
    );
    assert_eq!(
        killpg(pid, None),
        Err(Errno::ESRCH),
        "harness process group must not survive shutdown"
    );

    let _ = fs::remove_dir_all(&state_dir);
}

#[test]
fn shim_spawn_round_trips_through_shim_and_kills_harness_group() {
    let state_dir = std::env::temp_dir().join(format!("ajss-{}", std::process::id()));
    let cwd = state_dir.join("worktree");
    fs::create_dir_all(&cwd).expect("create scratch worktree");
    enable(
        PathBuf::from(env!("CARGO_BIN_EXE_ajax-cli")),
        state_dir.clone(),
    );

    let mut command = Command::new("sh");
    command
        .args(["-c", "while read l; do echo got:$l; done"])
        .current_dir(&cwd);
    let ShimSpawn { mut child, stream } = shim_spawn(&command)
        .expect("shim spawn")
        .expect("shim enabled");
    let shim_pid = child.id();
    let group = Pid::from_raw(i32::try_from(shim_pid).expect("pid fits in i32"));
    let _harness_guard = GroupKill(shim_pid);

    stream
        .set_read_timeout(Some(TIMEOUT))
        .expect("set read timeout");
    let mut writer = stream.try_clone().expect("clone socket");
    writer.write_all(b"x\n").expect("send harness input");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read harness output");
    assert_eq!(line, "got:x\n");

    let killed = Command::new("kill")
        .args(["-KILL", "--", &format!("-{shim_pid}")])
        .status()
        .expect("run kill");
    assert!(killed.success(), "kill of shim group failed");
    child.wait().expect("reap shim");
    assert!(
        wait_until(|| killpg(group, None) == Err(Errno::ESRCH)),
        "harness process group must not survive the shim group kill"
    );

    let _ = fs::remove_dir_all(&state_dir);
}

struct GroupKill(u32);

impl Drop for GroupKill {
    fn drop(&mut self) {
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", self.0)])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
