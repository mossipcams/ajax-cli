use super::log::inbound_log_path;
use super::process::{serve, ShimArgs, Teardown};
use super::shim_paths;
use super::spawn::{connect_to_child, retire_live_shim, start_shim};
use super::ShimSpawn;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const ECHO_LOOP: &str = "while read l; do echo \"got:$l\"; done";
const CLIENT_TIMEOUT: Duration = Duration::from_secs(5);
const EXIT_TIMEOUT: Duration = Duration::from_secs(10);

fn scratch_state_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from("../../target/shim-process").join(name);
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn start_serve(state_dir: &Path, handle: &str, argv: &[&str]) -> Receiver<io::Result<i32>> {
    let args = ShimArgs {
        handle: handle.to_owned(),
        state_dir: state_dir.to_path_buf(),
        argv: argv.iter().map(OsString::from).collect(),
    };
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(serve(args, Teardown::Child));
    });
    receiver
}

fn await_serve(receiver: &Receiver<io::Result<i32>>) -> io::Result<i32> {
    receiver
        .recv_timeout(EXIT_TIMEOUT)
        .expect("serve should return before the test timeout")
}

fn socket_for(state_dir: &Path, handle: &str) -> PathBuf {
    shim_paths(state_dir, handle).socket
}

fn connect(socket: &Path) -> UnixStream {
    let deadline = Instant::now() + CLIENT_TIMEOUT;
    loop {
        match UnixStream::connect(socket) {
            Ok(stream) => {
                stream
                    .set_read_timeout(Some(CLIENT_TIMEOUT))
                    .expect("client read timeout");
                return stream;
            }
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "shim never listened on {}: {error}",
                    socket.display()
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

struct Client {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl Client {
    fn open(socket: &Path, hello: &str) -> Self {
        let stream = connect(socket);
        let reader = BufReader::new(stream.try_clone().expect("clone client stream"));
        let mut client = Self { stream, reader };
        client.send(hello);
        client
    }

    fn send(&mut self, line: &str) {
        self.stream
            .write_all(format!("{line}\n").as_bytes())
            .expect("send client line");
    }

    fn recv(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read shim line");
        line
    }
}

#[test]
fn live_lines_and_replay_reach_reconnecting_client() {
    let state_dir = scratch_state_dir("reconnect");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut first = Client::open(&socket, "REPLAY 0");
    first.send("a");
    assert_eq!(first.recv(), "got:a\n");
    drop(first);

    let mut second = Client::open(&socket, "REPLAY 0");
    assert_eq!(second.recv(), "got:a\n");
    second.send("b");
    assert_eq!(second.recv(), "got:b\n");
    second.send("SHUTDOWN");
    assert_eq!(await_serve(&serve).unwrap(), 0);
    assert!(!socket.exists());
}

#[test]
fn replay_from_mid_log_offset_returns_only_suffix() {
    let state_dir = scratch_state_dir("midlog");
    let socket = socket_for(&state_dir, "h");
    let script = "printf 'one\\ntwo\\n'; while read l; do echo \"got:$l\"; done";
    let serve = start_serve(&state_dir, "h", &["sh", "-c", script]);

    let mut full = Client::open(&socket, "REPLAY 0");
    assert_eq!(full.recv(), "one\n");
    assert_eq!(full.recv(), "two\n");

    let mut suffix = Client::open(&socket, "REPLAY 4");
    assert_eq!(suffix.recv(), "two\n");
    suffix.send("SHUTDOWN");
    assert_eq!(suffix.recv(), "");
    assert_eq!(await_serve(&serve).unwrap(), 0);
}

#[test]
fn inbound_log_records_only_json_rpc_responses() {
    let state_dir = scratch_state_dir("inbound-responses");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", "while read l; do :; done"]);

    let mut client = Client::open(&socket, "REPLAY 0");
    client.send(r#"{"jsonrpc":"2.0","id":3,"method":"session/request_permission","params":{}}"#);
    client.send(r#"{"jsonrpc":"2.0","method":"session/cancel","params":{}}"#);
    client.send(r#"{"jsonrpc":"2.0","id":3,"result":{"outcome":"selected"}}"#);
    client.send(r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"no"}}"#);
    client.send("SHUTDOWN");
    assert_eq!(await_serve(&serve).unwrap(), 0);

    let inbound = inbound_log_path(&shim_paths(&state_dir, "h").log);
    assert_eq!(
        fs::read_to_string(inbound).unwrap(),
        concat!(
            r#"{"jsonrpc":"2.0","id":3,"result":{"outcome":"selected"}}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"no"}}"#,
            "\n",
        )
    );
}

#[test]
fn shutdown_ends_harness_and_removes_socket() {
    let state_dir = scratch_state_dir("shutdown");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut client = Client::open(&socket, "REPLAY 0");
    client.send("SHUTDOWN");
    assert_eq!(client.recv(), "");
    assert_eq!(await_serve(&serve).unwrap(), 0);
    assert!(!socket.exists());
}

#[test]
fn write_then_close_client_still_shuts_harness_down() {
    let state_dir = scratch_state_dir("write-then-close");
    let socket = socket_for(&state_dir, "h");
    let log = shim_paths(&state_dir, "h").log;
    let serve = start_serve(&state_dir, "h", &["sh", "-c", "printf 'one\\n'; cat"]);

    let deadline = Instant::now() + CLIENT_TIMEOUT;
    while fs::metadata(&log).map_or(true, |metadata| metadata.len() == 0) {
        assert!(Instant::now() < deadline, "harness output never logged");
        thread::sleep(Duration::from_millis(10));
    }
    let mut stream = connect(&socket);
    stream
        .write_all(b"REPLAY 0\nSHUTDOWN\n")
        .expect("write hello and shutdown");
    drop(stream);

    assert_eq!(await_serve(&serve).unwrap(), 0);
    assert!(!socket.exists());
}

#[test]
fn shutdown_escalates_to_sigkill_when_harness_ignores_stdin_eof() {
    let state_dir = scratch_state_dir("sigkill");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", "exec sleep 30"]);

    let mut client = Client::open(&socket, "REPLAY 0");
    client.send("SHUTDOWN");
    assert_eq!(await_serve(&serve).unwrap(), 128 + 9);
    assert!(!socket.exists());
}

#[test]
fn harness_exit_returns_its_code_and_keeps_log() {
    let state_dir = scratch_state_dir("exit3");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", "echo hi; exit 3"]);

    assert_eq!(await_serve(&serve).unwrap(), 3);
    assert!(!socket.exists());
    let log = shim_paths(&state_dir, "h").log;
    assert_eq!(fs::read(log).unwrap(), b"hi\n");
}

#[test]
fn second_serve_for_live_handle_is_rejected_without_disturbing_first() {
    let state_dir = scratch_state_dir("conflict");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut first = Client::open(&socket, "REPLAY 0");
    first.send("a");
    assert_eq!(first.recv(), "got:a\n");

    let intruder = start_serve(&state_dir, "h", &["sh", "-c", "echo intruder"]);
    let error = await_serve(&intruder).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AddrInUse);

    first.send("b");
    assert_eq!(first.recv(), "got:b\n");
    let log = fs::read_to_string(shim_paths(&state_dir, "h").log).unwrap();
    assert!(!log.contains("intruder"));

    first.send("SHUTDOWN");
    assert_eq!(await_serve(&serve).unwrap(), 0);
}

#[test]
fn stale_socket_from_dead_shim_is_replaced() {
    let state_dir = scratch_state_dir("stale");
    let socket = socket_for(&state_dir, "h");
    fs::create_dir_all(socket.parent().expect("socket dir")).unwrap();
    drop(UnixListener::bind(&socket).unwrap());
    assert!(socket.exists());

    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);
    let mut client = Client::open(&socket, "REPLAY 0");
    client.send("SHUTDOWN");
    assert_eq!(client.recv(), "");
    assert_eq!(await_serve(&serve).unwrap(), 0);
    assert!(!socket.exists());
}

const SHIM_CHILD_STATE_ENV: &str = "AJAX_SESSION_SHIM_TEST_STATE_DIR";
const SHIM_CHILD_HANDLE: &str = "h";

#[test]
fn shim_child_helper() {
    let Some(state_dir) = std::env::var_os(SHIM_CHILD_STATE_ENV) else {
        return;
    };
    let args = ShimArgs {
        handle: SHIM_CHILD_HANDLE.to_owned(),
        state_dir: PathBuf::from(state_dir),
        argv: ["sh", "-c", ECHO_LOOP].iter().map(OsString::from).collect(),
    };
    let code = serve(args, Teardown::Child).unwrap_or(1);
    std::process::exit(code);
}

fn shim_child_command(state_dir: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().expect("test binary path"));
    command
        .args([
            "--exact",
            "adapters::session_shim::process_tests::shim_child_helper",
            "--nocapture",
        ])
        .env(SHIM_CHILD_STATE_ENV, state_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn await_exit(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + EXIT_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().expect("poll child") {
            return status;
        }
        assert!(Instant::now() < deadline, "child did not exit");
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn retire_live_shim_shuts_down_old_shim_before_returning() {
    let state_dir = scratch_state_dir("retire");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut client = Client::open(&socket, "REPLAY 0");
    client.send("a");
    assert_eq!(client.recv(), "got:a\n");

    retire_live_shim(&socket).expect("retire live shim");
    assert_eq!(await_serve(&serve).unwrap(), 0);
    assert!(!socket.exists());
}

#[test]
fn retire_live_shim_is_a_no_op_without_a_live_shim() {
    let state_dir = scratch_state_dir("retire-none");
    retire_live_shim(&socket_for(&state_dir, "h")).expect("nothing to retire");
}

#[test]
fn spawned_stream_is_never_connected_to_a_foreign_shim() {
    let state_dir = scratch_state_dir("foreign");
    let socket = socket_for(&state_dir, "h");
    let serve = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut foreign = Client::open(&socket, "REPLAY 0");
    foreign.send("a");
    assert_eq!(foreign.recv(), "got:a\n");

    let mut impostor = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn impostor");
    let error = connect_to_child(&mut impostor, &socket)
        .expect_err("a foreign shim must not be adopted by a new child");
    assert!(error.to_string().contains("not spawned pid"), "{error}");
    let _ = impostor.wait();

    let mut survivor = Client::open(&socket, "REPLAY 0");
    assert_eq!(survivor.recv(), "got:a\n");
    survivor.send("b");
    assert_eq!(survivor.recv(), "got:b\n");
    survivor.send("SHUTDOWN");
    assert_eq!(await_serve(&serve).unwrap(), 0);
}

#[test]
fn respawn_retires_live_shim_and_attaches_only_to_the_new_child() {
    let state_dir = scratch_state_dir("respawn");
    let socket = socket_for(&state_dir, "h");
    let old = start_serve(&state_dir, "h", &["sh", "-c", ECHO_LOOP]);

    let mut old_client = Client::open(&socket, "REPLAY 0");
    old_client.send("a");
    assert_eq!(old_client.recv(), "got:a\n");

    let ShimSpawn { mut child, stream } =
        start_shim(&mut shim_child_command(&state_dir), &socket).expect("respawn shim");
    assert_eq!(await_serve(&old).unwrap(), 0);

    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .expect("client read timeout");
    let mut writer = stream.try_clone().expect("clone socket");
    writer.write_all(b"x\n").expect("send input");
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read harness output");
    assert_eq!(line, "got:x\n");

    writer.write_all(b"SHUTDOWN\n").expect("send shutdown");
    assert!(await_exit(&mut child).success());
    assert!(!socket.exists());
}
