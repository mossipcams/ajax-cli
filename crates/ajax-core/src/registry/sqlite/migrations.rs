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
        // Already at the current schema: keep this a read-only open. Skipping
        // `create_current_schema` and the `user_version` rewrite avoids a write
        // transaction on every open (which bumped the file change counter).
        return ensure_agent_runs_columns(connection);
    }
    if user_version == 2 {
        migrate_v2_to_v3(connection)?;
    }
    if sqlite_user_version(connection)? == 3 {
        migrate_v3_to_v4(connection)?;
    }
    if sqlite_user_version(connection)? == 4 {
        migrate_v4_to_v5(connection)?;
    }
    if sqlite_user_version(connection)? == 5 {
        migrate_v5_to_v6(connection)?;
    }
    if sqlite_user_version(connection)? == 6 {
        migrate_v6_to_v7(connection)?;
    }
    if sqlite_user_version(connection)? == 7 {
        migrate_v7_to_current_schema(connection)?;
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

/// Version 10 only adds the durable execution tables; existing rows are
/// untouched by `create_current_schema`.
fn migrate_v9_to_v10(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    create_current_schema(connection)?;
    connection
        .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
        .map_err(database_error)
}

/// Repairs `agent_runs` databases created by earlier builds of this unreleased
/// schema 10, which may predate the trailing columns.
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

fn migrate_v6_to_v7(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    for column in [
        "live_status_observed_at_unix_seconds",
        "live_status_observed_at_subsec_nanos",
    ] {
        if !registry_tasks_has_column(connection, column)? {
            connection
                .execute_batch(&format!(
                    "ALTER TABLE registry_tasks ADD COLUMN {column} INTEGER;"
                ))
                .map_err(database_error)?;
        }
    }
    connection
        .execute_batch(
            "UPDATE registry_tasks \
             SET live_status_observed_at_unix_seconds = last_activity_at_unix_seconds, \
                 live_status_observed_at_subsec_nanos = last_activity_at_subsec_nanos \
             WHERE live_status_kind IS NOT NULL \
               AND live_status_observed_at_unix_seconds IS NULL \
               AND live_status_observed_at_subsec_nanos IS NULL;",
        )
        .map_err(database_error)?;
    connection
        .pragma_update(None, "user_version", 7)
        .map_err(database_error)
}

fn migrate_v7_to_current_schema(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    connection
        .execute_batch(
            r#"
            ALTER TABLE registry_tasks RENAME TO registry_tasks_v7;
            "#,
        )
        .map_err(database_error)?;
    create_current_schema(connection)?;
    connection
        .execute_batch(
            r#"
            INSERT INTO registry_tasks (
                task_id, repo, handle, title, branch, base_branch, worktree_path,
                tmux_session, task_window, selected_agent
            )
            SELECT
                task_id, repo, handle, title, branch, base_branch, worktree_path,
                tmux_session, task_window, selected_agent
            FROM registry_tasks_v7;

            INSERT INTO registry_task_workflow (
                task_id, lifecycle_status, agent_status, created_at_unix_seconds,
                created_at_subsec_nanos, last_activity_at_unix_seconds,
                last_activity_at_subsec_nanos, attention_acknowledged_at_unix_seconds,
                attention_acknowledged_at_subsec_nanos
            )
            SELECT
                task_id, lifecycle_status, agent_status, created_at_unix_seconds,
                created_at_subsec_nanos, last_activity_at_unix_seconds,
                last_activity_at_subsec_nanos, attention_acknowledged_at_unix_seconds,
                attention_acknowledged_at_subsec_nanos
            FROM registry_tasks_v7;

            INSERT INTO registry_task_live_status (
                task_id, live_status_kind, live_status_summary,
                live_status_observed_at_unix_seconds, live_status_observed_at_subsec_nanos
            )
            SELECT
                task_id, live_status_kind, live_status_summary,
                live_status_observed_at_unix_seconds, live_status_observed_at_subsec_nanos
            FROM registry_tasks_v7
            WHERE live_status_kind IS NOT NULL;

            INSERT INTO registry_task_runtime_projection (
                task_id, runtime_health, runtime_observed_at_unix_seconds,
                runtime_observed_at_subsec_nanos, runtime_observation_source,
                runtime_observation_error
            )
            SELECT
                task_id, runtime_health, runtime_observed_at_unix_seconds,
                runtime_observed_at_subsec_nanos, runtime_observation_source,
                runtime_observation_error
            FROM registry_tasks_v7;

            INSERT INTO registry_task_git_evidence (
                task_id, git_worktree_exists, git_branch_exists, git_current_branch,
                git_dirty, git_ahead, git_behind, git_merged, git_untracked_files,
                git_unpushed_commits, git_conflicted, git_last_commit
            )
            SELECT
                task_id, git_worktree_exists, git_branch_exists, git_current_branch,
                git_dirty, git_ahead, git_behind, git_merged, git_untracked_files,
                git_unpushed_commits, git_conflicted, git_last_commit
            FROM registry_tasks_v7
            WHERE git_worktree_exists IS NOT NULL;

            INSERT INTO registry_task_tmux_evidence (
                task_id, tmux_exists, tmux_session_name
            )
            SELECT
                task_id, tmux_exists, tmux_session_name
            FROM registry_tasks_v7
            WHERE tmux_exists IS NOT NULL;

            INSERT INTO registry_task_window_evidence (
                task_id, task_window_exists, task_window_name, task_window_current_path,
                task_window_points_at_expected_path
            )
            SELECT
                task_id, task_window_exists, task_window_name, task_window_current_path,
                task_window_points_at_expected_path
            FROM registry_tasks_v7
            WHERE task_window_exists IS NOT NULL;

            DROP TABLE registry_tasks_v7;
            "#,
        )
        .map_err(database_error)?;
    connection
        .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
        .map_err(database_error)
}

fn migrate_v5_to_v6(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    for column in [
        "attention_acknowledged_at_unix_seconds",
        "attention_acknowledged_at_subsec_nanos",
    ] {
        if !registry_tasks_has_column(connection, column)? {
            connection
                .execute_batch(&format!(
                    "ALTER TABLE registry_tasks ADD COLUMN {column} INTEGER;"
                ))
                .map_err(database_error)?;
        }
    }
    connection
        .pragma_update(None, "user_version", 6)
        .map_err(database_error)
}

fn registry_tasks_has_column(
    connection: &Connection,
    column: &str,
) -> Result<bool, RegistrySnapshotError> {
    let mut statement = connection
        .prepare("PRAGMA table_info(registry_tasks)")
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    while let Some(row) = rows.next().map_err(database_error)? {
        let name: String = row.get(1).map_err(database_error)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn migrate_v4_to_v5(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    connection
        .execute_batch(
            r#"
            ALTER TABLE registry_tasks
                ADD COLUMN runtime_observation_error TEXT;

            PRAGMA user_version = 5;
            "#,
        )
        .map_err(database_error)
}

fn migrate_v3_to_v4(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    connection
        .execute_batch(
            r#"
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

            PRAGMA user_version = 4;
            "#,
        )
        .map_err(database_error)
}

fn migrate_v2_to_v3(connection: &Connection) -> Result<(), RegistrySnapshotError> {
    connection
        .execute_batch(
            r#"
            ALTER TABLE registry_tasks
                ADD COLUMN runtime_health TEXT NOT NULL DEFAULT 'unobservable';
            ALTER TABLE registry_tasks
                ADD COLUMN runtime_observed_at_unix_seconds INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE registry_tasks
                ADD COLUMN runtime_observed_at_subsec_nanos INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE registry_tasks
                ADD COLUMN runtime_observation_source TEXT NOT NULL DEFAULT 'unknown';

            UPDATE registry_tasks
            SET
                runtime_health = CASE
                    WHEN git_worktree_exists IS NULL THEN 'unobservable'
                    WHEN git_worktree_exists = 0 THEN 'missing_worktree'
                    WHEN tmux_exists IS NULL THEN 'unobservable'
                    WHEN tmux_exists = 0 THEN 'missing_session'
                    WHEN task_window_exists IS NULL THEN 'unobservable'
                    WHEN task_window_exists = 0 THEN 'missing_task_window'
                    WHEN task_window_points_at_expected_path IS NULL THEN 'unobservable'
                    WHEN task_window_points_at_expected_path = 0 THEN 'wrong_task_window_path'
                    ELSE 'healthy'
                END,
                runtime_observed_at_unix_seconds = last_activity_at_unix_seconds,
                runtime_observed_at_subsec_nanos = last_activity_at_subsec_nanos,
                runtime_observation_source = 'unknown';

            PRAGMA user_version = 3;
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

    /// SQLite header field 7 (bytes 24..28, big-endian), bumped by every write transaction.
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

        // First open migrates the fresh database up to the current schema.
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
