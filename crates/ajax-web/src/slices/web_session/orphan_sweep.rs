use super::os_process;
use super::process_identity::{judge, start_token, Verdict};
use super::startup_recovery::RecoveryResult;
use ajax_core::registry::{ExecutionCommit, RunUpdate, SqliteRegistryStore};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SWEPT_ONCE: AtomicBool = AtomicBool::new(false);
static SWEEP_LOCK: Mutex<()> = Mutex::new(());

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
            continue;
        }
        let (pid, _pgid) = match run.child {
            Some(child) => child,
            None => continue,
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

fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    os_process::exists(pid) && !os_process::is_zombie(pid)
}

fn signal_pid(pid: i64, signal: &str) -> bool {
    if pid <= 0 || pid == std::process::id() as i64 {
        return false;
    }
    Command::new("kill")
        .args([signal, &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn signal_group_or_pid(pid: i64, signal: &str) -> bool {
    if pid <= 0 || pid == std::process::id() as i64 {
        return false;
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
