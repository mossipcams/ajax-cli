use super::execution_ownership::ExecutionOwnership;
use super::{test_support, SessionStorage};
use ajax_core::registry::OwnerClaim;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
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
fn directory_writes_before_first_claim_issue_1327() {
    use super::TaskSessionDirectory;
    use crate::adapters::web_session_store::{try_load, try_save_meta};

    let dir = test_support::scratch_dir("exec-ownership-before-claim");
    let directory = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));
    for (index, storage) in directory.execution_storages_for_test().iter().enumerate() {
        let handle = format!("web/before-claim-{index}");
        try_save_meta(
            &storage.registry_store(),
            &dir,
            &handle,
            "codex",
            Some("idle-session"),
            "idle-model",
        )
        .expect("an unclaimed directory must allow idle writes");
        let saved =
            try_load::<serde_json::Value>(&storage.registry_store(), &dir, &handle, "codex")
                .unwrap();
        assert_eq!(saved.acp_session_id.as_deref(), Some("idle-session"));
        assert_eq!(saved.model, "idle-model");
    }
    drop(directory);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn takeover_fences_directory_and_recovery_writes_issue_1327() {
    use super::TaskSessionDirectory;
    use crate::adapters::web_session_store::{try_load, try_save_meta};

    let dir = test_support::scratch_dir("exec-ownership-fenced-writes");
    let first = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));
    first
        .startup_dispatch_handles()
        .expect("first owner claims");
    for (index, storage) in first.execution_storages_for_test().iter().enumerate() {
        try_save_meta(
            &storage.registry_store(),
            &dir,
            &format!("web/fenced-{index}"),
            "codex",
            Some("original-session"),
            "original-model",
        )
        .expect("first owner commits before takeover");
    }

    // Make the first lease stale so the second directory takes the existing
    // stale-owner claim path without waiting for the timeout.
    assert!(SessionStorage::under(dir.clone())
        .registry_store()
        .heartbeat_execution_owner(&first.ownership_boot_id(), 0)
        .unwrap());
    let second = TaskSessionDirectory::new(first.execution_storages_for_test()[0].clone());
    second.startup_dispatch_handles().expect("stale takeover");
    assert_ne!(first.ownership_boot_id(), second.ownership_boot_id());

    for (index, (old, new)) in first
        .execution_storages_for_test()
        .iter()
        .zip(second.execution_storages_for_test())
        .enumerate()
    {
        let handle = format!("web/fenced-{index}");
        let error = try_save_meta(
            &old.registry_store(),
            &dir,
            &handle,
            "claude",
            Some("stale-session"),
            "stale-model",
        )
        .expect_err("displaced directory and recovery storage must reject commits");
        assert!(error.to_string().contains("fenced"), "{error}");
        let unchanged =
            try_load::<serde_json::Value>(&new.registry_store(), &dir, &handle, "codex").unwrap();
        assert_eq!(unchanged.harness, "codex");
        assert_eq!(
            unchanged.acp_session_id.as_deref(),
            Some("original-session")
        );
        assert_eq!(unchanged.model, "original-model");

        try_save_meta(
            &new.registry_store(),
            &dir,
            &handle,
            "claude",
            Some("current-session"),
            "current-model",
        )
        .expect("current directory and recovery storage must commit");
        let updated =
            try_load::<serde_json::Value>(&new.registry_store(), &dir, &handle, "claude").unwrap();
        assert_eq!(updated.harness, "claude");
        assert_eq!(updated.acp_session_id.as_deref(), Some("current-session"));
        assert_eq!(updated.model, "current-model");
    }
    drop(first);
    drop(second);
    std::fs::remove_dir_all(dir).unwrap();
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

#[test]
fn holder_is_dead_only_on_positive_evidence() {
    use super::execution_ownership::holder_is_dead;

    // Exit 1 with empty stdout is the only positive evidence of death.
    assert!(holder_is_dead(Some(1), true));

    // Everything else — including probes that never ran — means alive.
    assert!(!holder_is_dead(Some(0), false));
    assert!(!holder_is_dead(Some(0), true));
    assert!(!holder_is_dead(Some(1), false));
    assert!(!holder_is_dead(Some(2), true));
    assert!(!holder_is_dead(None, true));
}

#[test]
fn lost_lease_is_detected_by_heartbeat_and_blocks_ensure_owner() {
    let dir = test_support::scratch_dir("exec-ownership-lost-heartbeat");
    let first_storage = SessionStorage::under(dir.clone());
    let first = ExecutionOwnership::new(first_storage);
    assert!(first.ensure_owner().is_ok());

    let second = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(second.ensure_owner().is_ok());

    // The first boot no longer owns the lease; its heartbeat marks it lost.
    first.heartbeat();

    let err = first
        .check()
        .expect_err("the lease must be lost after heartbeat");
    assert!(err.contains("ownership lost"), "unexpected error: {err}");
}

#[test]
fn lost_to_a_live_foreign_host_stays_lost_and_is_refused() {
    let dir = test_support::scratch_dir("exec-ownership-lost-live-foreign");
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
    ownership.mark_lost_for_test();

    // A live foreign holder keeps the refusal in place.
    ownership.heartbeat();
    assert!(ownership.check().is_err());

    let err = ownership
        .ensure_owner()
        .expect_err("a live foreign owner must refuse");
    assert!(
        err.contains("another Ajax web host"),
        "unexpected error: {err}"
    );

    child.kill().expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");
}

#[test]
fn lost_lease_recovers_when_the_foreign_host_dies() {
    let dir = test_support::scratch_dir("exec-ownership-lost-dead-recovery");
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
    ownership.mark_lost_for_test();

    child.kill().expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");

    // The next heartbeat tick re-claims the dead host's lease.
    ownership.heartbeat();
    assert!(ownership.check().is_ok());
    assert!(ownership.ensure_owner().is_ok());
}

#[test]
fn lost_after_heartbeat_errors_recovers_when_the_database_is_healthy_again() {
    let dir = test_support::scratch_dir("exec-ownership-lost-db-recovery");
    // Make the registry database path a directory so it cannot be opened.
    std::fs::create_dir_all(dir.join("ajax.db")).expect("create bogus db dir");

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir.clone()));

    ownership.heartbeat();
    ownership.heartbeat();
    ownership.heartbeat();
    assert!(ownership.check().is_err());

    std::fs::remove_dir_all(dir.join("ajax.db")).expect("remove bogus db dir");

    // The next heartbeat tick finds a healthy database and recovers.
    ownership.heartbeat();
    assert!(ownership.check().is_ok());
}

#[test]
fn acquire_retry_recovers_a_lost_lease() {
    let dir = test_support::scratch_dir("exec-ownership-lost-acquire-retry");
    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    ownership.mark_lost_for_test();

    assert!(ownership.ensure_owner().is_ok());
    assert!(ownership.check().is_ok());
}

#[test]
fn repeated_heartbeat_errors_mark_the_lease_lost() {
    let dir = test_support::scratch_dir("exec-ownership-heartbeat-errors");
    // Make the registry database path a directory so it cannot be opened.
    std::fs::create_dir_all(dir.join("ajax.db")).expect("create bogus db dir");

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));

    ownership.heartbeat();
    assert!(ownership.check().is_ok());
    ownership.heartbeat();
    assert!(ownership.check().is_ok());

    // The third consecutive failure is positive enough evidence.
    ownership.heartbeat();
    let err = ownership
        .check()
        .expect_err("three heartbeat failures must mark the lease lost");
    assert!(err.contains("ownership lost"), "unexpected error: {err}");
}

#[test]
fn healthy_ownership_never_becomes_lost() {
    let dir = test_support::scratch_dir("exec-ownership-healthy");
    let ownership = ExecutionOwnership::new(SessionStorage::under(dir));
    assert!(ownership.ensure_owner().is_ok());

    for _ in 0..5 {
        ownership.heartbeat();
    }

    assert!(ownership.check().is_ok());
}

#[test]
fn lost_ownership_refuses_prompts_and_leaves_the_transcript_unchanged() {
    use super::test_support::{fake_acp_fixture, BlockingSessionDirectory};
    use crate::adapters::web_session_acp::with_test_acp_program;
    use ajax_core::models::AgentClient;

    let dir = test_support::scratch_dir("exec-ownership-prompt-refused");
    let handle = "web/exec-ownership-prompt-refused";
    let directory = BlockingSessionDirectory::new(dir.clone());
    let script = fake_acp_fixture();

    with_test_acp_program(&script, || {
        directory
            .acquire(handle, &dir, "auto", AgentClient::Cursor)
            .expect("acquire");
        let (before, cursor) = directory.read_from(handle, 0);

        // A second boot in the same process takes over the lease.
        let foreign = ExecutionOwnership::new(SessionStorage::under(dir.clone()));
        assert!(foreign.ensure_owner().is_ok());

        directory.inner().heartbeat_execution_ownership();

        let result = directory.submit_prompt_with_id(handle, "p1".to_string(), "hello".to_string());
        assert!(
            result.is_err(),
            "a lost ownership lease must refuse the prompt"
        );

        let err = result.expect_err("prompt refused");
        assert!(err.contains("ownership lost"), "unexpected error: {err}");

        let (after, next) = directory.read_from(handle, 0);
        assert_eq!(
            (after, next),
            (before, cursor),
            "a refused prompt must leave the transcript untouched"
        );
    });
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn recovered_turns_that_fail_to_report_are_retried_on_the_next_heartbeat() {
    use super::{SessionActivity, TaskSessionDirectory};

    let dir = test_support::scratch_dir("exec-ownership-requeue");
    let directory = TaskSessionDirectory::new(SessionStorage::under(dir.clone()));

    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    directory.set_report_session_activity(Arc::new(move |_: &str, _: SessionActivity| {
        // The first report fails; later reports succeed.
        count.fetch_add(1, Ordering::SeqCst) > 0
    }));

    directory.requeue_recovered_handles_for_test(vec!["web/requeue".to_string()]);

    directory.heartbeat_execution_ownership();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the first heartbeat must report the recovered turn once"
    );

    directory.heartbeat_execution_ownership();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a handle whose report failed must be retried on the next heartbeat"
    );

    directory.heartbeat_execution_ownership();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "a handle whose report succeeded must not be reported again"
    );

    let _ = std::fs::remove_dir_all(dir);
}
#[test]
fn healthy_heartbeat_retries_an_incomplete_startup_sweep_issue_1326() {
    use super::process_identity;
    use crate::adapters::web_session_store::{
        prompt_ledger,
        prompt_ledger::{PromptLedger, PromptPhase},
        record_child_identity,
    };
    use ajax_core::registry::SqliteRegistryStore;

    let dir = test_support::scratch_dir("exec-ownership-sweep-retry");

    // Seed an orphan run row and a Dispatching prompt, exactly as a dead host
    // would leave them, reusing the setup of
    // `sweep_once_retries_listing_failure_issue_1326`.
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut child = std::process::Command::new("sleep")
        .arg("60")
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("spawn sleep helper");
    let pid = child.id() as i64;
    record_child_identity(
        &store,
        &dir,
        "handle",
        "acp",
        Some((
            pid,
            process_identity::start_token(pid).expect("live child token"),
        )),
        "foreign-boot",
    )
    .expect("record child identity");
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued("p1".into(), "text".into(), "prompt".into(), Vec::new());
    assert!(ledger.mark_dispatching("p1"));
    prompt_ledger::try_persist(&store, &dir, "handle", &ledger).expect("persist ledger");

    // Make the startup sweep's recovery persist fail, so the first sweep is
    // INCOMPLETE and recovers nothing.
    let guard = prompt_ledger::ForcePersistFailGuard::enable();

    let ownership = ExecutionOwnership::new(SessionStorage::under(dir.clone()));
    assert!(
        ownership.ensure_owner().is_ok(),
        "the claim itself must succeed"
    );
    assert!(
        ownership.take_recovered_handles().is_empty(),
        "an incomplete startup sweep must not report recovered turns"
    );

    // Fix the failure: clear the injection, then one healthy heartbeat tick
    // must retry the sweep and complete it.
    drop(guard);
    ownership.heartbeat();

    assert_eq!(
        ownership.take_recovered_handles(),
        vec!["handle".to_string()],
        "the heartbeat must signal the orphan and recover the turn exactly once"
    );

    // The Dispatching prompt must be interrupted on disk.
    let recovered = prompt_ledger::try_load(&store, &dir, "handle").expect("load ledger");
    assert_eq!(
        recovered.entry("p1").expect("p1 must exist").phase,
        PromptPhase::Interrupted
    );

    // A second healthy heartbeat is a no-op: the sweep already completed.
    ownership.heartbeat();
    assert!(
        ownership.take_recovered_handles().is_empty(),
        "a completed sweep must not recover the same turn again"
    );

    child.kill().expect("kill sleep helper");
    child.wait().expect("wait on sleep helper");
    let _ = std::fs::remove_dir_all(dir);
}
