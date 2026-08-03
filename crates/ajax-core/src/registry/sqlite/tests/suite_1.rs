use super::super::*;
use super::*;

#[rstest]
#[case("Claude", AgentClient::Claude)]
#[case("Codex", AgentClient::Codex)]
#[case("Cursor", AgentClient::Cursor)]
#[case("Pi", AgentClient::Pi)]
#[case("Other", AgentClient::Other)]
fn parses_agent_client_names(#[case] name: &str, #[case] expected: AgentClient) {
    assert_eq!(parse_agent_client(name).unwrap(), expected);
}

#[rstest]
#[case("Created", LifecycleStatus::Created)]
#[case("Provisioning", LifecycleStatus::Provisioning)]
#[case("Active", LifecycleStatus::Active)]
#[case("Waiting", LifecycleStatus::Waiting)]
#[case("Reviewable", LifecycleStatus::Reviewable)]
#[case("Mergeable", LifecycleStatus::Mergeable)]
#[case("Merged", LifecycleStatus::Merged)]
#[case("Cleanable", LifecycleStatus::Cleanable)]
#[case("Removing", LifecycleStatus::Removing)]
#[case("TeardownIncomplete", LifecycleStatus::TeardownIncomplete)]
#[case("Removed", LifecycleStatus::Removed)]
#[case("Orphaned", LifecycleStatus::Orphaned)]
#[case("Error", LifecycleStatus::Error)]
fn parses_lifecycle_status_names(#[case] name: &str, #[case] expected: LifecycleStatus) {
    assert_eq!(parse_lifecycle_status(name).unwrap(), expected);
}

#[rstest]
#[case("NotStarted", AgentRuntimeStatus::NotStarted)]
#[case("Running", AgentRuntimeStatus::Running)]
#[case("Waiting", AgentRuntimeStatus::Waiting)]
#[case("Blocked", AgentRuntimeStatus::Blocked)]
#[case("Dead", AgentRuntimeStatus::Dead)]
#[case("Done", AgentRuntimeStatus::Done)]
#[case("Unknown", AgentRuntimeStatus::Unknown)]
fn parses_agent_runtime_status_names(#[case] name: &str, #[case] expected: AgentRuntimeStatus) {
    assert_eq!(parse_agent_runtime_status(name).unwrap(), expected);
}

#[rstest]
#[case("Dirty", SideFlag::Dirty)]
#[case("AgentRunning", SideFlag::AgentRunning)]
#[case("AgentDead", SideFlag::AgentDead)]
#[case("NeedsInput", SideFlag::NeedsInput)]
#[case("TestsFailed", SideFlag::TestsFailed)]
#[case("TmuxMissing", SideFlag::TmuxMissing)]
#[case("WorktreeMissing", SideFlag::WorktreeMissing)]
#[case("TaskWindowMissing", SideFlag::TaskWindowMissing)]
#[case("BranchMissing", SideFlag::BranchMissing)]
#[case("Stale", SideFlag::Stale)]
#[case("Conflicted", SideFlag::Conflicted)]
#[case("Unpushed", SideFlag::Unpushed)]
fn parses_side_flag_names(#[case] name: &str, #[case] expected: SideFlag) {
    assert_eq!(parse_side_flag(name).unwrap(), expected);
}

#[rstest]
#[case("WorktreeMissing", LiveStatusKind::WorktreeMissing)]
#[case("TmuxMissing", LiveStatusKind::TmuxMissing)]
#[case("TaskWindowMissing", LiveStatusKind::TaskWindowMissing)]
#[case("ShellIdle", LiveStatusKind::ShellIdle)]
#[case("CommandRunning", LiveStatusKind::CommandRunning)]
#[case("TestsRunning", LiveStatusKind::TestsRunning)]
#[case("AgentRunning", LiveStatusKind::AgentRunning)]
#[case("WaitingForApproval", LiveStatusKind::WaitingForApproval)]
#[case("WaitingForInput", LiveStatusKind::WaitingForInput)]
#[case("Blocked", LiveStatusKind::Blocked)]
#[case("RateLimited", LiveStatusKind::RateLimited)]
#[case("AuthRequired", LiveStatusKind::AuthRequired)]
#[case("MergeConflict", LiveStatusKind::MergeConflict)]
#[case("CiFailed", LiveStatusKind::CiFailed)]
#[case("ContextLimit", LiveStatusKind::ContextLimit)]
#[case("CommandFailed", LiveStatusKind::CommandFailed)]
#[case("Done", LiveStatusKind::Done)]
#[case("Unknown", LiveStatusKind::Unknown)]
fn parses_live_status_kind_names(#[case] name: &str, #[case] expected: LiveStatusKind) {
    assert_eq!(parse_live_status_kind(name).unwrap(), expected);
}

#[rstest]
#[case("TaskCreated", RegistryEventKind::TaskCreated)]
#[case("LifecycleChanged", RegistryEventKind::LifecycleChanged)]
#[case("SubstrateChanged", RegistryEventKind::SubstrateChanged)]
#[case("UserNote", RegistryEventKind::UserNote)]
fn parses_registry_event_kind_names(#[case] name: &str, #[case] expected: RegistryEventKind) {
    assert_eq!(parse_registry_event_kind(name).unwrap(), expected);
}

#[test]
fn sqlite_registry_store_saves_and_loads_registry_state() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    registry
        .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-save-load"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(restored.list_tasks().len(), 1);
    assert_eq!(restored.list_tasks()[0].qualified_handle(), "web/fix-login");
    assert_eq!(restored.events_for_task(&TaskId::new("task-1")).len(), 2);
}

#[test]
fn sqlite_registry_store_persists_step_receipts_idempotently() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    let receipt = StepReceipt::succeeded(
        TaskId::new("task-1"),
        TaskOperationKind::Drop,
        "tmux_session_absent",
        "ajax-web-fix-login",
        r#"{"program":"tmux"}"#,
    );
    registry.record_step_receipt(receipt.clone()).unwrap();
    registry.record_step_receipt(receipt).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-step-receipts"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    let receipts = restored.step_receipts_for_task(&TaskId::new("task-1"));
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].operation, TaskOperationKind::Drop);
    assert_eq!(receipts[0].step_key, "tmux_session_absent");
    assert_eq!(receipts[0].target, "ajax-web-fix-login");
}

#[test]
fn sqlite_registry_store_persists_hard_deleted_tasks() {
    let mut registry = InMemoryRegistry::default();
    let deleted_id = TaskId::new("task-1");
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    registry
        .create_task(task("task-2", "web", "keep-task"))
        .unwrap();
    registry
        .record_event(deleted_id.clone(), RegistryEventKind::UserNote, "ready")
        .unwrap();
    registry
        .record_step_receipt(StepReceipt::succeeded(
            deleted_id.clone(),
            TaskOperationKind::Drop,
            "worktree_absent",
            "/tmp/worktrees/web-fix-login",
            "{}",
        ))
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-hard-delete"
    ));
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();

    registry.delete_task(&deleted_id).unwrap();
    store.save(&registry).unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    let deleted_task_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_tasks WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let deleted_event_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_events WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let deleted_receipt_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM step_receipts WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(deleted_task_count, 0);
    assert_eq!(deleted_event_count, 0);
    assert_eq!(deleted_receipt_count, 0);
    assert!(restored.get_task(&deleted_id).is_none());
    assert!(restored.get_task(&TaskId::new("task-2")).is_some());
}

#[test]
fn sqlite_registry_store_rejects_accidental_empty_rewrite_of_non_empty_disk() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-empty-rewrite"
    ));
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();

    let error = store.save(&InMemoryRegistry::default()).unwrap_err();

    assert!(error
        .to_string()
        .contains("refusing to save empty registry"));
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    assert!(restored.get_task(&TaskId::new("task-1")).is_some());
}

#[test]
fn sqlite_registry_store_prunes_removed_task_ghosts() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-live", "web", "fix-login"))
        .unwrap();
    let mut removed = task("task-removed", "web", "old-task");
    removed.lifecycle_status = LifecycleStatus::Removed;
    removed.add_side_flag(SideFlag::NeedsInput);
    registry.create_task(removed).unwrap();
    let mut stale = task("task-stale", "web", "stale-task");
    stale.add_side_flag(SideFlag::Stale);
    stale.add_side_flag(SideFlag::WorktreeMissing);
    stale.add_side_flag(SideFlag::BranchMissing);
    registry.create_task(stale).unwrap();
    registry
        .record_event(
            TaskId::new("task-removed"),
            RegistryEventKind::UserNote,
            "ghost note",
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-prune-removed"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert!(restored.get_task(&TaskId::new("task-live")).is_some());
    assert!(restored.get_task(&TaskId::new("task-removed")).is_none());
    assert!(restored.get_task(&TaskId::new("task-stale")).is_none());
    assert!(restored
        .events_for_task(&TaskId::new("task-removed"))
        .is_empty());
    assert_eq!(restored.list_tasks().len(), 1);
}

#[test]
fn sqlite_registry_store_persists_active_missing_substrate_tasks() {
    let mut registry = InMemoryRegistry::default();
    let mut broken = task("task-broken", "web", "fix-login");
    broken.lifecycle_status = LifecycleStatus::Active;
    broken.add_side_flag(SideFlag::TmuxMissing);
    registry.create_task(broken).unwrap();
    registry
        .create_task(task("task-live", "web", "keep-task"))
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-active-missing-substrate"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    let broken_task_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_tasks WHERE task_id = 'task-broken'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(broken_task_count, 1);
    let restored_broken = restored
        .get_task(&TaskId::new("task-broken"))
        .expect("active missing-substrate task should survive save/load");
    assert_eq!(restored_broken.lifecycle_status, LifecycleStatus::Active);
    assert!(restored_broken.has_side_flag(SideFlag::TmuxMissing));
    assert!(restored.get_task(&TaskId::new("task-live")).is_some());
}

#[test]
fn sqlite_registry_store_persists_teardown_incomplete_for_cleanup_retry() {
    let mut registry = InMemoryRegistry::default();
    let mut incomplete = task("task-incomplete", "web", "fix-login");
    incomplete.lifecycle_status = LifecycleStatus::TeardownIncomplete;
    incomplete.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
    incomplete.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    registry.create_task(incomplete).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-teardown-incomplete-retry"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    let task = restored
        .get_task(&TaskId::new("task-incomplete"))
        .expect("teardown-incomplete task with remaining worktree should persist");
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
}

#[test]
fn sqlite_registry_store_retains_events_and_receipts_for_persisted_missing_substrate_tasks() {
    let mut registry = InMemoryRegistry::default();
    let mut broken = task("task-broken", "web", "fix-login");
    broken.lifecycle_status = LifecycleStatus::Active;
    broken.add_side_flag(SideFlag::WorktreeMissing);
    registry.create_task(broken).unwrap();
    registry
        .record_event(
            TaskId::new("task-broken"),
            RegistryEventKind::UserNote,
            "operator context",
        )
        .unwrap();
    registry
        .record_step_receipt(StepReceipt::succeeded(
            TaskId::new("task-broken"),
            TaskOperationKind::Drop,
            "tmux_session_absent",
            "ajax-web-fix-login",
            "{}",
        ))
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-missing-substrate-history"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert!(restored.get_task(&TaskId::new("task-broken")).is_some());
    let events = restored.events_for_task(&TaskId::new("task-broken"));
    assert!(
        events
            .iter()
            .any(|event| event.message == "operator context"),
        "registry events should survive when the task survives"
    );
    let receipts = restored.step_receipts_for_task(&TaskId::new("task-broken"));
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].step_key, "tmux_session_absent");
}

#[test]
fn sqlite_registry_store_prunes_abandoned_provisioning_ghosts() {
    let mut registry = InMemoryRegistry::default();
    let mut ghost = task("task-ghost", "web", "fix-login");
    ghost.lifecycle_status = LifecycleStatus::Provisioning;
    ghost.add_side_flag(SideFlag::WorktreeMissing);
    ghost.add_side_flag(SideFlag::BranchMissing);
    ghost.add_side_flag(SideFlag::TmuxMissing);
    registry.create_task(ghost).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-abandoned-provisioning"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert!(restored.get_task(&TaskId::new("task-ghost")).is_none());
}

#[test]
fn sqlite_registry_store_persists_cleanable_missing_substrate_for_cleanup_retry() {
    let mut registry = InMemoryRegistry::default();
    let mut cleanable = task("task-cleanable", "web", "fix-login");
    cleanable.lifecycle_status = LifecycleStatus::Cleanable;
    cleanable.add_side_flag(SideFlag::WorktreeMissing);
    registry.create_task(cleanable).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-cleanable-missing-substrate"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    let task = restored
        .get_task(&TaskId::new("task-cleanable"))
        .expect("cleanable task with missing substrate should persist for tidy retry");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Cleanable);
    assert!(task.has_side_flag(SideFlag::WorktreeMissing));
}

#[test]
fn sqlite_registry_store_round_trips_full_task_state_without_json_payloads() {
    let mut registry = InMemoryRegistry::default();
    let mut task = task("task-1", "web", "fix-login");
    task.lifecycle_status = LifecycleStatus::Waiting;
    task.agent_status = AgentRuntimeStatus::Blocked;
    task.created_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, 123);
    task.last_activity_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_100, 456);
    task.add_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::Conflicted);
    task.metadata
        .insert("review".to_string(), "requested".to_string());
    task.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Claude,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_010, 789),
        finished_at: Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_020, 987)),
        status: AgentRuntimeStatus::Dead,
    });
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 2,
        behind: 1,
        merged: false,
        untracked_files: 3,
        unpushed_commits: 4,
        conflicted: true,
        last_commit: Some("abc123 Fix login".to_string()),
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present("task", "/tmp/web"));
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    registry.create_task(task).unwrap();
    let expected_runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_110, 654),
        RuntimeObservationSource::CommandResult,
    );
    registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .runtime_projection = expected_runtime_projection.clone();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "full-task-round-trip"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(restored_task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(restored_task.agent_status, AgentRuntimeStatus::Blocked);
    assert_eq!(
        restored_task.created_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, 123)
    );
    assert_eq!(
        restored_task.last_activity_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_100, 456)
    );
    assert!(restored_task.has_side_flag(SideFlag::NeedsInput));
    assert!(restored_task.has_side_flag(SideFlag::Conflicted));
    assert_eq!(
        restored_task.metadata.get("review").map(String::as_str),
        Some("requested")
    );
    assert_eq!(restored_task.agent_attempts.len(), 1);
    assert_eq!(restored_task.agent_attempts[0].agent, AgentClient::Claude);
    assert_eq!(
        restored_task.agent_attempts[0].started_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_010, 789)
    );
    assert_eq!(
        restored_task.git_status,
        registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .git_status
    );
    assert_eq!(
        restored_task.tmux_status,
        registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .tmux_status
    );
    assert_eq!(
        restored_task.task_window_status,
        registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .task_window_status
    );
    assert_eq!(
        restored_task.live_status,
        registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .live_status
    );
    assert_eq!(
        restored_task.runtime_projection,
        expected_runtime_projection
    );
}

#[test]
fn sqlite_registry_store_round_trips_task_intent_and_workflow_from_normalized_tables() {
    let mut registry = InMemoryRegistry::default();
    let mut task = task("task-1", "web", "fix-login");
    task.lifecycle_status = LifecycleStatus::Waiting;
    task.agent_status = AgentRuntimeStatus::Blocked;
    task.created_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, 123);
    task.last_activity_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_100, 456);
    task.attention_acknowledged_at =
        Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_200, 789));
    registry.create_task(task).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "task-intent-workflow"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let task_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_tasks WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let workflow_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_workflow WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(task_rows, 1);
    assert_eq!(workflow_rows, 1);
    assert_eq!(restored_task.repo, "web");
    assert_eq!(restored_task.handle, "fix-login");
    assert_eq!(restored_task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(restored_task.agent_status, AgentRuntimeStatus::Blocked);
    assert_eq!(
        restored_task.created_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_000, 123)
    );
    assert_eq!(
        restored_task.last_activity_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_100, 456)
    );
    assert_eq!(
        restored_task.attention_acknowledged_at,
        Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_200, 789))
    );
}

#[test]
fn sqlite_registry_store_round_trips_runtime_probe_failure() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    let expected_runtime_projection = RuntimeProjection::with_observation_error(
        RuntimeHealth::Healthy,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_110, 654),
        RuntimeObservationSource::TmuxProbe,
        "tmux server unavailable",
    );
    registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .runtime_projection = expected_runtime_projection.clone();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}-probe-error.db",
        std::process::id(),
        "runtime"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .runtime_projection,
        expected_runtime_projection
    );
}

#[test]
fn sqlite_registry_store_round_trips_checkout_mismatch_runtime_health() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    let expected_runtime_projection = RuntimeProjection::new(
        RuntimeHealth::CheckoutMismatch,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_120, 0),
        RuntimeObservationSource::TmuxProbe,
    );
    registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .runtime_projection = expected_runtime_projection.clone();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}-checkout-mismatch.db",
        std::process::id(),
        "runtime"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .runtime_projection,
        expected_runtime_projection
    );
}

#[test]
fn sqlite_registry_store_normalizes_legacy_waiting_to_active_runtime_condition() {
    let mut registry = InMemoryRegistry::default();
    let mut legacy_task = task("task-1", "web", "fix-login");
    legacy_task.lifecycle_status = LifecycleStatus::Waiting;
    legacy_task.agent_status = AgentRuntimeStatus::Waiting;
    registry.create_task(legacy_task).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-legacy-waiting.db",
        std::process::id()
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(restored_task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(
        restored_task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}
