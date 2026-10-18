//! Process-identity guard for terminating leftover agent processes.
//!
//! A recorded "start token" (the `lstart` field from `ps`) pins a pid to the
//! exact process that was running when it was recorded, so a reused pid can
//! never be signalled by mistake.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Read the start-time token for `pid` via `ps -p <pid> -o lstart=`.
///
/// Returns `Some(trimmed stdout)` only when the command ran, exited 0 and the
/// trimmed output is non-empty; every other outcome is `None`. Pids `<= 0` are
/// rejected without running anything.
pub(super) fn start_token(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "lstart="])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Verdict {
    /// No process is running for the recorded identity.
    Gone,
    /// A process with the same pid has a *different* start time.
    Reused,
    /// The live start token matches the recorded one exactly.
    Same,
}

/// Compare a recorded start token against the live one. Pure function of its
/// inputs; no process inspection happens here.
#[allow(dead_code)]
pub(super) fn judge(recorded: &str, live: Option<&str>) -> Verdict {
    match live {
        None => Verdict::Gone,
        Some(live) if live == recorded => Verdict::Same,
        Some(_) => Verdict::Reused,
    }
}

/// Returns the pid of a foreign host's child that is still the same live process
/// recorded on the run row, or `None` when acquire must not be blocked.
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
    /// The matching process was signalled and is gone.
    Terminated,
    /// No process with the recorded identity exists any more.
    AlreadyGone,
    /// A same-pid process with a different start time; never signalled.
    NotSameProcess,
    /// `pid` is too privileged (<= 1) or is our own pid; never signalled.
    Refused,
    /// Signalling itself failed.
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

/// Poll until `start_token(pid)` no longer equals the recorded token (i.e. the
/// process is gone or has been replaced), for up to `budget`.
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

/// Terminate `pid` ONLY if it is provably the same process that was recorded:
/// same pid AND same start time. Sends SIGTERM, then escalates to SIGKILL if
/// the process is still the same one after 2 s. Never signals a reused pid,
/// pid <= 1, or our own pid.
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

    // ponytail: shells out to ps/kill; a token re-check just before each signal narrows but cannot close the pid-reuse race
    if !send_signal(pid, "-TERM") {
        return Termination::Failed;
    }
    if wait_no_longer_same(pid, recorded_token, Duration::from_secs(2)) {
        return Termination::Terminated;
    }

    // Still the same process after 2 s: re-check the token before escalating.
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
