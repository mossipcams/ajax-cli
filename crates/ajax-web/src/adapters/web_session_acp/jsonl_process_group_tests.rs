//! Regression tests for process-group cleanup when a [`JsonlProcess`] is dropped.
//!
//! The child must be spawned in its own process group and the whole group must
//! be killed on drop, otherwise grandchildren survive and leak.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::jsonl_process::{JsonlProcess, JsonlRecord};

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
    // `sh` backgrounds a sleeper (the grandchild), reports its pid as one JSONL
    // record, then execs another sleeper as the direct child.
    let script = r#"sleep 300 & printf '{"grandchild":%s}\n' "$!"; exec sleep 300"#;
    let process = JsonlProcess::spawn(
        Path::new("sh"),
        &["-c".to_string(), script.to_string()],
        &std::env::temp_dir(),
    )
    .expect("spawn sh");

    // Read until a record carrying the grandchild's pid; block with bounded
    // `try_recv` polls because `recv()` would block forever on a hung child.
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

    // Poll up to 3s for the grandchild to disappear.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && pid_alive(grandchild) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let gone = pid_dead(grandchild);

    // Never leak: if the group kill failed, clean up manually.
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
