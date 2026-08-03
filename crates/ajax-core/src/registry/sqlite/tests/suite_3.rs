use super::super::*;
use super::*;

#[test]
fn sqlite_registry_store_migrates_v7_wide_tasks_to_current_normalized_tables() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-v7-to-current-migration"
    ));
    seed_v7_database(&path);
    let store = SqliteRegistryStore::new(&path);

    let restored = store.load().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let task_columns = table_columns(&connection, "registry_tasks");
    let workflow_columns = table_columns(&connection, "registry_task_workflow");
    let live_columns = table_columns(&connection, "registry_task_live_status");
    let runtime_columns = table_columns(&connection, "registry_task_runtime_projection");
    let git_columns = table_columns(&connection, "registry_task_git_evidence");
    let tmux_columns = table_columns(&connection, "registry_task_tmux_evidence");
    let task_window_columns = table_columns(&connection, "registry_task_window_evidence");
    std::fs::remove_file(&path).unwrap();
    let task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(version, 9);
    assert_eq!(task.title, "Fix login");
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
    assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
    assert_eq!(
        task.attention_acknowledged_at,
        Some(v7_attention_acknowledged_at())
    );
    assert!(task_columns.contains(&"repo".to_string()));
    assert!(workflow_columns.contains(&"lifecycle_status".to_string()));
    assert!(live_columns.contains(&"live_status_kind".to_string()));
    assert!(runtime_columns.contains(&"runtime_health".to_string()));
    assert!(git_columns.contains(&"git_worktree_exists".to_string()));
    assert!(tmux_columns.contains(&"tmux_exists".to_string()));
    assert!(task_window_columns.contains(&"task_window_exists".to_string()));
}

#[test]
fn sqlite_registry_store_uses_typed_columns_not_json_payloads() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "typed-columns"
    ));
    let store = SqliteRegistryStore::new(&path);

    store.save(&InMemoryRegistry::default()).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let task_columns = table_columns(&connection, "registry_tasks");
    let workflow_columns = table_columns(&connection, "registry_task_workflow");
    let live_columns = table_columns(&connection, "registry_task_live_status");
    let event_columns = table_columns(&connection, "registry_events");
    std::fs::remove_file(&path).unwrap();

    assert!(!task_columns.contains(&"payload".to_string()));
    assert!(!event_columns.contains(&"payload".to_string()));
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
    for required in [
        "lifecycle_status",
        "agent_status",
        "created_at_unix_seconds",
        "last_activity_at_unix_seconds",
        "attention_acknowledged_at_unix_seconds",
        "attention_acknowledged_at_subsec_nanos",
    ] {
        assert!(
            workflow_columns.contains(&required.to_string()),
            "{required}"
        );
    }
    for required in [
        "live_status_kind",
        "live_status_summary",
        "live_status_observed_at_unix_seconds",
        "live_status_observed_at_subsec_nanos",
    ] {
        assert!(live_columns.contains(&required.to_string()), "{required}");
    }
    for required in [
        "sequence",
        "task_id",
        "kind",
        "message",
        "occurred_at_unix_seconds",
    ] {
        assert!(event_columns.contains(&required.to_string()), "{required}");
    }
}

#[test]
fn sqlite_registry_round_trips_attention_acknowledged_at() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "ack-round-trip"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut seeded = task("task-1", "web", "fix-login");
    let acknowledged_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_500, 123_456_789);
    seeded.attention_acknowledged_at = Some(acknowledged_at);
    registry.create_task(seeded).unwrap();
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .attention_acknowledged_at,
        Some(acknowledged_at)
    );
}

#[test]
fn sqlite_registry_round_trips_live_status_observed_at() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "live-observed-round-trip"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut seeded = task("task-1", "web", "fix-login");
    let observed_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_400, 987_654_321);
    seeded.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    seeded.live_status_observed_at = Some(observed_at);
    registry.create_task(seeded).unwrap();
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .live_status_observed_at,
        Some(observed_at)
    );
}

#[test]
fn sqlite_registry_store_round_trips_live_and_runtime_tables() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "live-runtime-tables"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut seeded = task("task-1", "web", "fix-login");
    seeded.lifecycle_status = LifecycleStatus::Active;
    let live_observed_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_400, 987_654_321);
    let runtime_observed_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_500, 123_456_789);
    seeded.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    seeded.live_status_observed_at = Some(live_observed_at);
    registry.create_task(seeded).unwrap();
    {
        let stored = registry.get_task_mut(&TaskId::new("task-1")).unwrap();
        stored.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        stored.live_status_observed_at = Some(live_observed_at);
    }
    registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .runtime_projection = RuntimeProjection::with_observation_error(
        RuntimeHealth::Healthy,
        runtime_observed_at,
        RuntimeObservationSource::TmuxProbe,
        "tmux server unavailable",
    );
    let store = SqliteRegistryStore::new(&path);

    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let live_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_live_status WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let runtime_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_runtime_projection WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let restored_task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(live_rows, 1);
    assert_eq!(runtime_rows, 1);
    assert_eq!(
        restored_task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
    assert_eq!(
        restored_task.live_status_observed_at,
        Some(live_observed_at)
    );
    assert_eq!(
        restored_task.runtime_projection,
        RuntimeProjection::with_observation_error(
            RuntimeHealth::Healthy,
            runtime_observed_at,
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        )
    );
}

#[test]
fn sqlite_registry_migrates_v5_with_null_attention_acknowledgment() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "ack-migrate-v5"
    ));
    seed_v7_database(&path);
    downgrade_to_v5_without_acknowledgment_columns(&path);
    let store = SqliteRegistryStore::new(&path);

    let restored = store.load().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let columns = table_columns(&connection, "registry_task_workflow");
    std::fs::remove_file(&path).unwrap();

    assert_eq!(version, 9);
    assert!(columns.contains(&"attention_acknowledged_at_unix_seconds".to_string()));
    assert!(columns.contains(&"attention_acknowledged_at_subsec_nanos".to_string()));
    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .attention_acknowledged_at,
        None
    );
}

#[test]
fn sqlite_registry_migrates_v6_live_status_timestamp_from_last_activity() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "live-observed-migrate-v6"
    ));
    seed_v7_database(&path);
    downgrade_to_v6_without_live_observation_columns(&path);
    let store = SqliteRegistryStore::new(&path);

    let restored = store.load().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(version, 9);
    assert_eq!(
        restored
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .live_status_observed_at,
        Some(SystemTime::UNIX_EPOCH + Duration::new(1_700_000_200, 456_000_000))
    );
    assert_eq!(
        restored
            .get_task(&TaskId::new("task-2"))
            .unwrap()
            .live_status_observed_at,
        None
    );
}

#[test]
fn sqlite_registry_rejects_half_present_live_status_timestamp() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "live-observed-half-present"
    ));
    let mut registry = InMemoryRegistry::default();
    let mut seeded = task("task-1", "web", "fix-login");
    seeded.lifecycle_status = LifecycleStatus::Active;
    let observed_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_400, 987_654_321);
    seeded.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    seeded.live_status_observed_at = Some(observed_at);
    registry.create_task(seeded).unwrap();
    {
        let stored = registry.get_task_mut(&TaskId::new("task-1")).unwrap();
        stored.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        stored.live_status_observed_at = Some(observed_at);
    }
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let live_rows: i64 = connection
        .query_row(
            "SELECT count(*) FROM registry_task_live_status WHERE task_id = 'task-1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    connection
        .execute(
            "UPDATE registry_task_live_status \
             SET live_status_observed_at_unix_seconds = 1700000000, \
                 live_status_observed_at_subsec_nanos = NULL \
             WHERE task_id = 'task-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let result = store.load();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(live_rows, 1);
    assert!(
        matches!(result, Err(RegistrySnapshotError::Decode(_))),
        "{result:?}"
    );
}

#[test]
fn sqlite_registry_migration_preserves_existing_v5_task_state() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "ack-migrate-preserve"
    ));
    seed_v7_database(&path);
    downgrade_to_v5_without_acknowledgment_columns(&path);
    let store = SqliteRegistryStore::new(&path);

    let restored = store.load().unwrap();
    std::fs::remove_file(&path).unwrap();
    let task = restored.get_task(&TaskId::new("task-1")).unwrap();

    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
    assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
    assert!(restored
        .events_for_task(&TaskId::new("task-1"))
        .iter()
        .any(|event| event.kind == RegistryEventKind::UserNote));
    assert_eq!(
        restored
            .step_receipts_for_task(&TaskId::new("task-1"))
            .len(),
        1
    );
}

#[test]
fn sqlite_registry_rejects_half_present_acknowledgment_timestamp() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "ack-half-present"
    ));
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task("task-1", "web", "fix-login"))
        .unwrap();
    let store = SqliteRegistryStore::new(&path);
    store.save(&registry).unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE registry_task_workflow \
             SET attention_acknowledged_at_unix_seconds = 1700000000, \
                 attention_acknowledged_at_subsec_nanos = NULL \
             WHERE task_id = 'task-1'",
            [],
        )
        .unwrap();
    drop(connection);

    let result = store.load();
    std::fs::remove_file(&path).unwrap();

    assert!(matches!(result, Err(RegistrySnapshotError::Decode(_))));
}

#[test]
fn sqlite_registry_store_rejects_future_schema_version() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-future-schema"
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("PRAGMA user_version = 999;")
        .unwrap();
    drop(connection);
    let store = SqliteRegistryStore::new(&path);

    let error = store.load().unwrap_err();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(
        error,
        RegistrySnapshotError::IncompatibleSchema {
            found: 999,
            supported: crate::registry::sqlite::migrations::SQLITE_SCHEMA_VERSION
        }
    );
}

#[test]
fn sqlite_registry_store_rejects_legacy_payload_schema_without_migration() {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        "sqlite-legacy-payload-schema"
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE registry_tasks (
                task_id TEXT PRIMARY KEY NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE TABLE registry_events (
                sequence INTEGER PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL,
                payload TEXT NOT NULL
            );
            PRAGMA user_version = 1;
            "#,
        )
        .unwrap();
    drop(connection);
    let store = SqliteRegistryStore::new(&path);

    let error = store.load().unwrap_err();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(error, RegistrySnapshotError::LegacySqlitePayloadSchema);
    assert_eq!(
        error.to_string(),
        "legacy SQLite payload schema is unsupported after the typed state rewrite; remove the state database to start fresh"
    );
}

#[rstest]
#[case::tasks_payload(
    "tasks-payload",
    r#"
    CREATE TABLE registry_tasks (
        task_id TEXT PRIMARY KEY NOT NULL,
        payload TEXT NOT NULL
    );
    CREATE TABLE registry_events (
        sequence INTEGER PRIMARY KEY NOT NULL,
        task_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        message TEXT NOT NULL
    );
    PRAGMA user_version = 1;
    "#
)]
#[case::events_payload(
    "events-payload",
    r#"
    CREATE TABLE registry_tasks (
        task_id TEXT PRIMARY KEY NOT NULL
    );
    CREATE TABLE registry_events (
        sequence INTEGER PRIMARY KEY NOT NULL,
        task_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        message TEXT NOT NULL,
        payload TEXT NOT NULL
    );
    PRAGMA user_version = 1;
    "#
)]
fn sqlite_registry_store_rejects_either_legacy_payload_table(
    #[case] fixture_name: &str,
    #[case] schema: &str,
) {
    let path = std::env::temp_dir().join(format!(
        "ajax-registry-store-{}-{}.db",
        std::process::id(),
        fixture_name
    ));
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection.execute_batch(schema).unwrap();
    drop(connection);
    let store = SqliteRegistryStore::new(&path);

    let error = store.load().unwrap_err();
    std::fs::remove_file(&path).unwrap();

    assert_eq!(error, RegistrySnapshotError::LegacySqlitePayloadSchema);
}
