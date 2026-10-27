use std::process::{Command, Stdio};
use std::time::Duration;

use super::process_identity::{judge, start_token, terminate_if_same, Termination, Verdict};

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

fn kill_and_reap(pid: i64, handle: std::thread::JoinHandle<std::process::ExitStatus>) {
    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("kill sleep helper");
    handle.join().expect("join sleep reaper");
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
fn terminates_the_matching_process() {
    let (pid, handle) = spawn_sleep_child();
    let token = start_token(pid).expect("live child must have a start token");

    assert_eq!(terminate_if_same(pid, &token), Termination::Terminated);

    let status = handle.join().expect("join sleep reaper");
    assert!(
        !status.success(),
        "a terminated process must not exit successfully"
    );
}

#[test]
fn never_signals_a_process_with_a_different_start_time() {
    let (pid, handle) = spawn_sleep_child();

    assert_eq!(
        terminate_if_same(pid, "Thu Jan  1 00:00:00 1970"),
        Termination::NotSameProcess,
        "a mismatched start token must never lead to a signal"
    );

    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !handle.is_finished(),
        "the process must still be alive after a refused termination"
    );

    kill_and_reap(pid, handle);
}

#[test]
fn already_gone_is_reported() {
    let mut child = Command::new("true")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn true helper");
    let pid = child.id() as i64;
    child.wait().expect("wait on true helper");

    assert_eq!(terminate_if_same(pid, "x"), Termination::AlreadyGone);
}

#[test]
fn refuses_pid_one_and_own_pid() {
    assert_eq!(terminate_if_same(1, "x"), Termination::Refused);
    assert_eq!(
        terminate_if_same(std::process::id() as i64, "x"),
        Termination::Refused
    );
}

#[test]
fn start_token_rejects_non_positive_pids() {
    assert_eq!(start_token(0), None);
    assert_eq!(start_token(-5), None);
}
