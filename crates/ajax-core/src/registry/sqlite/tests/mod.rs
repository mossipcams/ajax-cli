#![allow(unused_imports)]
pub(super) use super::{
    parse_agent_client, parse_agent_runtime_status, parse_lifecycle_status, parse_live_status_kind,
    parse_registry_event_kind, parse_side_flag, SqliteRegistryStore,
};
pub(super) use crate::models::{
    AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
    LiveStatusKind, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, SideFlag,
    StepReceipt, Task, TaskId, TaskOperationKind, TaskWindowStatus, TmuxStatus,
};
pub(super) use crate::registry::{
    InMemoryRegistry, Registry, RegistryEvent, RegistryEventKind, RegistrySnapshotError,
};
pub(super) use rstest::rstest;
pub(super) use std::time::{Duration, SystemTime};

pub(super) fn task(id: &str, repo: &str, handle: &str) -> Task {
    Task::new(
        TaskId::new(id),
        repo,
        handle,
        "Fix login",
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/{repo}-{handle}"),
        format!("ajax-{repo}-{handle}"),
        "task",
        AgentClient::Codex,
    )
}

mod suite_1;
mod suite_2;
mod suite_3;
mod suite_4;

pub(super) fn table_columns(connection: &rusqlite::Connection, table: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    statement
        .query_map([], |row| row.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

pub(super) fn v7_attention_acknowledged_at() -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::new(1_700_000_400, 222_333_444)
}

pub(super) fn seed_v7_database(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE registry_tasks (
                task_id TEXT PRIMARY KEY NOT NULL,
                repo TEXT NOT NULL,
                handle TEXT NOT NULL,
                title TEXT NOT NULL,
                branch TEXT NOT NULL,
                base_branch TEXT NOT NULL,
                worktree_path TEXT NOT NULL,
                tmux_session TEXT NOT NULL,
                task_window TEXT NOT NULL,
                selected_agent TEXT NOT NULL,
                lifecycle_status TEXT NOT NULL,
                agent_status TEXT NOT NULL,
                created_at_unix_seconds INTEGER NOT NULL,
                created_at_subsec_nanos INTEGER NOT NULL,
                last_activity_at_unix_seconds INTEGER NOT NULL,
                last_activity_at_subsec_nanos INTEGER NOT NULL,
                live_status_kind TEXT,
                live_status_summary TEXT,
                live_status_observed_at_unix_seconds INTEGER,
                live_status_observed_at_subsec_nanos INTEGER,
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
                git_last_commit TEXT,
                tmux_exists INTEGER,
                tmux_session_name TEXT,
                task_window_exists INTEGER,
                task_window_name TEXT,
                task_window_current_path TEXT,
                task_window_points_at_expected_path INTEGER,
                runtime_health TEXT NOT NULL,
                runtime_observed_at_unix_seconds INTEGER NOT NULL,
                runtime_observed_at_subsec_nanos INTEGER NOT NULL,
                runtime_observation_source TEXT NOT NULL,
                runtime_observation_error TEXT,
                attention_acknowledged_at_unix_seconds INTEGER,
                attention_acknowledged_at_subsec_nanos INTEGER
            );
            CREATE TABLE registry_task_side_flags (
                task_id TEXT NOT NULL,
                flag TEXT NOT NULL,
                PRIMARY KEY (task_id, flag)
            );
            CREATE TABLE registry_task_metadata (
                task_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (task_id, key)
            );
            CREATE TABLE registry_agent_attempts (
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
            CREATE TABLE registry_events (
                sequence INTEGER PRIMARY KEY NOT NULL,
                task_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                message TEXT NOT NULL,
                occurred_at_unix_seconds INTEGER NOT NULL,
                occurred_at_subsec_nanos INTEGER NOT NULL
            );
            CREATE TABLE step_receipts (
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
            CREATE TABLE registry_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value INTEGER NOT NULL
            );
            INSERT INTO registry_meta (key, value) VALUES ('revision', 12);
            INSERT INTO registry_tasks VALUES (
                'task-1',
                'web',
                'fix-login',
                'Fix login',
                'ajax/fix-login',
                'main',
                '/tmp/worktrees/web-fix-login',
                'ajax-web-fix-login',
                'task',
                'Codex',
                'Active',
                'Blocked',
                1700000100,
                123000000,
                1700000200,
                456000000,
                'WaitingForInput',
                'waiting for input',
                1700000200,
                456000000,
                1,
                1,
                'ajax/fix-login',
                1,
                2,
                1,
                0,
                3,
                4,
                1,
                'abc123 Fix login',
                1,
                'ajax-web-fix-login',
                1,
                'task',
                '/tmp/worktrees/web-fix-login',
                1,
                'healthy',
                1700000300,
                654000000,
                'command_result',
                NULL,
                1700000400,
                222333444
            );
            INSERT INTO registry_task_side_flags VALUES ('task-1', 'NeedsInput');
            INSERT INTO registry_task_metadata VALUES ('task-1', 'review', 'requested');
            INSERT INTO registry_agent_attempts VALUES (
                'task-1',
                0,
                'Claude',
                'tmux:%1',
                1700000110,
                789000000,
                1700000120,
                987000000,
                'Dead'
            );
            INSERT INTO registry_events VALUES (
                0,
                'task-1',
                'UserNote',
                'ready',
                1700000130,
                111000000
            );
            INSERT INTO step_receipts VALUES (
                'task-1',
                'drop',
                'tmux_session_absent',
                'ajax-web-fix-login',
                'succeeded',
                '{"program":"tmux"}',
                1700000140,
                222000000
            );
            INSERT INTO registry_tasks VALUES (
                'task-2',
                'web',
                'without-live',
                'Fix login follow-up',
                'ajax/without-live',
                'main',
                '/tmp/worktrees/web-without-live',
                'ajax-web-without-live',
                'task',
                'Codex',
                'Active',
                'Running',
                1700000500,
                0,
                1700000600,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                1,
                1,
                'ajax/without-live',
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                NULL,
                1,
                'ajax-web-without-live',
                1,
                'task',
                '/tmp/worktrees/web-without-live',
                1,
                'healthy',
                1700000600,
                0,
                'command_result',
                NULL,
                NULL,
                NULL
            );
            PRAGMA user_version = 7;
            "#,
        )
        .unwrap();
}

pub(super) fn downgrade_to_v5_without_acknowledgment_columns(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            r#"
            ALTER TABLE registry_tasks DROP COLUMN attention_acknowledged_at_unix_seconds;
            ALTER TABLE registry_tasks DROP COLUMN attention_acknowledged_at_subsec_nanos;
            PRAGMA user_version = 5;
            "#,
        )
        .unwrap();
}

pub(super) fn downgrade_to_v6_without_live_observation_columns(path: &std::path::Path) {
    let connection = rusqlite::Connection::open(path).unwrap();
    connection
        .execute_batch(
            r#"
            ALTER TABLE registry_tasks DROP COLUMN live_status_observed_at_unix_seconds;
            ALTER TABLE registry_tasks DROP COLUMN live_status_observed_at_subsec_nanos;
            PRAGMA user_version = 6;
            "#,
        )
        .unwrap();
}
