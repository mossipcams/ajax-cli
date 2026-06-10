use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, Row, Transaction};

use super::{
    refresh_task_annotations, InMemoryRegistry, RegistryEvent, RegistryEventKind,
    RegistrySnapshotError, RegistryStore,
};
use crate::ghost_task::is_registry_ghost_task;
use crate::lifecycle::hydrate_lifecycle_status;
use crate::models::{
    AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
    LiveStatusKind, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, SideFlag,
    StepReceipt, StepReceiptStatus, Task, TaskId, TaskOperationKind, TmuxStatus, WorktrunkStatus,
};

const SQLITE_SCHEMA_VERSION: i64 = 7;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteRegistryStore {
    path: PathBuf,
}

impl SqliteRegistryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load_tasks_only(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        let connection = self.open()?;
        Self::migrate(&connection)?;

        let mut tasks = load_tasks(&connection)?;
        for task in &mut tasks {
            refresh_task_annotations(task);
        }

        Ok(InMemoryRegistry {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events: Vec::new(),
            step_receipts: BTreeMap::new(),
        })
    }

    fn open(&self) -> Result<Connection, RegistrySnapshotError> {
        Connection::open(&self.path).map_err(database_error)
    }

    fn migrate(connection: &Connection) -> Result<(), RegistrySnapshotError> {
        let user_version = sqlite_user_version(connection)?;
        if has_legacy_payload_schema(connection)? {
            return Err(RegistrySnapshotError::LegacySqlitePayloadSchema);
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
        let user_version = sqlite_user_version(connection)?;
        if user_version > 0 && user_version != SQLITE_SCHEMA_VERSION {
            return Err(RegistrySnapshotError::IncompatibleSchema {
                found: user_version,
                supported: SQLITE_SCHEMA_VERSION,
            });
        }

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
                    worktrunk_window TEXT NOT NULL,
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
                    worktrunk_exists INTEGER,
                    worktrunk_window_name TEXT,
                    worktrunk_current_path TEXT,
                    worktrunk_points_at_expected_path INTEGER,
                    runtime_health TEXT NOT NULL,
                    runtime_observed_at_unix_seconds INTEGER NOT NULL,
                    runtime_observed_at_subsec_nanos INTEGER NOT NULL,
                    runtime_observation_source TEXT NOT NULL,
                    runtime_observation_error TEXT,
                    attention_acknowledged_at_unix_seconds INTEGER,
                    attention_acknowledged_at_subsec_nanos INTEGER
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
                "#,
            )
            .map_err(database_error)?;
        connection
            .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
            .map_err(database_error)
    }

    pub fn current_revision(&self) -> Result<u64, RegistrySnapshotError> {
        let connection = self.open()?;
        Self::migrate(&connection)?;
        revision(&connection)
    }

    pub fn save_if_revision(
        &self,
        registry: &InMemoryRegistry,
        expected_revision: u64,
    ) -> Result<u64, RegistrySnapshotError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RegistrySnapshotError::Io(error.to_string()))?;
        }
        let mut connection = self.open()?;
        Self::migrate(&connection)?;
        let transaction = connection.transaction().map_err(database_error)?;
        let actual = revision(&transaction)?;
        if actual != expected_revision {
            return Err(RegistrySnapshotError::RevisionConflict {
                expected: expected_revision,
                actual,
            });
        }
        save_registry(&transaction, registry)?;
        let next = actual.saturating_add(1);
        transaction
            .execute(
                "UPDATE registry_meta SET value = ?1 WHERE key = 'revision'",
                [next as i64],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)?;
        Ok(next)
    }
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
                    WHEN worktrunk_exists IS NULL THEN 'unobservable'
                    WHEN worktrunk_exists = 0 THEN 'missing_task_window'
                    WHEN worktrunk_points_at_expected_path IS NULL THEN 'unobservable'
                    WHEN worktrunk_points_at_expected_path = 0 THEN 'wrong_task_window_path'
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

impl RegistryStore for SqliteRegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        let connection = self.open()?;
        Self::migrate(&connection)?;

        let mut tasks = load_tasks(&connection)?;
        for task in &mut tasks {
            refresh_task_annotations(task);
        }
        let events = load_events(&connection)?;
        let step_receipts = load_step_receipts(&connection)?;

        Ok(InMemoryRegistry {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events,
            step_receipts: step_receipts
                .into_iter()
                .map(|receipt| (receipt.identity(), receipt))
                .collect(),
        })
    }

    fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| RegistrySnapshotError::Io(error.to_string()))?;
        }

        let mut connection = self.open()?;
        Self::migrate(&connection)?;
        let transaction = connection.transaction().map_err(database_error)?;
        save_registry(&transaction, registry)?;
        transaction
            .execute(
                "UPDATE registry_meta SET value = value + 1 WHERE key = 'revision'",
                [],
            )
            .map_err(database_error)?;
        transaction.commit().map_err(database_error)
    }
}

fn save_registry(
    transaction: &Transaction<'_>,
    registry: &InMemoryRegistry,
) -> Result<(), RegistrySnapshotError> {
    transaction
        .execute("DELETE FROM registry_events", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_agent_attempts", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_metadata", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_task_side_flags", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM registry_tasks", [])
        .map_err(database_error)?;
    transaction
        .execute("DELETE FROM step_receipts", [])
        .map_err(database_error)?;

    let live_task_ids = registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
        .map(|task| task.id.clone())
        .collect::<BTreeSet<_>>();

    for task in registry
        .tasks
        .values()
        .filter(|task| !is_registry_ghost_task(task))
    {
        save_task(transaction, task)?;
    }

    for (sequence, event) in registry
        .events
        .iter()
        .filter(|event| live_task_ids.contains(&event.task_id))
        .enumerate()
    {
        let (occurred_at_seconds, occurred_at_nanos) =
            system_time_to_unix_parts(event.occurred_at)?;
        transaction
            .execute(
                "INSERT INTO registry_events \
                     (sequence, task_id, kind, message, occurred_at_unix_seconds, \
                      occurred_at_subsec_nanos) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    sequence as i64,
                    event.task_id.as_str(),
                    registry_event_kind_name(event.kind),
                    event.message,
                    occurred_at_seconds,
                    occurred_at_nanos,
                ],
            )
            .map_err(database_error)?;
    }

    for receipt in registry
        .step_receipts
        .values()
        .filter(|receipt| live_task_ids.contains(&receipt.task_id))
    {
        save_step_receipt(transaction, receipt)?;
    }

    Ok(())
}

fn revision(connection: &Connection) -> Result<u64, RegistrySnapshotError> {
    connection
        .query_row(
            "SELECT value FROM registry_meta WHERE key = 'revision'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(database_error)
        .and_then(|value| {
            u64::try_from(value).map_err(|error| RegistrySnapshotError::Decode(error.to_string()))
        })
}

fn load_tasks(connection: &Connection) -> Result<Vec<Task>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, repo, handle, title, branch, base_branch, worktree_path, \
             tmux_session, worktrunk_window, selected_agent, lifecycle_status, agent_status, \
             created_at_unix_seconds, created_at_subsec_nanos, last_activity_at_unix_seconds, \
             last_activity_at_subsec_nanos, live_status_kind, live_status_summary, \
             live_status_observed_at_unix_seconds, live_status_observed_at_subsec_nanos, \
             git_worktree_exists, git_branch_exists, git_current_branch, git_dirty, git_ahead, \
             git_behind, git_merged, git_untracked_files, git_unpushed_commits, git_conflicted, \
             git_last_commit, tmux_exists, tmux_session_name, worktrunk_exists, \
             worktrunk_window_name, worktrunk_current_path, worktrunk_points_at_expected_path, \
             runtime_health, runtime_observed_at_unix_seconds, runtime_observed_at_subsec_nanos, \
             runtime_observation_source, runtime_observation_error, \
             attention_acknowledged_at_unix_seconds, attention_acknowledged_at_subsec_nanos \
             FROM registry_tasks WHERE lifecycle_status != 'Removed' ORDER BY task_id",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut tasks = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        tasks.push(task_from_row(row)?);
    }

    load_task_side_flags_by_task(connection, &mut tasks)?;
    tasks.retain(|task| !is_registry_ghost_task(task));
    load_task_metadata_by_task(connection, &mut tasks)?;
    load_agent_attempts_by_task(connection, &mut tasks)?;

    Ok(tasks)
}

fn task_from_row(row: &Row<'_>) -> Result<Task, RegistrySnapshotError> {
    let task_id = TaskId::new(row.get::<_, String>("task_id").map_err(database_error)?);
    let repo = row.get::<_, String>("repo").map_err(database_error)?;
    let handle = row.get::<_, String>("handle").map_err(database_error)?;
    let title = row.get::<_, String>("title").map_err(database_error)?;
    let branch = row.get::<_, String>("branch").map_err(database_error)?;
    let base_branch = row
        .get::<_, String>("base_branch")
        .map_err(database_error)?;
    let worktree_path = row
        .get::<_, String>("worktree_path")
        .map_err(database_error)?;
    let tmux_session = row
        .get::<_, String>("tmux_session")
        .map_err(database_error)?;
    let worktrunk_window = row
        .get::<_, String>("worktrunk_window")
        .map_err(database_error)?;
    let selected_agent = parse_agent_client(
        &row.get::<_, String>("selected_agent")
            .map_err(database_error)?,
    )?;
    let persisted_lifecycle_status = parse_lifecycle_status(
        &row.get::<_, String>("lifecycle_status")
            .map_err(database_error)?,
    )?;
    let mut agent_status = parse_agent_runtime_status(
        &row.get::<_, String>("agent_status")
            .map_err(database_error)?,
    )?;
    let created_at = unix_parts_to_system_time(
        row.get::<_, i64>("created_at_unix_seconds")
            .map_err(database_error)?,
        row.get::<_, u32>("created_at_subsec_nanos")
            .map_err(database_error)?,
    )?;
    let last_activity_at = unix_parts_to_system_time(
        row.get::<_, i64>("last_activity_at_unix_seconds")
            .map_err(database_error)?,
        row.get::<_, u32>("last_activity_at_subsec_nanos")
            .map_err(database_error)?,
    )?;
    let mut live_status = live_status_from_row(row)?;
    let mut live_status_observed_at = live_status_observed_at_from_row(row)?;
    let git_status = git_status_from_row(row)?;
    let tmux_status = tmux_status_from_row(row)?;
    let worktrunk_status = worktrunk_status_from_row(row)?;
    let mut runtime_projection = runtime_projection_from_row(row)?;

    let lifecycle_status = if persisted_lifecycle_status == LifecycleStatus::Waiting {
        if !matches!(
            live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput)
        ) {
            live_status = Some(LiveObservation::new(
                LiveStatusKind::WaitingForInput,
                "waiting for input",
            ));
        }
        LifecycleStatus::Active
    } else {
        persisted_lifecycle_status
    };

    let legacy_agent_unknown = agent_status == AgentRuntimeStatus::Unknown;
    let legacy_live_unknown = matches!(
        live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::Unknown)
    );
    if legacy_agent_unknown {
        agent_status = AgentRuntimeStatus::NotStarted;
    }
    if legacy_live_unknown {
        live_status = None;
        live_status_observed_at = None;
    }
    if (legacy_agent_unknown || legacy_live_unknown)
        && runtime_projection.observation_error.is_none()
    {
        runtime_projection.observation_error = Some("agent status not observed".to_string());
    }

    let mut task = Task::new(
        task_id,
        repo,
        handle,
        title,
        branch,
        base_branch,
        worktree_path,
        tmux_session,
        worktrunk_window,
        selected_agent,
    );
    hydrate_lifecycle_status(&mut task, lifecycle_status);
    task.agent_status = agent_status;
    task.created_at = created_at;
    task.last_activity_at = last_activity_at;
    task.live_status = live_status;
    task.live_status_observed_at = live_status_observed_at;
    task.git_status = git_status;
    task.tmux_status = tmux_status;
    task.worktrunk_status = worktrunk_status;
    task.runtime_projection = runtime_projection;
    task.attention_acknowledged_at = attention_acknowledged_at_from_row(row)?;

    Ok(task)
}

fn live_status_observed_at_from_row(
    row: &Row<'_>,
) -> Result<Option<SystemTime>, RegistrySnapshotError> {
    let seconds = row
        .get::<_, Option<i64>>("live_status_observed_at_unix_seconds")
        .map_err(database_error)?;
    let nanos = row
        .get::<_, Option<u32>>("live_status_observed_at_subsec_nanos")
        .map_err(database_error)?;
    match (seconds, nanos) {
        (Some(seconds), Some(nanos)) => Ok(Some(unix_parts_to_system_time(seconds, nanos)?)),
        (None, None) => Ok(None),
        _ => Err(RegistrySnapshotError::Decode(
            "live status observation timestamp is incomplete".to_string(),
        )),
    }
}

fn attention_acknowledged_at_from_row(
    row: &Row<'_>,
) -> Result<Option<SystemTime>, RegistrySnapshotError> {
    let seconds = row
        .get::<_, Option<i64>>("attention_acknowledged_at_unix_seconds")
        .map_err(database_error)?;
    let nanos = row
        .get::<_, Option<u32>>("attention_acknowledged_at_subsec_nanos")
        .map_err(database_error)?;
    match (seconds, nanos) {
        (Some(seconds), Some(nanos)) => Ok(Some(unix_parts_to_system_time(seconds, nanos)?)),
        (None, None) => Ok(None),
        _ => Err(RegistrySnapshotError::Decode(
            "attention acknowledgment timestamp is incomplete".to_string(),
        )),
    }
}

fn runtime_projection_from_row(row: &Row<'_>) -> Result<RuntimeProjection, RegistrySnapshotError> {
    let health = parse_runtime_health(
        &row.get::<_, String>("runtime_health")
            .map_err(database_error)?,
    )?;
    let observed_at = unix_parts_to_system_time(
        row.get::<_, i64>("runtime_observed_at_unix_seconds")
            .map_err(database_error)?,
        row.get::<_, u32>("runtime_observed_at_subsec_nanos")
            .map_err(database_error)?,
    )?;
    let source = parse_runtime_observation_source(
        &row.get::<_, String>("runtime_observation_source")
            .map_err(database_error)?,
    )?;

    let observation_error = row
        .get::<_, Option<String>>("runtime_observation_error")
        .map_err(database_error)?;

    Ok(match observation_error {
        Some(error) => {
            RuntimeProjection::with_observation_error(health, observed_at, source, error)
        }
        None => RuntimeProjection::new(health, observed_at, source),
    })
}

fn live_status_from_row(row: &Row<'_>) -> Result<Option<LiveObservation>, RegistrySnapshotError> {
    let Some(kind) = row
        .get::<_, Option<String>>("live_status_kind")
        .map_err(database_error)?
    else {
        return Ok(None);
    };
    let summary = row
        .get::<_, Option<String>>("live_status_summary")
        .map_err(database_error)?
        .ok_or_else(|| RegistrySnapshotError::Decode("live status summary missing".to_string()))?;

    Ok(Some(LiveObservation::new(
        parse_live_status_kind(&kind)?,
        summary,
    )))
}

fn git_status_from_row(row: &Row<'_>) -> Result<Option<GitStatus>, RegistrySnapshotError> {
    let Some(worktree_exists) = row
        .get::<_, Option<bool>>("git_worktree_exists")
        .map_err(database_error)?
    else {
        return Ok(None);
    };

    Ok(Some(GitStatus {
        worktree_exists,
        branch_exists: required_optional(
            row.get("git_branch_exists").map_err(database_error)?,
            "git branch",
        )?,
        current_branch: row.get("git_current_branch").map_err(database_error)?,
        dirty: required_optional(row.get("git_dirty").map_err(database_error)?, "git dirty")?,
        ahead: required_optional(row.get("git_ahead").map_err(database_error)?, "git ahead")?,
        behind: required_optional(row.get("git_behind").map_err(database_error)?, "git behind")?,
        merged: required_optional(row.get("git_merged").map_err(database_error)?, "git merged")?,
        untracked_files: required_optional(
            row.get("git_untracked_files").map_err(database_error)?,
            "git untracked files",
        )?,
        unpushed_commits: required_optional(
            row.get("git_unpushed_commits").map_err(database_error)?,
            "git unpushed commits",
        )?,
        conflicted: required_optional(
            row.get("git_conflicted").map_err(database_error)?,
            "git conflicted",
        )?,
        last_commit: row.get("git_last_commit").map_err(database_error)?,
    }))
}

fn tmux_status_from_row(row: &Row<'_>) -> Result<Option<TmuxStatus>, RegistrySnapshotError> {
    let Some(exists) = row
        .get::<_, Option<bool>>("tmux_exists")
        .map_err(database_error)?
    else {
        return Ok(None);
    };
    let session_name = row
        .get::<_, Option<String>>("tmux_session_name")
        .map_err(database_error)?
        .ok_or_else(|| RegistrySnapshotError::Decode("tmux session missing".to_string()))?;

    Ok(Some(TmuxStatus {
        exists,
        session_name,
    }))
}

fn worktrunk_status_from_row(
    row: &Row<'_>,
) -> Result<Option<WorktrunkStatus>, RegistrySnapshotError> {
    let Some(exists) = row
        .get::<_, Option<bool>>("worktrunk_exists")
        .map_err(database_error)?
    else {
        return Ok(None);
    };
    let window_name = row
        .get::<_, Option<String>>("worktrunk_window_name")
        .map_err(database_error)?
        .ok_or_else(|| RegistrySnapshotError::Decode("worktrunk window missing".to_string()))?;
    let current_path = row
        .get::<_, Option<String>>("worktrunk_current_path")
        .map_err(database_error)?
        .ok_or_else(|| RegistrySnapshotError::Decode("worktrunk path missing".to_string()))?;
    let points_at_expected_path = row
        .get::<_, Option<bool>>("worktrunk_points_at_expected_path")
        .map_err(database_error)?
        .ok_or_else(|| RegistrySnapshotError::Decode("worktrunk path flag missing".to_string()))?;

    Ok(Some(WorktrunkStatus {
        exists,
        window_name,
        current_path: PathBuf::from(current_path),
        points_at_expected_path,
    }))
}

fn required_optional<T>(value: Option<T>, label: &'static str) -> Result<T, RegistrySnapshotError> {
    value.ok_or_else(|| RegistrySnapshotError::Decode(format!("{label} missing")))
}

fn task_indexes_by_id(tasks: &[Task]) -> BTreeMap<TaskId, usize> {
    tasks
        .iter()
        .enumerate()
        .map(|(index, task)| (task.id.clone(), index))
        .collect()
}

fn load_task_side_flags_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare("SELECT task_id, flag FROM registry_task_side_flags ORDER BY task_id, flag")
        .map_err(database_error)?;
    let flags = statement
        .query_map([], |row| {
            Ok((
                TaskId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    for (task_id, flag) in flags {
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        tasks[index].add_side_flag(parse_side_flag(&flag)?);
    }

    Ok(())
}

fn load_task_metadata_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare("SELECT task_id, key, value FROM registry_task_metadata ORDER BY task_id, key")
        .map_err(database_error)?;
    let entries = statement
        .query_map([], |row| {
            Ok((
                TaskId::new(row.get::<_, String>(0)?),
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    for (task_id, key, value) in entries {
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        tasks[index].metadata.insert(key, value);
    }

    Ok(())
}

fn load_agent_attempts_by_task(
    connection: &Connection,
    tasks: &mut [Task],
) -> Result<(), RegistrySnapshotError> {
    let task_indexes = task_indexes_by_id(tasks);
    let mut statement = connection
        .prepare(
            "SELECT task_id, agent, launch_target, started_at_unix_seconds, finished_at_unix_seconds, \
             started_at_subsec_nanos, finished_at_subsec_nanos, status \
             FROM registry_agent_attempts ORDER BY task_id, sequence",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;

    while let Some(row) = rows.next().map_err(database_error)? {
        let task_id = TaskId::new(row.get::<_, String>(0).map_err(database_error)?);
        let Some(index) = task_indexes.get(&task_id).copied() else {
            continue;
        };
        let agent = parse_agent_client(&row.get::<_, String>(1).map_err(database_error)?)?;
        let launch_target = row.get::<_, String>(2).map_err(database_error)?;
        let started_at = unix_parts_to_system_time(
            row.get::<_, i64>(3).map_err(database_error)?,
            row.get::<_, u32>(5).map_err(database_error)?,
        )?;
        let finished_seconds = row.get::<_, Option<i64>>(4).map_err(database_error)?;
        let finished_nanos = row.get::<_, Option<u32>>(6).map_err(database_error)?;
        let finished_at = match (finished_seconds, finished_nanos) {
            (Some(seconds), Some(nanos)) => Some(unix_parts_to_system_time(seconds, nanos)?),
            (None, None) => None,
            _ => {
                return Err(RegistrySnapshotError::Decode(
                    "agent attempt finished timestamp is incomplete".to_string(),
                ))
            }
        };
        let status = parse_agent_runtime_status(&row.get::<_, String>(7).map_err(database_error)?)?;
        tasks[index].agent_attempts.push(AgentAttempt {
            agent,
            launch_target,
            started_at,
            finished_at,
            status,
        });
    }

    Ok(())
}

fn load_events(connection: &Connection) -> Result<Vec<RegistryEvent>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, kind, message, occurred_at_unix_seconds, occurred_at_subsec_nanos \
             FROM registry_events ORDER BY sequence",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut events = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        events.push(RegistryEvent {
            task_id: TaskId::new(row.get::<_, String>(0).map_err(database_error)?),
            kind: parse_registry_event_kind(&row.get::<_, String>(1).map_err(database_error)?)?,
            message: row.get::<_, String>(2).map_err(database_error)?,
            occurred_at: unix_parts_to_system_time(
                row.get::<_, i64>(3).map_err(database_error)?,
                row.get::<_, u32>(4).map_err(database_error)?,
            )?,
        });
    }

    Ok(events)
}

fn load_step_receipts(connection: &Connection) -> Result<Vec<StepReceipt>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, operation, step_key, target, status, receipt_json, \
             created_at_unix_seconds, created_at_subsec_nanos \
             FROM step_receipts ORDER BY task_id, operation, step_key, target",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut receipts = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        let operation =
            parse_task_operation_kind(&row.get::<_, String>(1).map_err(database_error)?)?;
        let status = parse_step_receipt_status(&row.get::<_, String>(4).map_err(database_error)?)?;
        receipts.push(StepReceipt {
            task_id: TaskId::new(row.get::<_, String>(0).map_err(database_error)?),
            operation,
            step_key: row.get::<_, String>(2).map_err(database_error)?,
            target: row.get::<_, String>(3).map_err(database_error)?,
            status,
            receipt_json: row.get::<_, String>(5).map_err(database_error)?,
            created_at: unix_parts_to_system_time(
                row.get::<_, i64>(6).map_err(database_error)?,
                row.get::<_, u32>(7).map_err(database_error)?,
            )?,
        });
    }

    Ok(receipts)
}

fn save_task(transaction: &Transaction<'_>, task: &Task) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(task.created_at)?;
    let (last_activity_seconds, last_activity_nanos) =
        system_time_to_unix_parts(task.last_activity_at)?;
    let (runtime_observed_seconds, runtime_observed_nanos) =
        system_time_to_unix_parts(task.runtime_projection.observed_at)?;
    let attention_acknowledged_parts = task
        .attention_acknowledged_at
        .map(system_time_to_unix_parts)
        .transpose()?;
    let live_status_observed_parts = task
        .live_status_observed_at
        .map(system_time_to_unix_parts)
        .transpose()?;
    transaction
        .execute(
            "INSERT INTO registry_tasks \
             (task_id, repo, handle, title, branch, base_branch, worktree_path, tmux_session, \
              worktrunk_window, selected_agent, lifecycle_status, agent_status, \
              created_at_unix_seconds, created_at_subsec_nanos, last_activity_at_unix_seconds, \
              last_activity_at_subsec_nanos, live_status_kind, live_status_summary, \
              live_status_observed_at_unix_seconds, live_status_observed_at_subsec_nanos, \
              git_worktree_exists, git_branch_exists, git_current_branch, git_dirty, git_ahead, \
              git_behind, git_merged, git_untracked_files, git_unpushed_commits, git_conflicted, \
              git_last_commit, tmux_exists, tmux_session_name, worktrunk_exists, \
              worktrunk_window_name, worktrunk_current_path, worktrunk_points_at_expected_path, \
              runtime_health, runtime_observed_at_unix_seconds, runtime_observed_at_subsec_nanos, \
              runtime_observation_source, runtime_observation_error, \
              attention_acknowledged_at_unix_seconds, attention_acknowledged_at_subsec_nanos) \
             VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
              ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, \
              ?34, ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44)",
            params![
                task.id.as_str(),
                task.repo,
                task.handle,
                task.title,
                task.branch,
                task.base_branch,
                task.worktree_path.to_string_lossy().as_ref(),
                task.tmux_session,
                task.worktrunk_window,
                agent_client_name(task.selected_agent),
                lifecycle_status_name(task.lifecycle_status),
                agent_runtime_status_name(task.agent_status),
                created_at_seconds,
                created_at_nanos,
                last_activity_seconds,
                last_activity_nanos,
                task.live_status
                    .as_ref()
                    .map(|status| live_status_kind_name(status.kind)),
                task.live_status
                    .as_ref()
                    .map(|status| status.summary.as_str()),
                live_status_observed_parts.map(|(seconds, _)| seconds),
                live_status_observed_parts.map(|(_, nanos)| nanos),
                task.git_status
                    .as_ref()
                    .map(|status| status.worktree_exists),
                task.git_status.as_ref().map(|status| status.branch_exists),
                task.git_status
                    .as_ref()
                    .and_then(|status| status.current_branch.as_deref()),
                task.git_status.as_ref().map(|status| status.dirty),
                task.git_status.as_ref().map(|status| status.ahead),
                task.git_status.as_ref().map(|status| status.behind),
                task.git_status.as_ref().map(|status| status.merged),
                task.git_status
                    .as_ref()
                    .map(|status| status.untracked_files),
                task.git_status
                    .as_ref()
                    .map(|status| status.unpushed_commits),
                task.git_status.as_ref().map(|status| status.conflicted),
                task.git_status
                    .as_ref()
                    .and_then(|status| status.last_commit.as_deref()),
                task.tmux_status.as_ref().map(|status| status.exists),
                task.tmux_status
                    .as_ref()
                    .map(|status| status.session_name.as_str()),
                task.worktrunk_status.as_ref().map(|status| status.exists),
                task.worktrunk_status
                    .as_ref()
                    .map(|status| status.window_name.as_str()),
                task.worktrunk_status
                    .as_ref()
                    .map(|status| status.current_path.to_string_lossy().to_string()),
                task.worktrunk_status
                    .as_ref()
                    .map(|status| status.points_at_expected_path),
                task.runtime_projection.health.as_str(),
                runtime_observed_seconds,
                runtime_observed_nanos,
                task.runtime_projection.source.as_str(),
                task.runtime_projection.observation_error.as_deref(),
                attention_acknowledged_parts.map(|(seconds, _)| seconds),
                attention_acknowledged_parts.map(|(_, nanos)| nanos),
            ],
        )
        .map_err(database_error)?;

    for flag in task.side_flags() {
        transaction
            .execute(
                "INSERT INTO registry_task_side_flags (task_id, flag) VALUES (?1, ?2)",
                params![task.id.as_str(), side_flag_name(flag)],
            )
            .map_err(database_error)?;
    }

    for (key, value) in &task.metadata {
        transaction
            .execute(
                "INSERT INTO registry_task_metadata (task_id, key, value) VALUES (?1, ?2, ?3)",
                params![task.id.as_str(), key, value],
            )
            .map_err(database_error)?;
    }

    for (sequence, attempt) in task.agent_attempts.iter().enumerate() {
        let (started_at_seconds, started_at_nanos) = system_time_to_unix_parts(attempt.started_at)?;
        let finished_at_parts = attempt
            .finished_at
            .map(system_time_to_unix_parts)
            .transpose()?;
        transaction
            .execute(
                "INSERT INTO registry_agent_attempts \
                 (task_id, sequence, agent, launch_target, started_at_unix_seconds, \
                  started_at_subsec_nanos, finished_at_unix_seconds, \
                  finished_at_subsec_nanos, status) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    task.id.as_str(),
                    sequence as i64,
                    agent_client_name(attempt.agent),
                    attempt.launch_target,
                    started_at_seconds,
                    started_at_nanos,
                    finished_at_parts.map(|(seconds, _)| seconds),
                    finished_at_parts.map(|(_, nanos)| nanos),
                    agent_runtime_status_name(attempt.status),
                ],
            )
            .map_err(database_error)?;
    }

    Ok(())
}

fn save_step_receipt(
    transaction: &Transaction<'_>,
    receipt: &StepReceipt,
) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(receipt.created_at)?;
    transaction
        .execute(
            "INSERT INTO step_receipts \
             (task_id, operation, step_key, target, status, receipt_json, \
              created_at_unix_seconds, created_at_subsec_nanos) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
             ON CONFLICT(task_id, operation, step_key, target) DO UPDATE SET \
                status = excluded.status, \
                receipt_json = excluded.receipt_json, \
                created_at_unix_seconds = excluded.created_at_unix_seconds, \
                created_at_subsec_nanos = excluded.created_at_subsec_nanos",
            params![
                receipt.task_id.as_str(),
                receipt.operation.as_str(),
                receipt.step_key,
                receipt.target,
                receipt.status.as_str(),
                receipt.receipt_json,
                created_at_seconds,
                created_at_nanos,
            ],
        )
        .map_err(database_error)?;

    Ok(())
}

fn database_error(error: rusqlite::Error) -> RegistrySnapshotError {
    RegistrySnapshotError::Database(error.to_string())
}

fn system_time_to_unix_parts(time: SystemTime) -> Result<(i64, u32), RegistrySnapshotError> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))?;
    let seconds = duration.as_secs();
    i64::try_from(seconds)
        .map_err(|error| RegistrySnapshotError::Encode(format!("timestamp out of range: {error}")))
        .map(|seconds| (seconds, duration.subsec_nanos()))
}

fn unix_parts_to_system_time(
    seconds: i64,
    nanos: u32,
) -> Result<SystemTime, RegistrySnapshotError> {
    if nanos >= 1_000_000_000 {
        return Err(RegistrySnapshotError::Decode(format!(
            "timestamp nanoseconds out of range: {nanos}"
        )));
    }
    let seconds = u64::try_from(seconds).map_err(|error| {
        RegistrySnapshotError::Decode(format!("negative timestamp is unsupported: {error}"))
    })?;

    Ok(UNIX_EPOCH + Duration::new(seconds, nanos))
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

fn agent_client_name(value: AgentClient) -> &'static str {
    match value {
        AgentClient::Claude => "Claude",
        AgentClient::Codex => "Codex",
        AgentClient::Other => "Other",
    }
}

fn parse_agent_client(value: &str) -> Result<AgentClient, RegistrySnapshotError> {
    match value {
        "Claude" => Ok(AgentClient::Claude),
        "Codex" => Ok(AgentClient::Codex),
        "Other" => Ok(AgentClient::Other),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown agent client: {value}"
        ))),
    }
}

fn lifecycle_status_name(value: LifecycleStatus) -> &'static str {
    match value {
        LifecycleStatus::Created => "Created",
        LifecycleStatus::Provisioning => "Provisioning",
        LifecycleStatus::Active => "Active",
        LifecycleStatus::Waiting => "Waiting",
        LifecycleStatus::Reviewable => "Reviewable",
        LifecycleStatus::Mergeable => "Mergeable",
        LifecycleStatus::Merged => "Merged",
        LifecycleStatus::Cleanable => "Cleanable",
        LifecycleStatus::Removing => "Removing",
        LifecycleStatus::TeardownIncomplete => "TeardownIncomplete",
        LifecycleStatus::Removed => "Removed",
        LifecycleStatus::Orphaned => "Orphaned",
        LifecycleStatus::Error => "Error",
    }
}

fn parse_lifecycle_status(value: &str) -> Result<LifecycleStatus, RegistrySnapshotError> {
    match value {
        "Created" => Ok(LifecycleStatus::Created),
        "Provisioning" => Ok(LifecycleStatus::Provisioning),
        "Active" => Ok(LifecycleStatus::Active),
        "Waiting" => Ok(LifecycleStatus::Waiting),
        "Reviewable" => Ok(LifecycleStatus::Reviewable),
        "Mergeable" => Ok(LifecycleStatus::Mergeable),
        "Merged" => Ok(LifecycleStatus::Merged),
        "Cleanable" => Ok(LifecycleStatus::Cleanable),
        "Removing" => Ok(LifecycleStatus::Removing),
        "TeardownIncomplete" => Ok(LifecycleStatus::TeardownIncomplete),
        "Removed" => Ok(LifecycleStatus::Removed),
        "Orphaned" => Ok(LifecycleStatus::Orphaned),
        "Error" => Ok(LifecycleStatus::Error),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown lifecycle status: {value}"
        ))),
    }
}

fn agent_runtime_status_name(value: AgentRuntimeStatus) -> &'static str {
    match value {
        AgentRuntimeStatus::NotStarted => "NotStarted",
        AgentRuntimeStatus::Running => "Running",
        AgentRuntimeStatus::Waiting => "Waiting",
        AgentRuntimeStatus::Blocked => "Blocked",
        AgentRuntimeStatus::Dead => "Dead",
        AgentRuntimeStatus::Done => "Done",
        AgentRuntimeStatus::Unknown => "Unknown",
    }
}

fn parse_agent_runtime_status(value: &str) -> Result<AgentRuntimeStatus, RegistrySnapshotError> {
    match value {
        "NotStarted" => Ok(AgentRuntimeStatus::NotStarted),
        "Running" => Ok(AgentRuntimeStatus::Running),
        "Waiting" => Ok(AgentRuntimeStatus::Waiting),
        "Blocked" => Ok(AgentRuntimeStatus::Blocked),
        "Dead" => Ok(AgentRuntimeStatus::Dead),
        "Done" => Ok(AgentRuntimeStatus::Done),
        "Unknown" => Ok(AgentRuntimeStatus::Unknown),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown agent runtime status: {value}"
        ))),
    }
}

fn side_flag_name(value: SideFlag) -> &'static str {
    match value {
        SideFlag::Dirty => "Dirty",
        SideFlag::AgentRunning => "AgentRunning",
        SideFlag::AgentDead => "AgentDead",
        SideFlag::NeedsInput => "NeedsInput",
        SideFlag::TestsFailed => "TestsFailed",
        SideFlag::TmuxMissing => "TmuxMissing",
        SideFlag::WorktreeMissing => "WorktreeMissing",
        SideFlag::WorktrunkMissing => "WorktrunkMissing",
        SideFlag::BranchMissing => "BranchMissing",
        SideFlag::Stale => "Stale",
        SideFlag::Conflicted => "Conflicted",
        SideFlag::Unpushed => "Unpushed",
    }
}

fn parse_side_flag(value: &str) -> Result<SideFlag, RegistrySnapshotError> {
    match value {
        "Dirty" => Ok(SideFlag::Dirty),
        "AgentRunning" => Ok(SideFlag::AgentRunning),
        "AgentDead" => Ok(SideFlag::AgentDead),
        "NeedsInput" => Ok(SideFlag::NeedsInput),
        "TestsFailed" => Ok(SideFlag::TestsFailed),
        "TmuxMissing" => Ok(SideFlag::TmuxMissing),
        "WorktreeMissing" => Ok(SideFlag::WorktreeMissing),
        "WorktrunkMissing" => Ok(SideFlag::WorktrunkMissing),
        "BranchMissing" => Ok(SideFlag::BranchMissing),
        "Stale" => Ok(SideFlag::Stale),
        "Conflicted" => Ok(SideFlag::Conflicted),
        "Unpushed" => Ok(SideFlag::Unpushed),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown side flag: {value}"
        ))),
    }
}

fn live_status_kind_name(value: LiveStatusKind) -> &'static str {
    match value {
        LiveStatusKind::WorktreeMissing => "WorktreeMissing",
        LiveStatusKind::TmuxMissing => "TmuxMissing",
        LiveStatusKind::WorktrunkMissing => "WorktrunkMissing",
        LiveStatusKind::ShellIdle => "ShellIdle",
        LiveStatusKind::CommandRunning => "CommandRunning",
        LiveStatusKind::TestsRunning => "TestsRunning",
        LiveStatusKind::AgentRunning => "AgentRunning",
        LiveStatusKind::WaitingForApproval => "WaitingForApproval",
        LiveStatusKind::WaitingForInput => "WaitingForInput",
        LiveStatusKind::Blocked => "Blocked",
        LiveStatusKind::RateLimited => "RateLimited",
        LiveStatusKind::AuthRequired => "AuthRequired",
        LiveStatusKind::MergeConflict => "MergeConflict",
        LiveStatusKind::CiFailed => "CiFailed",
        LiveStatusKind::ContextLimit => "ContextLimit",
        LiveStatusKind::CommandFailed => "CommandFailed",
        LiveStatusKind::Done => "Done",
        LiveStatusKind::Unknown => "Unknown",
    }
}

fn parse_live_status_kind(value: &str) -> Result<LiveStatusKind, RegistrySnapshotError> {
    match value {
        "WorktreeMissing" => Ok(LiveStatusKind::WorktreeMissing),
        "TmuxMissing" => Ok(LiveStatusKind::TmuxMissing),
        "WorktrunkMissing" => Ok(LiveStatusKind::WorktrunkMissing),
        "ShellIdle" => Ok(LiveStatusKind::ShellIdle),
        "CommandRunning" => Ok(LiveStatusKind::CommandRunning),
        "TestsRunning" => Ok(LiveStatusKind::TestsRunning),
        "AgentRunning" => Ok(LiveStatusKind::AgentRunning),
        "WaitingForApproval" => Ok(LiveStatusKind::WaitingForApproval),
        "WaitingForInput" => Ok(LiveStatusKind::WaitingForInput),
        "Blocked" => Ok(LiveStatusKind::Blocked),
        "RateLimited" => Ok(LiveStatusKind::RateLimited),
        "AuthRequired" => Ok(LiveStatusKind::AuthRequired),
        "MergeConflict" => Ok(LiveStatusKind::MergeConflict),
        "CiFailed" => Ok(LiveStatusKind::CiFailed),
        "ContextLimit" => Ok(LiveStatusKind::ContextLimit),
        "CommandFailed" => Ok(LiveStatusKind::CommandFailed),
        "Done" => Ok(LiveStatusKind::Done),
        "Unknown" => Ok(LiveStatusKind::Unknown),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown live status kind: {value}"
        ))),
    }
}

fn parse_runtime_health(value: &str) -> Result<RuntimeHealth, RegistrySnapshotError> {
    RuntimeHealth::from_label(value)
        .ok_or_else(|| RegistrySnapshotError::Decode(format!("unknown runtime health: {value}")))
}

fn parse_runtime_observation_source(
    value: &str,
) -> Result<RuntimeObservationSource, RegistrySnapshotError> {
    RuntimeObservationSource::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown runtime observation source: {value}"))
    })
}

fn registry_event_kind_name(value: RegistryEventKind) -> &'static str {
    match value {
        RegistryEventKind::TaskCreated => "TaskCreated",
        RegistryEventKind::LifecycleChanged => "LifecycleChanged",
        RegistryEventKind::SubstrateChanged => "SubstrateChanged",
        RegistryEventKind::UserNote => "UserNote",
    }
}

fn parse_registry_event_kind(value: &str) -> Result<RegistryEventKind, RegistrySnapshotError> {
    match value {
        "TaskCreated" => Ok(RegistryEventKind::TaskCreated),
        "LifecycleChanged" => Ok(RegistryEventKind::LifecycleChanged),
        "SubstrateChanged" => Ok(RegistryEventKind::SubstrateChanged),
        "UserNote" => Ok(RegistryEventKind::UserNote),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown registry event kind: {value}"
        ))),
    }
}

fn parse_task_operation_kind(value: &str) -> Result<TaskOperationKind, RegistrySnapshotError> {
    TaskOperationKind::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown task operation kind: {value}"))
    })
}

fn parse_step_receipt_status(value: &str) -> Result<StepReceiptStatus, RegistrySnapshotError> {
    StepReceiptStatus::from_label(value).ok_or_else(|| {
        RegistrySnapshotError::Decode(format!("unknown step receipt status: {value}"))
    })
}

#[cfg(test)]
mod tests {
    use super::{
        parse_agent_client, parse_agent_runtime_status, parse_lifecycle_status,
        parse_live_status_kind, parse_registry_event_kind, parse_side_flag, SqliteRegistryStore,
    };
    use crate::models::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, RuntimeHealth, RuntimeObservationSource, RuntimeProjection, SideFlag,
        StepReceipt, Task, TaskId, TaskOperationKind, TmuxStatus, WorktrunkStatus,
    };
    use crate::registry::{
        InMemoryRegistry, Registry, RegistryEvent, RegistryEventKind, RegistrySnapshotError,
        RegistryStore,
    };
    use rstest::rstest;
    use std::time::{Duration, SystemTime};

    fn task(id: &str, repo: &str, handle: &str) -> Task {
        Task::new(
            TaskId::new(id),
            repo,
            handle,
            "Fix login",
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/{repo}-{handle}"),
            format!("ajax-{repo}-{handle}"),
            "worktrunk",
            AgentClient::Codex,
        )
    }

    #[rstest]
    #[case("Claude", AgentClient::Claude)]
    #[case("Codex", AgentClient::Codex)]
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
    #[case("WorktrunkMissing", SideFlag::WorktrunkMissing)]
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
    #[case("WorktrunkMissing", LiveStatusKind::WorktrunkMissing)]
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
    fn sqlite_registry_store_batches_task_detail_loads() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/registry/sqlite.rs"),
        )
        .unwrap();
        let production_source = source.split("#[cfg(test)]").next().unwrap();

        assert!(production_source.contains("fn load_task_side_flags_by_task"));
        assert!(production_source.contains("fn load_task_metadata_by_task"));
        assert!(production_source.contains("fn load_agent_attempts_by_task"));
        assert!(!production_source.contains("WHERE task_id = ?1 ORDER BY flag"));
        assert!(!production_source.contains("WHERE task_id = ?1 ORDER BY key"));
        assert!(!production_source.contains("WHERE task_id = ?1 ORDER BY sequence"));
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
        task.worktrunk_status = Some(WorktrunkStatus::present("worktrunk", "/tmp/web"));
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
            restored_task.worktrunk_status,
            registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .worktrunk_status
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
            RegistrySnapshotError::Decode(
                "agent attempt finished timestamp is incomplete".to_string()
            )
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

        assert_eq!(version, 7);
    }

    #[test]
    fn sqlite_registry_store_migrates_v4_probe_error_column() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-v4-probe-error.db",
            std::process::id()
        ));
        let store = SqliteRegistryStore::new(&path);
        store.save(&InMemoryRegistry::default()).unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute_batch(
                r#"
                ALTER TABLE registry_tasks DROP COLUMN runtime_observation_error;
                PRAGMA user_version = 4;
                "#,
            )
            .unwrap();
        drop(connection);

        store.load().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let columns = table_columns(&connection, "registry_tasks");
        std::fs::remove_file(&path).unwrap();

        assert_eq!(version, 7);
        assert!(columns.contains(&"runtime_observation_error".to_string()));
    }

    #[test]
    fn sqlite_registry_store_does_not_persist_removed_task_tombstones() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-{}.db",
            std::process::id(),
            "sqlite-purges-removed"
        ));
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .create_task(task("task-2", "web", "old-task"))
            .unwrap();
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
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .create_task(task("task-2", "web", "old-task"))
            .unwrap();
        registry
            .get_task_mut(&TaskId::new("task-2"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Removed;
        let store = SqliteRegistryStore::new(&path);
        store.save(&registry).unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE registry_tasks SET lifecycle_status = 'Removed' WHERE task_id = 'task-1'",
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
    fn sqlite_registry_store_migrates_v2_tasks_to_runtime_projection_columns() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-{}.db",
            std::process::id(),
            "sqlite-v2-runtime-migration"
        ));
        let connection = rusqlite::Connection::open(&path).unwrap();
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
                    worktrunk_window TEXT NOT NULL,
                    selected_agent TEXT NOT NULL,
                    lifecycle_status TEXT NOT NULL,
                    agent_status TEXT NOT NULL,
                    created_at_unix_seconds INTEGER NOT NULL,
                    created_at_subsec_nanos INTEGER NOT NULL,
                    last_activity_at_unix_seconds INTEGER NOT NULL,
                    last_activity_at_subsec_nanos INTEGER NOT NULL,
                    live_status_kind TEXT,
                    live_status_summary TEXT,
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
                    worktrunk_exists INTEGER,
                    worktrunk_window_name TEXT,
                    worktrunk_current_path TEXT,
                    worktrunk_points_at_expected_path INTEGER
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
                INSERT INTO registry_tasks (
                    task_id, repo, handle, title, branch, base_branch, worktree_path, tmux_session,
                    worktrunk_window, selected_agent, lifecycle_status, agent_status,
                    created_at_unix_seconds, created_at_subsec_nanos,
                    last_activity_at_unix_seconds, last_activity_at_subsec_nanos,
                    live_status_kind, live_status_summary, git_worktree_exists,
                    git_branch_exists, git_current_branch, git_dirty, git_ahead, git_behind,
                    git_merged, git_untracked_files, git_unpushed_commits, git_conflicted,
                    git_last_commit, tmux_exists, tmux_session_name, worktrunk_exists,
                    worktrunk_window_name, worktrunk_current_path, worktrunk_points_at_expected_path
                ) VALUES (
                    'task-1', 'web', 'fix-login', 'Fix login', 'ajax/fix-login', 'main',
                    '/tmp/worktrees/web-fix-login', 'ajax-web-fix-login', 'worktrunk',
                    'Codex', 'Active', 'Running', 1700000000, 0, 1700000001, 0,
                    NULL, NULL, 1, 1, 'ajax/fix-login', 0, 0, 0, 0, 0, 0, 0,
                    'abc123', 1, 'ajax-web-fix-login', 1, 'worktrunk',
                    '/tmp/worktrees/web-fix-login', 1
                );
                PRAGMA user_version = 2;
                "#,
            )
            .unwrap();
        drop(connection);
        let store = SqliteRegistryStore::new(&path);

        let restored = store.load().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let columns = table_columns(&connection, "registry_tasks");
        std::fs::remove_file(&path).unwrap();
        let task = restored.get_task(&TaskId::new("task-1")).unwrap();

        assert_eq!(version, 7);
        assert!(columns.contains(&"runtime_health".to_string()));
        assert!(columns.contains(&"runtime_observation_error".to_string()));
        assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
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
            "worktrunk_window",
            "selected_agent",
            "lifecycle_status",
            "agent_status",
            "created_at_unix_seconds",
            "last_activity_at_unix_seconds",
            "live_status_observed_at_unix_seconds",
            "live_status_observed_at_subsec_nanos",
            "attention_acknowledged_at_unix_seconds",
            "attention_acknowledged_at_subsec_nanos",
        ] {
            assert!(task_columns.contains(&required.to_string()), "{required}");
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

    fn downgrade_to_v5_without_acknowledgment_columns(path: &std::path::Path) {
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

    fn downgrade_to_v6_without_live_observation_columns(path: &std::path::Path) {
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

    #[test]
    fn sqlite_registry_migrates_v5_with_null_attention_acknowledgment() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-{}.db",
            std::process::id(),
            "ack-migrate-v5"
        ));
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let store = SqliteRegistryStore::new(&path);
        store.save(&registry).unwrap();
        downgrade_to_v5_without_acknowledgment_columns(&path);

        let restored = store.load().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        let columns = table_columns(&connection, "registry_tasks");
        std::fs::remove_file(&path).unwrap();

        assert_eq!(version, 7);
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
        let mut registry = InMemoryRegistry::default();
        let mut with_live = task("task-1", "web", "with-live");
        let observed_at = SystemTime::UNIX_EPOCH + Duration::new(1_700_000_700, 444);
        with_live.last_activity_at = observed_at;
        with_live.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting",
        ));
        with_live.live_status_observed_at = Some(observed_at);
        registry.create_task(with_live).unwrap();
        registry
            .create_task(task("task-2", "web", "without-live"))
            .unwrap();
        let store = SqliteRegistryStore::new(&path);
        store.save(&registry).unwrap();
        downgrade_to_v6_without_live_observation_columns(&path);

        let restored = store.load().unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(version, 7);
        assert_eq!(
            restored
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .live_status_observed_at,
            Some(observed_at)
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
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let store = SqliteRegistryStore::new(&path);
        store.save(&registry).unwrap();
        let connection = rusqlite::Connection::open(&path).unwrap();
        connection
            .execute(
                "UPDATE registry_tasks \
                 SET live_status_observed_at_unix_seconds = 1700000000, \
                     live_status_observed_at_subsec_nanos = NULL \
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
    fn sqlite_registry_migration_preserves_existing_v5_task_state() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-{}.db",
            std::process::id(),
            "ack-migrate-preserve"
        ));
        let mut registry = InMemoryRegistry::default();
        let mut seeded = task("task-1", "web", "fix-login");
        seeded.lifecycle_status = LifecycleStatus::Active;
        seeded.add_side_flag(SideFlag::NeedsInput);
        seeded.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        registry.create_task(seeded).unwrap();
        let expected_runtime = registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .runtime_projection
            .clone();
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
        let store = SqliteRegistryStore::new(&path);
        store.save(&registry).unwrap();
        downgrade_to_v5_without_acknowledgment_columns(&path);

        let restored = store.load().unwrap();
        std::fs::remove_file(&path).unwrap();
        let task = restored.get_task(&TaskId::new("task-1")).unwrap();

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
        assert_eq!(task.runtime_projection, expected_runtime);
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
                "UPDATE registry_tasks \
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
                supported: 7
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

    #[test]
    fn sqlite_store_rejects_stale_expected_revision_without_overwriting_newer_state() {
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-stale-revision.db",
            std::process::id()
        ));
        let store = SqliteRegistryStore::new(&path);
        let mut first = InMemoryRegistry::default();
        first
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        store.save(&first).unwrap();
        let revision = store.current_revision().unwrap();

        let mut newer = first.clone();
        newer.get_task_mut(&TaskId::new("task-1")).unwrap().title = "newer".to_string();
        store.save_if_revision(&newer, revision).unwrap();

        let error = store.save_if_revision(&first, revision).unwrap_err();
        assert_eq!(
            error,
            RegistrySnapshotError::RevisionConflict {
                expected: revision,
                actual: revision + 1,
            }
        );
        assert_eq!(
            store
                .load()
                .unwrap()
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .title,
            "newer"
        );
        std::fs::remove_file(path).unwrap();
    }

    fn table_columns(connection: &rusqlite::Connection, table: &str) -> Vec<String> {
        let mut statement = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }
}
