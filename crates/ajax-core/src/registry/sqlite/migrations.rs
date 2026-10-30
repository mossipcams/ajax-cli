use super::database_error;
use crate::registry::RegistrySnapshotError;
use rusqlite::Connection;

pub(super) const SQLITE_SCHEMA_VERSION: i64 = 10;

pub(super) fn migrate(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    let user_version = sqlite_user_version(connection)?;
    if has_legacy_payload_schema(connection)? {
        return Err(RegistrySnapshotError::LegacySqlitePayloadSchema);
    }
    if user_version == SQLITE_SCHEMA_VERSION {
        return ensure_agent_runs_columns(connection);
    }

    if sqlite_user_version(connection)? == 9 {
        migrate_v9_to_v10(connection)?;
    }
    let user_version = sqlite_user_version(connection)?;
    if user_version > 0 && user_version != SQLITE_SCHEMA_VERSION {
        return Err(RegistrySnapshotError::IncompatibleSchema {
            found: user_version,
            supported: SQLITE_SCHEMA_VERSION,
        });
    }
    create_current_schema(connection)?;
    ensure_agent_runs_columns(connection)?;
    connection
        .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
        .map_err(database_error)
}

fn migrate_v9_to_v10(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    create_current_schema(connection)?;
    connection
        .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
        .map_err(database_error)
}

fn ensure_agent_runs_columns(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    for (column, ddl) in [
        (
            "unreadable_history_rows",
            "unreadable_history_rows INTEGER NOT NULL DEFAULT 0",
        ),
        ("child_started", "child_started TEXT"),
    ] {
        if !table_has_column(connection, "agent_runs", column)? {
            connection
                .execute_batch(&format!("ALTER TABLE agent_runs ADD COLUMN {ddl}"))
                .map_err(database_error)?;
        }
    }
    Ok(())
}

fn create_current_schema(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS registry_tasks (
                task_id TEXT PRIMARY KEY NOT NULL,
                repo TEXT NOT NULL,
                handle TEXT NOT NULL,
                title TEXT NOT NULL,
                branch TEXT NOT NULL,
                base_branch TEXT NOT NULL,
                worktree_path TEXT NOT NULL,
                tmux_session TEXT NOT NULL,
                task_window TEXT NOT NULL,
                selected_agent TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS registry_task_workflow (
                task_id TEXT PRIMARY KEY NOT NULL,
                lifecycle_status TEXT NOT NULL,
                agent_status TEXT NOT NULL,
                created_at_unix_seconds INTEGER NOT NULL,
                created_at_subsec_nanos INTEGER NOT NULL,
                last_activity_at_unix_seconds INTEGER NOT NULL,
                last_activity_at_subsec_nanos INTEGER NOT NULL,
                attention_acknowledged_at_unix_seconds INTEGER,
                attention_acknowledged_at_subsec_nanos INTEGER
            );

            CREATE TABLE IF NOT EXISTS registry_task_live_status (
                task_id TEXT PRIMARY KEY NOT NULL,
                live_status_kind TEXT,
                live_status_summary TEXT,
                live_status_observed_at_unix_seconds INTEGER,
                live_status_observed_at_subsec_nanos INTEGER
            );

            CREATE TABLE IF NOT EXISTS registry_task_runtime_projection (
                task_id TEXT PRIMARY KEY NOT NULL,
                runtime_health TEXT NOT NULL,
                runtime_observed_at_unix_seconds INTEGER NOT NULL,
                runtime_observed_at_subsec_nanos INTEGER NOT NULL,
                runtime_observation_source TEXT NOT NULL,
                runtime_observation_error TEXT
            );

            CREATE TABLE IF NOT EXISTS registry_task_git_evidence (
                task_id TEXT PRIMARY KEY NOT NULL,
                git_worktree_exists INTEGER,
                git_branch_exists INTEGER,
                git_current_branch TEXT,
                git_dirty INTEGER,
                git_ahead INTEGER,
                git_behind INTEGER,
                git_merged INTEGER,
                git_untracked_files INTEGER,
                git_unpushed_commits INTEGER,
                git_conflicted INTEGER,
                git_last_commit TEXT
            );

            CREATE TABLE IF NOT EXISTS registry_task_tmux_evidence (
                task_id TEXT PRIMARY KEY NOT NULL,
                tmux_exists INTEGER,
                tmux_session_name TEXT
            );

            CREATE TABLE IF NOT EXISTS registry_task_window_evidence (
                task_id TEXT PRIMARY KEY NOT NULL,
                task_window_exists INTEGER,
                task_window_name TEXT,
                task_window_current_path TEXT,
                task_window_points_at_expected_path INTEGER
            );

            CREATE TABLE IF NOT EXISTS registry_task_side_flags (
                task_id TEXT NOT NULL,
                flag TEXT NOT NULL,
                PRIMARY KEY (task_id, flag)
            );

            CREATE TABLE IF NOT EXISTS registry_task_metadata (
                task_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (task_id, key)
            );

            CREATE TABLE IF NOT EXISTS registry_agent_attempts (
                task_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                agent TEXT NOT NULL,
                launch_target TEXT NOT NULL,
                started_at_unix_seconds INTEGER NOT NULL,
                started_at_subsec_nanos INTEGER NOT NULL,
                finished_at_unix_seconds INTEGER,
                finished_at_subsec_nanos INTEGER,
                status TEXT NOT NULL,
                PRIMARY KEY (task_id, sequence)
            );

            CREATE TABLE IF NOT EXISTS registry_events (
                sequence INTEGER PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL,
                occurred_at_unix_seconds INTEGER NOT NULL,
                occurred_at_subsec_nanos INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS step_receipts (
                task_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                step_key TEXT NOT NULL,
                target TEXT NOT NULL,
                status TEXT NOT NULL,
                receipt_json TEXT NOT NULL,
                created_at_unix_seconds INTEGER NOT NULL,
                created_at_subsec_nanos INTEGER NOT NULL,
                PRIMARY KEY (task_id, operation, step_key, target)
            );

            CREATE TABLE IF NOT EXISTS registry_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value INTEGER NOT NULL
            );
            INSERT OR IGNORE INTO registry_meta (key, value) VALUES ('revision', 0);

            CREATE TABLE IF NOT EXISTS agent_runs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                harness TEXT NOT NULL,
                model TEXT NOT NULL,
                native_session_id TEXT,
                capabilities_json TEXT NOT NULL DEFAULT '{}',
                child_pid INTEGER,
                child_pgid INTEGER,
                owner_boot_id TEXT,
                schema_version INTEGER NOT NULL,
                next_event_seq INTEGER NOT NULL DEFAULT 1,
                created_unix_ms INTEGER NOT NULL,
                unreadable_history_rows INTEGER NOT NULL DEFAULT 0,
                child_started TEXT
            );

            CREATE TABLE IF NOT EXISTS execution_tasks (
                task_exec_id TEXT PRIMARY KEY,
                run_id TEXT NOT NULL,
                parent_id TEXT,
                kind TEXT NOT NULL,
                state TEXT NOT NULL,
                cancel_requested INTEGER NOT NULL DEFAULT 0,
                outcome_json TEXT
            );

            CREATE TABLE IF NOT EXISTS prompt_submissions (
                submission_id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                idempotency_key TEXT NOT NULL,
                task_exec_id TEXT NOT NULL,
                content_json TEXT NOT NULL,
                UNIQUE(run_id, idempotency_key)
            );

            CREATE TABLE IF NOT EXISTS execution_checkpoints (
                task_exec_id TEXT NOT NULL,
                version INTEGER NOT NULL,
                operation TEXT NOT NULL,
                replay_policy TEXT NOT NULL,
                external_operation_id TEXT,
                schema_version INTEGER NOT NULL,
                intent_json TEXT NOT NULL,
                result_json TEXT,
                PRIMARY KEY (task_exec_id, version)
            );

            CREATE TABLE IF NOT EXISTS execution_events (
                run_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                PRIMARY KEY (run_id, seq)
            );

            CREATE TABLE IF NOT EXISTS execution_owner (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                boot_id TEXT NOT NULL,
                pid INTEGER NOT NULL,
                heartbeat_unix_ms INTEGER NOT NULL
            );
            "#,
        )
        .map_err(database_error)
}

fn sqlite_user_version(connection: &Connection) -> Result<i64, RegistrySnapshotError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)
}

fn has_legacy_payload_schema(connection: &Connection) -> Result<bool, RegistrySnapshotError> {
    Ok(table_has_column(connection, "registry_tasks", "payload")?
        || table_has_column(connection, "registry_events", "payload")?)
}

fn table_has_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
) -> Result<bool, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(database_error)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    Ok(columns.iter().any(|existing| existing == column))
}

#[cfg(test)]
mod tests {
    use crate::registry::SqliteRegistryStore;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn file_change_counter(path: &std::path::Path) -> u32 {
        let bytes = std::fs::read(path).expect("read database file");
        u32::from_be_bytes([bytes[24], bytes[25], bytes[26], bytes[27]])
    }

    #[test]
    fn open_at_current_schema_does_not_write() {
        let unique = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "ajax-migrations-read-only-{}-{unique}",
            std::process::id()
        ));

        {
            let store = SqliteRegistryStore::new(path.clone());
            assert_eq!(store.current_revision().expect("initial revision"), 0);
            drop(store);
        }

        let before = file_change_counter(&path);
        {
            let store = SqliteRegistryStore::new(path.clone());
            assert_eq!(store.current_revision().expect("revision"), 0);
            store.load_tasks_only().expect("load tasks");
            drop(store);
        }

        let after = file_change_counter(&path);
        let _ = std::fs::remove_file(&path);
        assert_eq!(
            after, before,
            "opening an up-to-date database must not write"
        );
    }
}
