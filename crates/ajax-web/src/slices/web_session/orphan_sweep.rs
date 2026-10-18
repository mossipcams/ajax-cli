//! Orphan sweep for Chat harness child processes, retried until complete.
//!
//! Each run row records the live agent child's pid, a start-time token, and the
//! owning host's boot id (`TaskSessionState::record_child_identity`); this is
//! the only code that consumes that data. When a host dies (crash, kill), its
//! spawned ACP children keep running forever. This sweep terminates those
//! children when a *new* host takes over the registry: it runs until complete per
//! host process, and only after that host has acquired the execution owner
//! lease (`ExecutionOwnership::try_claim`). The same claim hook
//! also interrupts prompts left `Dispatching` by dead hosts (see
//! `super::startup_recovery`). Runs owned by this host — same boot
//! id — are never touched, so attach/respawn within one process cannot kill a
//! live child. A pid is signalled only when its live start-time token matches
//! the stored one (pid-reuse protection); anything else leaves the row as is.
//! The sweep never fails startup, attach, or prompt admission: every error is
//! logged with `tracing::warn` and the run moves on.

use super::process_identity::{judge, start_token, Verdict};
use super::startup_recovery::RecoveryResult;
use ajax_core::registry::{ExecutionCommit, RunUpdate, SqliteRegistryStore};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Set the first time this process successfully runs the sweep. nextest and
/// production each run their own process, so the flag cannot leak between tests.
static SWEPT_ONCE: AtomicBool = AtomicBool::new(false);
static SWEEP_LOCK: Mutex<()> = Mutex::new(());

/// Runs [`sweep`] — then `super::startup_recovery::recover_interrupted_prompts`
/// — until both complete in this host process. Called from
/// `ExecutionOwnership::try_claim` immediately after this host holds the
/// execution owner lease, and never while a live foreign host holds it. The
/// sweep runs first so it clears dead hosts' child identities before recovery
/// rewrites prompt state. Returns the handles (task ids) of runs whose prompts
/// were interrupted by the call that actually ran the sweep, in scan order;
/// every call after completion returns no handles and does no work.
pub(crate) fn sweep_once(
    store: &SqliteRegistryStore,
    state_dir: &std::path::Path,
    boot_id: &str,
) -> RecoveryResult {
    let _guard = SWEEP_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    if SWEPT_ONCE.load(Ordering::SeqCst) {
        return RecoveryResult {
            recovered_handles: Vec::new(),
            complete: true,
        };
    }
    let sweep = sweep(store, boot_id);
    let mut recovery =
        super::startup_recovery::recover_interrupted_prompts(store, state_dir, boot_id);
    recovery.complete &= sweep.complete;
    if recovery.complete {
        let _ = SWEPT_ONCE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
    }
    recovery
}

pub(crate) struct SweepResult {
    pub(crate) handled: u64,
    pub(crate) complete: bool,
}

/// Terminates harness children left behind by previous hosts and clears their
/// recorded identity. Returns the number of run rows cleared and whether
/// listing, termination, and identity writes succeeded. Invariants:
///
/// - A run whose `owner_boot_id` equals `current_boot_id` is never touched —
///   it belongs to a live child in this host.
/// - A child is signalled (its process group via `kill -TERM -- -<pid>` first,
///   since the child is a group leader, falling back to `kill -TERM <pid>`) ONLY
///   when the process is alive
///   and its start-time token equals the stored `child_started`. A missing,
///   mismatched (pid reuse), or unreadable token means: do not signal.
/// - After TERM, wait up to 5 s for exit, then escalate to KILL and wait up to
///   2 s more. Clear identity only once the pid is observed gone; a surviving
///   child leaves the sweep incomplete for the next heartbeat to retry.
/// - When the pid no longer exists,
///   the run's child fields are cleared via `commit_execution` with
///   `RunUpdate { child: Some(None), child_started: Some(None), .. }`. When
///   the token is unreadable for a pid that may still exist, the row is left
///   unchanged.
pub(crate) fn sweep(store: &SqliteRegistryStore, current_boot_id: &str) -> SweepResult {
    sweep_with_grace(store, current_boot_id, Duration::from_secs(5))
}

pub(super) fn sweep_with_grace(
    store: &SqliteRegistryStore,
    current_boot_id: &str,
    grace: Duration,
) -> SweepResult {
    let mut result = SweepResult {
        handled: 0,
        complete: true,
    };
    let runs = match store.runs_with_children() {
        Ok(runs) => runs,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "orphan sweep: cannot list child rows");
            result.complete = false;
            return result;
        }
    };
    for run in &runs {
        if run.owner_boot_id.as_deref() == Some(current_boot_id) {
            continue; // This host's own live child.
        }
        let (pid, _pgid) = match run.child {
            Some(child) => child,
            None => continue, // Defensively: the listing filters these out.
        };
        if !pid_alive(pid) {
            if clear_child_identity(store, &run.run_id, pid) {
                result.handled += 1;
            } else {
                result.complete = false;
            }
            continue;
        }
        match judge(
            run.child_started.as_deref().unwrap_or(""),
            start_token(pid).as_deref(),
        ) {
            Verdict::Same => {
                if !signal_group_or_pid(pid, "-TERM") {
                    result.complete = false;
                    continue;
                }
                if !wait_until_gone(pid, grace) {
                    // The pid may have been reused during the grace period.
                    if judge(
                        run.child_started.as_deref().unwrap_or(""),
                        start_token(pid).as_deref(),
                    ) == Verdict::Same
                    {
                        signal_group_or_pid(pid, "-KILL");
                    }
                    if !wait_until_gone(pid, Duration::from_secs(2)) {
                        tracing::warn!(target: "ajax_web", run_id = %run.run_id, pid,
                            "orphan sweep: child still alive after termination attempt; retaining identity for retry");
                        result.complete = false;
                        continue;
                    }
                }
                if clear_child_identity(store, &run.run_id, pid) {
                    result.handled += 1;
                } else {
                    result.complete = false;
                }
            }
            _ => tracing::debug!(
                target: "ajax_web",
                run_id = %run.run_id,
                pid,
                "orphan sweep left row unchanged: start-time token not confirmed"
            ),
        }
    }
    result
}

fn wait_until_gone(pid: i64, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if !pid_alive(pid) {
            return true;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

/// Clears the run's child identity fields; everything else is untouched.
fn clear_child_identity(store: &SqliteRegistryStore, run_id: &str, pid: i64) -> bool {
    match store.commit_execution(&ExecutionCommit {
        run_id: run_id.to_owned(),
        run_update: Some(RunUpdate {
            child: Some(None),
            child_started: Some(None),
            ..Default::default()
        }),
        ..Default::default()
    }) {
        Ok(_) => true,
        Err(error) => {
            tracing::warn!(target: "ajax_web", run_id = %run_id, pid, %error, "orphan sweep: failed to clear child identity");
            false
        }
    }
}

/// `kill -0` succeeds only for an existing process we may signal.
fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Best-effort signal to a pid; `true` when the signal was sent.
fn signal_pid(pid: i64, signal: &str) -> bool {
    if pid <= 0 || pid == std::process::id() as i64 {
        return false; // Never signal ourselves (or invalid pids).
    }
    Command::new("kill")
        .args([signal, &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Best-effort signal to the child's process group via `kill <signal> -- -<pid>` (the
/// current host spawns children as their own group leaders, so the recorded
/// pid is also the pgid), falling back to a plain `kill <signal> <pid>` for
/// children recorded by older hosts that were not group leaders. The
/// self-pid and invalid-pid guards apply before either signal.
fn signal_group_or_pid(pid: i64, signal: &str) -> bool {
    if pid <= 0 || pid == std::process::id() as i64 {
        return false; // Never signal ourselves (or invalid pids).
    }
    let group_signalled = Command::new("kill")
        .args([signal, "--", &format!("-{pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if group_signalled {
        true
    } else {
        signal_pid(pid, signal)
    }
}
