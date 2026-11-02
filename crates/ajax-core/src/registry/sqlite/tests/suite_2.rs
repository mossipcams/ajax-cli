use super::super::*;
use super::*;

#[test]
fn sqlite_registry_store_normalizes_legacy_unknown_to_not_observed() {
    let mut registry = InMemoryRegistry::default();
    let mut legacy_task = task("task-1", "web", "fix-login");
    legacy_task.agent_status = AgentRuntimeStatus::Unknown;
    legacy_task.live_status = Some(LiveObservation::new(LiveStatusKind::Unknown, "unknown"));
    registry.create_task(legacy_task).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-legacy-unknown.db",
        std::process::id()
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(restored_task.agent_status, AgentRuntimeStatus::NotStarted);
    assert!(restored_task.live_status.is_none());
    assert_eq!(
        restored_task
            .runtime_projection
            .observation_error
            .as_deref(),
        Some("agent status not observed")
    );
}

#[test]
fn sqlite_registry_store_rejects_incomplete_agent_attempt_finished_timestamp() {
    let mut registry = InMemoryRegistry::default();
    let mut task = task("task-1", "web", "fix-login");
    task.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_010, 789),
        finished_at: Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_020, 987)),
        status: AgentRuntimeStatus::Dead,
    });
    registry.create_task(task).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "incomplete-agent-attempt"
    ));
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE registry_agent_attempts SET finished_at_subsec_nanos = NULL \
             WHERE task_id = ?1",
            ["task-1"],
        )
        .unwrap();
    drop(connection);

    let error = store.load().unwrap_err();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        error,
        RegistrySnapshotError::Decode("agent attempt finished timestamp is incomplete".to_string())
    );
}

#[test]
fn sqlite_registry_store_round_trips_unfinished_agent_attempt() {
    let mut registry = InMemoryRegistry::default();
    let mut task = task("task-1", "web", "fix-login");
    task.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_010, 789),
        finished_at: None,
        status: AgentRuntimeStatus::Running,
    });
    registry.create_task(task).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "unfinished-agent-attempt"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(restored_task.agent_attempts.len(), 1);
    assert_eq!(restored_task.agent_attempts[0].finished_at, None);
    assert_eq!(
        restored_task.agent_attempts[0].status,
        AgentRuntimeStatus::Running
    );
}

#[test]
fn sqlite_registry_store_round_trips_typed_event_rows_in_order() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    registry.events.clear();
    registry.events.push(RegistryEvent {
        task_id: TaskId::new("task-1"),
        kind: RegistryEventKind::UserNote,
        message: "first".to_string(),
        occurred_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_030, 111),
    });
    registry.events.push(RegistryEvent {
        task_id: TaskId::new("task-1"),
        kind: RegistryEventKind::LifecycleChanged,
        message: "second".to_string(),
        occurred_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_040, 222),
    });
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "event-round-trip"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let payload_columns = table_columns(&connection, "registry_events")
        .into_iter()
        .filter(|column| column == "payload")
        .count();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let events = restored.events_for_task(&TaskId::new("task-1"));

    assert_eq!(payload_columns, 0);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, RegistryEventKind::UserNote);
    assert_eq!(events[0].message, "first");
    assert_eq!(
        events[0].occurred_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_030, 111)
    );
    assert_eq!(events[1].kind, RegistryEventKind::LifecycleChanged);
    assert_eq!(events[1].message, "second");
    assert_eq!(
        events[1].occurred_at,
        SystemTime::UNIX_EPOCH + Duration::new(1_700_000_040, 222)
    );
}

#[test]
fn sqlite_registry_store_round_trips_substrate_events_and_evidence() {
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    registry
        .update_git_status(
            &TaskId::new("task-1"),
            GitStatus {
                worktree_exists: true,
                branch_exists: true,
                current_branch: Some("ajax/fix-login".to_string()),
                dirty: true,
                ahead: 1,
                behind: 0,
                merged: false,
                untracked_files: 1,
                unpushed_commits: 1,
                conflicted: false,
                last_commit: Some("abc123".to_string()),
            },
        )
        .unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "substrate-event-round-trip"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let task = restored.get_task(&TaskId::new("task-1")).unwrap();
    let events = restored.events_for_task(&TaskId::new("task-1"));

    assert_eq!(
        task.git_status
            .as_ref()
            .and_then(|status| status.last_commit.as_deref()),
        Some("abc123")
    );
    assert_eq!(events.len(), 2);
    assert_eq!(events[1].kind, RegistryEventKind::SubstrateChanged);
    assert_eq!(events[1].message, "git evidence changed");
}

#[test]
fn sqlite_registry_store_round_trips_substrate_evidence_tables() {
    let mut registry = InMemoryRegistry::default();
    let mut seeded = task("task-1", "web", "fix-login");
    seeded.lifecycle_status = LifecycleStatus::Active;
    seeded.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 1,
        behind: 0,
        merged: false,
        untracked_files: 1,
        unpushed_commits: 1,
        conflicted: false,
        last_commit: Some("abc123".to_string()),
    });
    seeded.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    seeded.task_window_status = Some(TaskWindowStatus::present("task", "/tmp/web"));
    registry.create_task(seeded).unwrap();
    let mut absent = task("task-2", "web", "no-evidence");
    absent.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(absent).unwrap();
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "substrate-evidence-tables"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let git_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_git_evidence WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let tmux_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_tmux_evidence WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let task_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_window_evidence WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let absent_git_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_git_evidence WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let absent_tmux_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_tmux_evidence WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let absent_task_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_window_evidence WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();
    let absent_task = restored.get_task(&TaskId::new("task-2")).unwrap();

    assert_eq!(git_rows, 1);
    assert_eq!(tmux_rows, 1);
    assert_eq!(task_rows, 1);
    assert_eq!(absent_git_rows, 0);
    assert_eq!(absent_tmux_rows, 0);
    assert_eq!(absent_task_rows, 0);
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
    assert!(absent_task.git_status.is_none());
    assert!(absent_task.tmux_status.is_none());
    assert!(absent_task.task_window_status.is_none());
}

#[test]
fn sqlite_registry_store_records_current_schema_version() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-schema-version"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&InMemoryRegistry::default()).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(version, 10);
}

#[test]
fn sqlite_registry_store_creates_normalized_task_tables() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-normalized-schema"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&InMemoryRegistry::default()).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let task_columns = table_columns(&connection, "registry_tasks");
    let workflow_columns = table_columns(&connection, "registry_task_workflow");
    let live_columns = table_columns(&connection, "registry_task_live_status");
    let runtime_columns = table_columns(&connection, "registry_task_runtime_projection");
    let git_columns = table_columns(&connection, "registry_task_git_evidence");
    let tmux_columns = table_columns(&connection, "registry_task_tmux_evidence");
    let task_window_columns = table_columns(&connection, "registry_task_window_evidence");
    std::fs::remove_file(&path).unwrap();

    for required in [
        "task_id",
        "repo",
        "handle",
        "title",
        "branch",
        "base_branch",
        "worktree_path",
        "tmux_session",
        "task_window",
        "selected_agent",
    ] {
        assert!(task_columns.contains(&required.to_string()), "{required}");
    }
    for forbidden in [
        "lifecycle_status",
        "agent_status",
        "live_status_kind",
        "runtime_health",
        "git_worktree_exists",
        "tmux_exists",
        "task_window_exists",
    ] {
        assert!(
            !task_columns.contains(&forbidden.to_string()),
            "{forbidden}"
        );
    }
    assert!(workflow_columns.contains(&"lifecycle_status".to_string()));
    assert!(workflow_columns.contains(&"agent_status".to_string()));
    assert!(live_columns.contains(&"live_status_kind".to_string()));
    assert!(runtime_columns.contains(&"runtime_health".to_string()));
    assert!(git_columns.contains(&"git_worktree_exists".to_string()));
    assert!(tmux_columns.contains(&"tmux_exists".to_string()));
    assert!(task_window_columns.contains(&"task_window_exists".to_string()));
}

#[test]
fn sqlite_registry_store_does_not_persist_removed_task_tombstones() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-purges-removed"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut kept = task("task-1", "web", "fix-login");
    kept.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(kept).unwrap();
    let mut removed = task("task-2", "web", "old-task");
    removed.lifecycle_status = LifecycleStatus::Removed;
    registry.create_task(removed).unwrap();
    registry
        .get_task_mut(&TaskId::new("task-2"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let removed_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_tasks WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let removed_event_count: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_events WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(removed_count, 0);
    assert_eq!(removed_event_count, 0);
    assert!(restored.get_task(&TaskId::new("task-1")).is_some());
    assert!(restored.get_task(&TaskId::new("task-2")).is_none());
}

#[test]
fn sqlite_registry_store_ignores_existing_removed_task_tombstones_on_load() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-load-skips-removed"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut live = task("task-1", "web", "fix-login");
    live.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(live).unwrap();
    let mut removed = task("task-2", "web", "old-task");
    removed.lifecycle_status = LifecycleStatus::Removed;
    registry.create_task(removed).unwrap();
    registry
        .get_task_mut(&TaskId::new("task-2"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE registry_task_workflow SET lifecycle_status = 'Removed' WHERE task_id = 'task-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert!(restored.get_task(&TaskId::new("task-1")).is_none());
    assert!(restored.get_task(&TaskId::new("task-2")).is_none());
}

#[test]
fn sqlite_registry_store_round_trips_removed_tasks_with_remaining_git_substrate() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-removed-with-substrate"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut removed = task("task-1", "web", "fix-login");
    removed.lifecycle_status = LifecycleStatus::Removed;
    removed.git_status = Some(GitStatus {
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
    registry.create_task(removed).unwrap();
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    let task = restored
        .get_task(&TaskId::new("task-1"))
        .expect("Removed tasks with remaining git substrate must survive SQLite reload");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
    assert_eq!(
        task.git_status
            .as_ref()
            .map(|status| status.worktree_exists),
        Some(true)
    );
}

#[test]
fn sqlite_registry_store_preserves_side_tables_with_normalized_schema() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-side-table-cleanup"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut kept = task("task-1", "web", "fix-login");
    kept.lifecycle_status = LifecycleStatus::Active;
    kept.add_side_flag(SideFlag::NeedsInput);
    kept.metadata
        .insert("review".to_string(), "requested".to_string());
    kept.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::new(1_700_000_010, 789),
        finished_at: None,
        status: AgentRuntimeStatus::Running,
    });
    registry.create_task(kept).unwrap();
    registry
        .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
        .unwrap();
    registry
        .record_step_receipt(StepReceipt::succeeded(
            TaskId::new("task-1"),
            TaskOperationKind::Drop,
            "tmux_session_absent",
            "ajax-web-fix-login",
            r#"{"program":"tmux"}"#,
        ))
        .unwrap();

    let mut pruned = task("task-2", "web", "old-task");
    pruned.lifecycle_status = LifecycleStatus::Removed;
    pruned.add_side_flag(SideFlag::Stale);
    registry.create_task(pruned).unwrap();

    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();

    let connection = rusqlite::Connection::open(&path).unwrap();
    let task_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_tasks WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let workflow_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_workflow WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let flag_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_side_flags WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let metadata_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_metadata WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let attempt_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_agent_attempts WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let event_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_events WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let receipt_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM step_receipts WHERE task_id = 'task-2'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let revision: i64 = connection
        .query_row(
            "SELECT value FROM registry_meta WHERE key = 'revision'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(connection);

    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(task_rows, 0);
    assert_eq!(workflow_rows, 0);
    assert_eq!(flag_rows, 0);
    assert_eq!(metadata_rows, 0);
    assert_eq!(attempt_rows, 0);
    assert_eq!(event_rows, 0);
    assert_eq!(receipt_rows, 0);
    assert_eq!(revision, 1);
    assert!(restored.get_task(&TaskId::new("task-1")).is_some());
    assert!(restored.get_task(&TaskId::new("task-2")).is_none());
}
