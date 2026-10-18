//! Tests for the one-time orphan sweep. Each nextest case runs in its own
//! process, so pid-scoped scratch directories cannot collide between tests.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::orphan_sweep::sweep;
use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger, PromptPhase};
use ajax_core::registry::{RunRecord, SqliteRegistryStore};

/// Spawn a real `sleep 60`; the reaper thread keeps it collectable so a killed
/// child leaves no zombie that `ps` would still list.
fn spawn_sleep_child() -> (i64, std::thread::JoinHandle<std::process::ExitStatus>) {
    let child = Command::new("sleep")
        .arg("60")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn sleep helper");
    let pid = child.id() as i64;
    (pid, std::thread::spawn(move || reap(child)))
}

fn reap(mut child: Child) -> std::process::ExitStatus {
    child.wait().expect("wait on sleep helper")
}

/// Kill a pid we no longer own the `Child` for, then join the reaper.
fn kill_and_reap(pid: i64, handle: std::thread::JoinHandle<std::process::ExitStatus>) {
    Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("kill sleep helper");
    handle.join().expect("join sleep reaper");
}

/// A scratch state dir plus a fresh registry store inside it.
fn scratch_store(label: &str) -> (PathBuf, SqliteRegistryStore) {
    let dir =
        std::env::temp_dir().join(format!("ajax-orphan-sweep-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let store = SqliteRegistryStore::new(dir.join("registry.db"));
    (dir, store)
}

/// Record a run row exactly as `TaskSessionState::record_child_identity` does.
fn record_row(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    child: Option<(i64, String)>,
    owner_boot_id: &str,
) {
    crate::adapters::web_session_store::record_child_identity(
        store,
        state_dir,
        handle,
        "acp",
        child,
        owner_boot_id,
    )
    .expect("record child identity");
}

fn listed_row(store: &SqliteRegistryStore, run_id: &str) -> Option<RunRecord> {
    store
        .runs_with_children()
        .expect("list runs with children")
        .into_iter()
        .find(|run| run.run_id == run_id)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn term_ignoring_child_is_killed_before_identity_is_cleared_issue_1327() {
    use std::os::unix::process::{CommandExt, ExitStatusExt};
    use std::time::Instant;

    // Keep failure paths (including the red run) from leaving a child behind.
    struct ChildGuard {
        pid: i64,
        reaper: Option<std::thread::JoinHandle<std::process::ExitStatus>>,
    }
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(reaper) = self.reaper.take() {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{}", self.pid)])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                kill_and_reap(self.pid, reaper);
            }
        }
    }

    let (dir, store) = scratch_store("term-ignoring-1327");
    let ready = dir.join("ready");
    // Use shell builtins so the fixture has no grandchildren to orphan.
    let child = Command::new("sh")
        .args([
            "-c",
            r#"trap '' TERM; echo ready > "$1"; while :; do :; done"#,
            "sh",
        ])
        .arg(&ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .expect("spawn TERM-ignoring process-group leader");
    let pid = child.id() as i64;
    let mut child = ChildGuard {
        pid,
        reaper: Some(std::thread::spawn(move || reap(child))),
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        ready.exists(),
        "child must install its TERM trap before sweep"
    );
    let token = super::process_identity::start_token(pid).expect("live child token");
    record_row(&store, &dir, "handle", Some((pid, token)), "foreign-boot");
    let run_id = row_id_for(&store, "handle");
    let alive = || {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("probe child")
            .success()
    };

    let (result, cleared_while_alive) = std::thread::scope(|scope| {
        let sweep = scope.spawn(|| {
            super::orphan_sweep::sweep_with_grace(&store, "this-boot", Duration::from_millis(200))
        });
        let mut cleared_while_alive = false;
        while !sweep.is_finished() {
            if listed_row(&store, &run_id).is_none() && alive() {
                cleared_while_alive = true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        (sweep.join().expect("join sweep"), cleared_while_alive)
    });

    assert!(
        !alive(),
        "sweep must observe the child gone before returning"
    );
    assert!(
        !cleared_while_alive,
        "identity must remain while the child lives"
    );
    assert!(result.complete);
    assert_eq!(result.handled, 1);
    assert!(listed_row(&store, &run_id).is_none());
    let status = child
        .reaper
        .take()
        .unwrap()
        .join()
        .expect("join child reaper");
    assert_eq!(
        status.signal(),
        Some(9),
        "SIGKILL escalation must end the child"
    );
    cleanup(&dir);
}

#[test]
fn matching_token_terminates_child_and_clears_row() {
    let (dir, store) = scratch_store("matching");
    let (pid, reaper) = spawn_sleep_child();
    let token = super::process_identity::start_token(pid).expect("live child token");
    record_row(
        &store,
        &dir,
        "handle",
        Some((pid, token.clone())),
        "foreign-boot",
    );

    let result = sweep(&store, "this-boot");
    assert!(result.complete);
    let handled = result.handled;

    assert_eq!(handled, 1, "the matching orphan must be handled");
    assert!(
        listed_row(&store, row_id_for(&store, "handle").as_str()).is_none(),
        "the run row's child fields must be cleared"
    );
    let status = reaper.join().expect("join sleep reaper");
    assert!(
        !status.success(),
        "a terminated child must not exit successfully"
    );
    cleanup(&dir);
}

#[test]
fn mismatching_token_leaves_child_and_row_untouched() {
    let (dir, store) = scratch_store("mismatch");
    let (pid, reaper) = spawn_sleep_child();
    record_row(
        &store,
        &dir,
        "handle",
        Some((pid, "Thu Jan  1 00:00:00 1970".to_owned())),
        "foreign-boot",
    );

    let result = sweep(&store, "this-boot");
    assert!(result.complete);
    let handled = result.handled;

    assert_eq!(handled, 0, "a mismatched token must never be signalled");
    std::thread::sleep(Duration::from_millis(300));
    assert!(!reaper.is_finished(), "the child must still be alive");
    let row = listed_row(&store, row_id_for(&store, "handle").as_str())
        .expect("the run row must remain recorded");
    assert_eq!(
        row.child,
        Some((pid, pid)),
        "the child fields must be unchanged"
    );
    kill_and_reap(pid, reaper);
    cleanup(&dir);
}

#[test]
fn same_boot_id_is_never_touched() {
    let (dir, store) = scratch_store("own-boot");
    let (pid, reaper) = spawn_sleep_child();
    record_row(
        &store,
        &dir,
        "handle",
        Some((pid, super::process_identity::start_token(pid).unwrap())),
        "this-boot",
    );

    let result = sweep(&store, "this-boot");
    assert!(result.complete);
    let handled = result.handled;

    assert_eq!(handled, 0, "a run owned by this host must be skipped");
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !reaper.is_finished(),
        "the live child of this host must survive"
    );
    let row = listed_row(&store, row_id_for(&store, "handle").as_str())
        .expect("the run row must remain recorded");
    assert_eq!(row.child, Some((pid, pid)));
    kill_and_reap(pid, reaper);
    cleanup(&dir);
}

#[test]
fn dead_pid_clears_row_without_signalling() {
    let (dir, store) = scratch_store("dead");
    // Spawn and reap a short-lived process so we have a known-dead pid.
    let mut child = Command::new("true")
        .stdin(Stdio::null())
        .spawn()
        .expect("spawn true helper");
    let pid = child.id() as i64;
    child.wait().expect("wait on true helper");

    record_row(
        &store,
        &dir,
        "handle",
        Some((pid, "any-token".to_owned())),
        "foreign-boot",
    );

    let result = sweep(&store, "this-boot");
    assert!(result.complete);
    let handled = result.handled;

    assert_eq!(handled, 1, "a dead orphan must be cleared");
    assert!(
        listed_row(&store, row_id_for(&store, "handle").as_str()).is_none(),
        "the run row's child fields must be cleared"
    );
    cleanup(&dir);
}

#[test]
fn matching_token_terminates_the_whole_process_group_issue_1298() {
    use std::os::unix::process::CommandExt;

    let (dir, store) = scratch_store("group");
    let pidfile = dir.join("grandchild.pid");
    let mut cmd = Command::new("sh");
    cmd.args([
        "-c",
        r#"sleep 300 & echo $! > "$1"; exec sleep 300"#,
        "sh",
        pidfile.to_str().expect("pidfile path is unicode"),
    ]);
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    cmd.process_group(0);
    let child = cmd.spawn().expect("spawn sh process-group helper");
    let pid = child.id() as i64;
    let reaper = std::thread::spawn(move || reap(child));

    // Wait for the backgrounded grandchild's pid to be recorded.
    let mut grandchild: Option<i64> = None;
    for _ in 0..20 {
        if let Ok(contents) = std::fs::read_to_string(&pidfile) {
            if let Ok(p) = contents.trim().parse::<i64>() {
                grandchild = Some(p);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let grandchild_pid = grandchild.expect("grandchild pid file must appear within 2s");

    let token = super::process_identity::start_token(pid).expect("live child token");
    record_row(&store, &dir, "handle", Some((pid, token)), "foreign-boot");

    let result = sweep(&store, "this-boot");
    assert!(result.complete);
    let handled = result.handled;
    assert_eq!(handled, 1, "the matching orphan must be handled");

    // The grandchild shares the child's process group and must die too (issue #1298).
    let pid_alive = |p: i64| {
        Command::new("kill")
            .args(["-0", &p.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    let mut gone = false;
    for _ in 0..30 {
        if !pid_alive(grandchild_pid) {
            gone = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !gone {
        let _ = Command::new("kill")
            .args(["-KILL", &grandchild_pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        panic!("issue 1298: grandchild {grandchild_pid} of group {pid} survived the sweep's TERM");
    }

    let status = reaper.join().expect("join sh helper reaper");
    assert!(
        !status.success(),
        "a terminated child must not exit successfully"
    );
    cleanup(&dir);
}

/// The run id for a handle, as `record_child_identity` derives it.
fn row_id_for(store: &SqliteRegistryStore, handle: &str) -> String {
    store
        .latest_run_for_task(handle)
        .expect("load run")
        .expect("run must exist")
        .run_id
}

#[test]
fn sweep_once_interrupts_dispatching_prompts_of_dead_hosts() {
    let (dir, store) = scratch_store("recovery");

    record_row(&store, &dir, "handle", None, "foreign-boot");

    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_string(),
        "transcript text".to_string(),
        "prompt text".to_string(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("p1"));
    prompt_ledger::try_persist(&store, &dir, "handle", &ledger).expect("persist ledger");

    let recovered_handles = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    assert!(recovered_handles.complete);
    assert_eq!(
        recovered_handles.recovered_handles,
        vec!["handle".to_string()]
    );

    let recovered = prompt_ledger::try_load(&store, &dir, "handle").expect("load ledger");
    let entry = recovered.entry("p1").expect("p1 must exist in the ledger");
    assert_eq!(entry.phase, PromptPhase::Interrupted);
    cleanup(&dir);
}

#[test]
fn sweep_once_retries_listing_failure_issue_1326() {
    let (dir, store) = scratch_store("listing-retry");
    let (pid, reaper) = spawn_sleep_child();
    record_row(
        &store,
        &dir,
        "handle",
        Some((pid, super::process_identity::start_token(pid).unwrap())),
        "foreign-boot",
    );
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued("p1".into(), "text".into(), "prompt".into(), Vec::new());
    assert!(ledger.mark_dispatching("p1"));
    prompt_ledger::try_persist(&store, &dir, "handle", &ledger).unwrap();

    let database = dir.join("registry.db");
    let saved = dir.join("registry.saved");
    std::fs::rename(&database, &saved).unwrap();
    std::fs::create_dir(&database).unwrap();
    assert!(store.runs_with_children().is_err());
    let first = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    std::fs::remove_dir(&database).unwrap();
    std::fs::rename(&saved, &database).unwrap();
    assert!(
        !reaper.is_finished(),
        "failed listing must leave the child alive"
    );
    let second = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");

    for _ in 0..100 {
        if reaper.is_finished() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let signalled = reaper.is_finished();
    if signalled {
        assert!(!reaper.join().unwrap().success());
    } else {
        kill_and_reap(pid, reaper);
    }
    assert!(signalled, "retry must signal the orphan child");
    assert!(!first.complete);
    assert!(first.recovered_handles.is_empty());
    assert!(second.complete);
    assert_eq!(second.recovered_handles, vec!["handle".to_string()]);
    assert!(listed_row(&store, &row_id_for(&store, "handle")).is_none());
    let recovered = prompt_ledger::try_load(&store, &dir, "handle").unwrap();
    assert_eq!(
        recovered.entry("p1").unwrap().phase,
        PromptPhase::Interrupted
    );

    // A new foreign row after completion must not be scanned or recovered.
    record_row(
        &store,
        &dir,
        "later",
        Some((-1, "gone".into())),
        "foreign-boot",
    );
    prompt_ledger::try_persist(&store, &dir, "later", &ledger).unwrap();
    let third = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    assert!(third.complete);
    assert!(third.recovered_handles.is_empty());
    assert!(listed_row(&store, &row_id_for(&store, "later")).is_some());
    let untouched = prompt_ledger::try_load(&store, &dir, "later").unwrap();
    assert_eq!(
        untouched.entry("p1").unwrap().phase,
        PromptPhase::Dispatching
    );
    cleanup(&dir);
}
