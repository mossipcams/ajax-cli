//! One-time orphan sweep for Chat harness child processes.
//!
//! Each run row records the live agent child's pid, a start-time token, and the
//! owning host's boot id (`TaskSessionState::record_child_identity`); this is
//! the only code that consumes that data. When a host dies (crash, kill), its
//! spawned ACP children keep running forever. This sweep terminates those
//! children when a *new* host takes over the registry: it runs exactly once per
//! host process, and only after that host has acquired the execution owner
//! lease (`ExecutionOwnership::try_claim`). Runs owned by this host — same boot
//! id — are never touched, so attach/respawn within one process cannot kill a
//! live child. A pid is signalled only when its live start-time token matches
//! the stored one (pid-reuse protection); anything else leaves the row as is.
//! The sweep never fails startup, attach, or prompt admission: every error is
//! logged with `tracing::warn` and the run moves on.

use super::process_identity::{judge, start_token, Verdict};
use ajax_core::registry::{ExecutionCommit, RunUpdate, SqliteRegistryStore};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set the first time this process successfully runs the sweep. nextest and
/// production each run their own process, so the flag cannot leak between tests.
static SWEPT_ONCE: AtomicBool = AtomicBool::new(false);

/// Runs [`sweep`] at most once per host process. Called from
/// `ExecutionOwnership::try_claim` immediately after this host holds the
/// execution owner lease, and never while a live foreign host holds it.
pub(crate) fn sweep_once(store: &SqliteRegistryStore, boot_id: &str) {
    if SWEPT_ONCE
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    // `sweep` logs its own errors and returns only a count.
    let _ = sweep(store, boot_id);
}

/// Terminates harness children left behind by previous hosts and clears their
/// recorded identity. Returns the number of run rows handled (signalled or
/// found already gone). Invariants:
///
/// - A run whose `owner_boot_id` equals `current_boot_id` is never touched —
///   it belongs to a live child in this host.
/// - A child is signalled (`kill -TERM <pid>`) ONLY when the process is alive
///   and its start-time token equals the stored `child_started`. A missing,
///   mismatched (pid reuse), or unreadable token means: do not signal.
/// - After a matching child is signalled, or when the pid no longer exists,
///   the run's child fields are cleared via `commit_execution` with
///   `RunUpdate { child: Some(None), child_started: Some(None), .. }`. When
///   the token is unreadable for a pid that may still exist, the row is left
///   unchanged.
pub(crate) fn sweep(store: &SqliteRegistryStore, current_boot_id: &str) -> u64 {
    let runs = match store.runs_with_children() {
        Ok(runs) => runs,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "orphan sweep: cannot list child rows");
            return 0;
        }
    };
    let mut handled = 0u64;
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
                handled += 1;
            }
            continue;
        }
        match judge(
            run.child_started.as_deref().unwrap_or(""),
            start_token(pid).as_deref(),
        ) {
            Verdict::Same => {
                // ponytail: only the recorded pid is signalled, not its descendants; upgrade path is to spawn children in their own process group (setpgid) and signal the group.
                if signal_term(pid) && clear_child_identity(store, &run.run_id, pid) {
                    handled += 1;
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
    handled
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

/// Best-effort `kill -TERM <pid>`; `true` when the signal was sent.
fn signal_term(pid: i64) -> bool {
    if pid <= 0 || pid == std::process::id() as i64 {
        return false; // Never signal ourselves (or invalid pids).
    }
    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
