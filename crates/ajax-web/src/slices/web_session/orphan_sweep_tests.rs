//! Tests for the one-time orphan sweep. Each nextest case runs in its own
//! process, so pid-scoped scratch directories cannot collide between tests.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use super::orphan_sweep::sweep;
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

    let handled = sweep(&store, "this-boot");

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

    let handled = sweep(&store, "this-boot");

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

    let handled = sweep(&store, "this-boot");

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

    let handled = sweep(&store, "this-boot");

    assert_eq!(handled, 1, "a dead orphan must be cleared");
    assert!(
        listed_row(&store, row_id_for(&store, "handle").as_str()).is_none(),
        "the run row's child fields must be cleared"
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
