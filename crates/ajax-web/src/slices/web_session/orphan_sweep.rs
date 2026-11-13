use super::os_process;
use super::process_identity::{judge, start_token, Verdict};
use super::process_tree::docker_text;
use super::shim_adoption::adoptable;
use super::startup_recovery::RecoveryResult;
use crate::adapters::program;
use ajax_core::adapters::docker_sandbox::{parse_ps, ps_argv, rm_argv, task_key};
use ajax_core::registry::{ExecutionCommit, Registry, RunUpdate, SqliteRegistryStore};
use std::io;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static SWEPT_ONCE: AtomicBool = AtomicBool::new(false);
static SWEEP_LOCK: Mutex<()> = Mutex::new(());

const SWEEP_GRACE: Duration = Duration::from_secs(5);

pub(crate) fn sweep_once(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    boot_id: &str,
) -> RecoveryResult {
    let _guard = SWEEP_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    if SWEPT_ONCE.load(Ordering::SeqCst) {
        return RecoveryResult {
            recovered_handles: Vec::new(),
            complete: true,
        };
    }
    let sweep = sweep_adopting(store, state_dir, boot_id, SWEEP_GRACE);
    let mut recovery =
        super::startup_recovery::recover_interrupted_prompts(store, state_dir, boot_id);
    recovery.complete &= sweep.complete;
    remove_finalised_containers(store, boot_id, &recovery.recovered_handles);
    if recovery.complete {
        let _ = SWEPT_ONCE.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
    }
    recovery
}

fn remove_finalised_containers(store: &SqliteRegistryStore, boot_id: &str, handles: &[String]) {
    if handles.is_empty() {
        return;
    }
    let Some(server) = program::sandbox_docker_server() else {
        return;
    };
    let registry = match store.load_tasks_only() {
        Ok(registry) => registry,
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "orphan sweep: cannot load tasks for container cleanup");
            return;
        }
    };
    for handle in handles {
        let Some(task) = registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == *handle)
        else {
            continue;
        };
        let key = task_key(&task.worktree_path);
        remove_task_containers(&program::sandbox_docker_run, &server, boot_id, &key);
    }
}

fn remove_task_containers(
    run: &dyn Fn(&[&str]) -> io::Result<Output>,
    server: &str,
    boot_id: &str,
    key: &str,
) {
    let Some(listing) = docker_text(run, &ps_argv(server, key)) else {
        tracing::warn!(target: "ajax_web", key, "orphan sweep: cannot list sandbox containers");
        return;
    };
    for row in parse_ps(&listing) {
        if row.server != server || row.boot_id == boot_id {
            continue;
        }
        if docker_text(run, &rm_argv(&row.id)).is_none() {
            tracing::warn!(target: "ajax_web", container = %row.id, "orphan sweep: failed to remove sandbox container");
        }
    }
}

pub(crate) struct SweepResult {
    pub(crate) handled: u64,
    pub(crate) complete: bool,
}

pub(super) fn sweep_adopting(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    current_boot_id: &str,
    grace: Duration,
) -> SweepResult {
    sweep_sparing(store, current_boot_id, grace, &|task_handle| {
        adoptable(store, state_dir, task_handle).is_some()
    })
}

#[cfg(test)]
pub(crate) fn sweep(store: &SqliteRegistryStore, current_boot_id: &str) -> SweepResult {
    sweep_with_grace(store, current_boot_id, SWEEP_GRACE)
}

#[cfg(test)]
pub(super) fn sweep_with_grace(
    store: &SqliteRegistryStore,
    current_boot_id: &str,
    grace: Duration,
) -> SweepResult {
    sweep_sparing(store, current_boot_id, grace, &|_| false)
}

fn sweep_sparing(
    store: &SqliteRegistryStore,
    current_boot_id: &str,
    grace: Duration,
    is_adoptable: &dyn Fn(&str) -> bool,
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
                if is_adoptable(&run.task_id) {
                    result.handled += 1;
                    continue;
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const SERVER: &str = "5a1c0e2f9b3d4e61";
    const KEY: &str = "repo-00000001";

    fn output(stdout: &str, code: i32) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn removal_calls(listing: &str, docker_up: bool, server: &str, boot_id: &str) -> Vec<String> {
        let calls = RefCell::new(Vec::new());
        let run = |args: &[&str]| -> io::Result<Output> {
            calls.borrow_mut().push(args.join(" "));
            if !docker_up {
                return Err(io::Error::other("daemon down"));
            }
            if args[0] == "ps" {
                return Ok(output(listing, 0));
            }
            Ok(output("", 0))
        };
        remove_task_containers(&run, server, boot_id, KEY);
        calls.into_inner()
    }

    fn removed(calls: &[String]) -> Vec<&str> {
        calls
            .iter()
            .filter_map(|call| call.strip_prefix("rm -f "))
            .collect()
    }

    #[test]
    fn finalised_orphan_container_of_a_dead_boot_is_removed() {
        let calls = removal_calls("c1 dead-boot 5a1c0e2f9b3d4e61\n", true, SERVER, "live-boot");

        assert!(calls[0].starts_with("ps --no-trunc --filter label=ajax.server=5a1c0e2f9b3d4e61"));
        assert!(calls[0].contains("label=ajax.task_key=repo-00000001"));
        assert_eq!(removed(&calls), vec!["c1"]);
    }

    #[test]
    fn container_of_the_current_boot_is_never_removed() {
        let calls = removal_calls("c2 live-boot 5a1c0e2f9b3d4e61\n", true, SERVER, "live-boot");

        assert!(removed(&calls).is_empty());
    }

    #[test]
    fn container_of_another_server_is_never_removed() {
        let calls = removal_calls("c3 dead-boot 9f0e1d2c3b4a5968\n", true, SERVER, "live-boot");

        assert!(removed(&calls).is_empty());
    }

    #[test]
    fn docker_failure_removes_nothing_and_does_not_panic() {
        let calls = removal_calls("", false, SERVER, "live-boot");

        assert_eq!(calls.len(), 1);
        assert!(removed(&calls).is_empty());
    }
}
