use super::execution_ownership::ExecutionOwnership;
use super::{test_support, SessionStorage};
use ajax_core::registry::OwnerClaim;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

#[test]
fn first_ensure_owner_is_ok_and_repeats_succeed() {
    let dir = test_support::scratch_dir("exec-ownership-first");
    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(ownership.ensure_owner().is_ok());
    assert!(ownership.ensure_owner().is_ok());
}

#[test]
fn second_directory_in_same_process_takes_over_lease() {
    let dir = test_support::scratch_dir("exec-ownership-same-process");
    let first_storage = SessionStorage::under(dir.clone());
    let first = ExecutionOwnership::new(first_storage.clone());
    assert!(first.ensure_owner().is_ok());

    let second = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(second.ensure_owner().is_ok());

    // The first boot id no longer owns the lease, so its heartbeat fails.
    let store = first_storage.registry_store();
    assert!(!store
        .heartbeat_execution_owner(first.boot_id(), now_ms())
        .unwrap());
}

#[test]
fn live_foreign_owner_is_refused() {
    let dir = test_support::scratch_dir("exec-ownership-live-foreign");
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep helper");
    let foreign_pid = child.id() as i64;

    let store = SessionStorage::under(dir.clone()).registry_store();
    let claim = store
        .claim_execution_owner("other", foreign_pid, now_ms(), 30_000)
        .unwrap();
    assert!(matches!(claim, OwnerClaim::Acquired { .. }));

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    let err = ownership
        .ensure_owner()
        .expect_err("a live foreign owner must refuse");
    assert!(
        err.contains("another Ajax web host"),
        "unexpected error: {err}"
    );
    assert!(
        err.contains(&foreign_pid.to_string()),
        "unexpected error: {err}"
    );

    child.kill().expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");
}

#[test]
fn dead_owner_is_replaced_immediately() {
    let dir = test_support::scratch_dir("exec-ownership-dead-owner");
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .expect("spawn sleep helper");
    let foreign_pid = child.id() as i64;

    let store = SessionStorage::under(dir.clone()).registry_store();
    let claim = store
        .claim_execution_owner("other", foreign_pid, now_ms(), 30_000)
        .unwrap();
    assert!(matches!(claim, OwnerClaim::Acquired { .. }));

    child.kill().expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(
        ownership.ensure_owner().is_ok(),
        "a dead owner must be replaced without waiting for the lease to go stale"
    );
}

#[test]
fn dropping_ownership_releases_the_lease() {
    let dir = test_support::scratch_dir("exec-ownership-drop");
    let storage = SessionStorage::under(dir.clone());
    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(ownership.ensure_owner().is_ok());
    drop(ownership);

    let claim = storage
        .registry_store()
        .claim_execution_owner("x", 1, now_ms(), 30_000)
        .unwrap();
    match claim {
        OwnerClaim::Acquired { previous: None } => {}
        other => panic!("expected a clean acquire after drop, got {other:?}"),
    }
}
#[test]
fn unreadable_store_reports_stored_session_unreadable() {
    let dir = test_support::scratch_dir("exec-ownership-unreadable");
    // Make the registry database path a directory so it cannot be opened.
    std::fs::create_dir_all(dir.join("ajax.db")).expect("create bogus db dir");

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    let err = ownership
        .ensure_owner()
        .expect_err("an unreadable store must fail");
    assert!(
        err.contains("stored session is unreadable"),
        "expected 'stored session is unreadable', got: {err}"
    );
    assert!(
        err.contains("ownership unavailable"),
        "expected 'ownership unavailable', got: {err}"
    );
}
