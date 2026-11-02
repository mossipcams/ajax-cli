use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::jsonl_process::{JsonlProcess, JsonlRecord, FAIL_READER_SPAWN};

fn pid_alive(pid: u64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn pid_dead(pid: u64) -> bool {
    !pid_alive(pid)
}

#[test]
fn dropping_the_process_also_kills_its_grandchildren() {
    let script = r#"sleep 300 & printf '{"grandchild":%s}\n' "$!"; exec sleep 300"#;
    let process = JsonlProcess::spawn(
        Path::new("sh"),
        &["-c".to_string(), script.to_string()],
        &std::env::temp_dir(),
    )
    .expect("spawn sh");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut grandchild: Option<u64> = None;
    while grandchild.is_none() {
        if Instant::now() >= deadline {
            panic!("timed out waiting for the grandchild pid record");
        }
        match process.try_recv() {
            Ok(JsonlRecord::Record(value)) => {
                if let Some(Value::Number(n)) = value.get("grandchild") {
                    if let Some(pid) = n.as_u64() {
                        grandchild = Some(pid);
                    }
                }
            }
            Ok(JsonlRecord::Exited) => panic!("sh exited before reporting the grandchild pid"),
            Ok(_) => {}
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                panic!("stdout reader finished before reporting the grandchild pid")
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let grandchild = grandchild.expect("grandchild pid");

    assert!(
        pid_alive(grandchild),
        "grandchild must be running before the drop"
    );
    drop(process);

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && pid_alive(grandchild) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let gone = pid_dead(grandchild);

    if !gone {
        let _ = Command::new("kill")
            .args(["-9", &grandchild.to_string()])
            .status();
    }

    assert!(
        gone,
        "grandchild {grandchild} survived dropping JsonlProcess: the child must run in its own process group and the group must be killed"
    );
}

#[test]
fn issue_1278_failed_reader_start_kills_and_reaps_the_child() {
    let mut command = Command::new("sh");
    command
        .args(["-c", "exec sleep 300"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    super::process_group::own_group(&mut command);
    let mut child = command.spawn().expect("spawn sleep");
    let pid = u64::from(child.id());
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let (done_sender, done) = std::sync::mpsc::channel::<bool>();
    let start = std::thread::spawn(move || {
        FAIL_READER_SPAWN.with(|fail| fail.set(true));
        let result = JsonlProcess::from_parts(
            child,
            Box::new(stdin),
            Box::new(stdout),
            Some(Box::new(stderr)),
        );
        let _ = done_sender.send(result.is_err());
    });

    let returned_err = done.recv_timeout(Duration::from_secs(5)).ok();
    let alive = pid_alive(pid);
    if alive {
        let _ = Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
    start.join().expect("start thread");

    assert_eq!(
        returned_err,
        Some(true),
        "reader spawn failure must surface as an error"
    );
    assert!(
        !alive,
        "child {pid} survived a failed JsonlProcess start: the child must be killed and reaped"
    );
}

#[test]
fn issue_1278_failed_reader_start_in_attach_leaves_the_shim_alive() {
    let mut shim = Command::new("sleep")
        .arg("300")
        .spawn()
        .expect("spawn shim stand-in");
    let pid = shim.id();
    let (reader, _peer_reader) = UnixStream::pair().expect("reader socket pair");
    let (writer, _peer_writer) = UnixStream::pair().expect("writer socket pair");

    FAIL_READER_SPAWN.with(|fail| fail.set(true));
    let result = JsonlProcess::attach(Box::new(reader), Box::new(writer), pid);

    let alive = pid_alive(u64::from(pid));
    let _ = shim.kill();
    let _ = shim.wait();

    assert!(
        result.is_err(),
        "reader spawn failure must surface as an error"
    );
    assert!(
        alive,
        "attach must not kill the shim {pid}: it holds a running agent"
    );
}
