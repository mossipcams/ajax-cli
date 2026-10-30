use std::process::{Command, Stdio};
use std::time::Duration;

use super::process_identity::{judge, start_token, Verdict};

fn spawn_sleep_child() -> (i64, std::thread::JoinHandle<std::process::ExitStatus>) {
    let mut child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn sleep helper");
    let pid = child.id() as i64;
    let handle = std::thread::spawn(move || child.wait().expect("wait on sleep helper"));
    (pid, handle)
}

#[test]
fn judge_truth_table() {
    assert_eq!(judge("A", None), Verdict::Gone);
    assert_eq!(judge("A", Some("A")), Verdict::Same);
    assert_eq!(judge("A", Some("B")), Verdict::Reused);
}

#[test]
fn start_token_of_a_live_child_is_stable_and_gone_after_exit() {
    let (pid, handle) = spawn_sleep_child();

    let first = start_token(pid).expect("live child must have a start token");
    let second = start_token(pid).expect("live child must still have a token");
    assert_eq!(
        first, second,
        "the token must be stable while the process lives"
    );

    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("kill sleep helper");

    let deadline = std::time::Instant::now();
    loop {
        if start_token(pid).is_none() {
            break;
        }
        assert!(
            deadline.elapsed() < Duration::from_secs(2),
            "process never left ps"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    handle.join().expect("join sleep reaper");
}

#[test]
fn start_token_rejects_non_positive_pids() {
    assert_eq!(start_token(0), None);
    assert_eq!(start_token(-5), None);
}
