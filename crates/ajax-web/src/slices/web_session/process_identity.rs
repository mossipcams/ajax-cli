use super::os_process;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub(super) fn start_token(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    os_process::start_token(pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Verdict {
    Gone,
    Reused,
    Same,
}

#[allow(dead_code)]
pub(super) fn judge(recorded: &str, live: Option<&str>) -> Verdict {
    match live {
        None => Verdict::Gone,
        Some(live) if live == recorded => Verdict::Same,
        Some(_) => Verdict::Reused,
    }
}

pub(super) fn foreign_live_child(
    child: Option<(i64, i64)>,
    child_started: Option<&str>,
    owner_boot_id: Option<&str>,
    host_boot_id: &str,
) -> Option<i64> {
    let (pid, _) = child?;
    let owner = owner_boot_id?;
    if owner == host_boot_id {
        return None;
    }
    let recorded = child_started?;
    if judge(recorded, start_token(pid).as_deref()) == Verdict::Same {
        Some(pid)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Termination {
    Terminated,
    AlreadyGone,
    NotSameProcess,
    Refused,
    Failed,
}

fn send_signal(pid: i64, signal: &str) -> bool {
    match Command::new("kill")
        .args([signal, &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
    {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

fn wait_no_longer_same(pid: i64, recorded_token: &str, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if judge(recorded_token, start_token(pid).as_deref()) != Verdict::Same {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[allow(dead_code)]
pub(super) fn terminate_if_same(pid: i64, recorded_token: &str) -> Termination {
    if pid <= 1 || pid == std::process::id() as i64 {
        return Termination::Refused;
    }
    match judge(recorded_token, start_token(pid).as_deref()) {
        Verdict::Gone => return Termination::AlreadyGone,
        Verdict::Reused => return Termination::NotSameProcess,
        Verdict::Same => {}
    }

    if !send_signal(pid, "-TERM") {
        return Termination::Failed;
    }
    if wait_no_longer_same(pid, recorded_token, Duration::from_secs(2)) {
        return Termination::Terminated;
    }

    match judge(recorded_token, start_token(pid).as_deref()) {
        Verdict::Same => {}
        _ => return Termination::Terminated,
    }
    if !send_signal(pid, "-KILL") {
        return Termination::Failed;
    }
    if wait_no_longer_same(pid, recorded_token, Duration::from_secs(1)) {
        Termination::Terminated
    } else {
        Termination::Failed
    }
}
