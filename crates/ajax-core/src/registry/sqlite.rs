use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, Row, Transaction};

use super::{
    InMemoryRegistry, RegistryEvent, RegistryEventKind, RegistrySnapshotError, RegistryStore,
};
use crate::lifecycle::hydrate_lifecycle_status;
use crate::models::{
    AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
    LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
};

const SQLITE_SCHEMA_VERSION: i64 = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteRegistryStore {
    path: PathBuf,
}

impl SqliteRegistryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn open(&self) -> Result<Connection, RegistrySnapshotError> {
        Connection::open(&self.path).map_err(database_error)
    }

    fn migrate(connection: &Connection) -> Result<(), RegistrySnapshotError> {
        let user_version = sqlite_user_version(connection)?;
        if has_legacy_payload_schema(connection)? {
            return Err(RegistrySnapshotError::LegacySqlitePayloadSchema);
        }
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
                "#,
            )
            .map_err(database_error)?;
        connection
            .pragma_update(None, "user_version", SQLITE_SCHEMA_VERSION)
            .map_err(database_error)
    }
}

impl RegistryStore for SqliteRegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        let connection = self.open()?;
        Self::migrate(&connection)?;

        let tasks = load_tasks(&connection)?;
        let events = load_events(&connection)?;

        Ok(InMemoryRegistry {
            tasks: tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events,
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

        for task in registry.tasks.values() {
            save_task(&transaction, task)?;
        }

        for (sequence, event) in registry.events.iter().enumerate() {
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

        transaction.commit().map_err(database_error)
    }
}

fn load_tasks(connection: &Connection) -> Result<Vec<Task>, RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT task_id, repo, handle, title, branch, base_branch, worktree_path, \
             tmux_session, worktrunk_window, selected_agent, lifecycle_status, agent_status, \
             created_at_unix_seconds, created_at_subsec_nanos, last_activity_at_unix_seconds, \
             last_activity_at_subsec_nanos, live_status_kind, live_status_summary, \
             git_worktree_exists, git_branch_exists, git_current_branch, git_dirty, git_ahead, \
             git_behind, git_merged, git_untracked_files, git_unpushed_commits, git_conflicted, \
             git_last_commit, tmux_exists, tmux_session_name, worktrunk_exists, \
             worktrunk_window_name, worktrunk_current_path, worktrunk_points_at_expected_path \
             FROM registry_tasks ORDER BY task_id",
        )
        .map_err(database_error)?;
    let mut rows = statement.query([]).map_err(database_error)?;
    let mut tasks = Vec::new();

    while let Some(row) = rows.next().map_err(database_error)? {
        let mut task = task_from_row(row)?;
        load_task_side_flags(connection, &mut task)?;
        load_task_metadata(connection, &mut task)?;
        load_agent_attempts(connection, &mut task)?;
        tasks.push(task);
    }

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
    let lifecycle_status = parse_lifecycle_status(
        &row.get::<_, String>("lifecycle_status")
            .map_err(database_error)?,
    )?;
    let agent_status = parse_agent_runtime_status(
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
    let live_status = live_status_from_row(row)?;
    let git_status = git_status_from_row(row)?;
    let tmux_status = tmux_status_from_row(row)?;
    let worktrunk_status = worktrunk_status_from_row(row)?;

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
    task.git_status = git_status;
    task.tmux_status = tmux_status;
    task.worktrunk_status = worktrunk_status;

    Ok(task)
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

fn load_task_side_flags(
    connection: &Connection,
    task: &mut Task,
) -> Result<(), RegistrySnapshotError> {
    let mut statement = connection
        .prepare("SELECT flag FROM registry_task_side_flags WHERE task_id = ?1 ORDER BY flag")
        .map_err(database_error)?;
    let flags = statement
        .query_map(params![task.id.as_str()], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    for flag in flags {
        task.add_side_flag(parse_side_flag(&flag)?);
    }

    Ok(())
}

fn load_task_metadata(
    connection: &Connection,
    task: &mut Task,
) -> Result<(), RegistrySnapshotError> {
    let mut statement = connection
        .prepare("SELECT key, value FROM registry_task_metadata WHERE task_id = ?1 ORDER BY key")
        .map_err(database_error)?;
    let entries = statement
        .query_map(params![task.id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;

    task.metadata.extend(entries);

    Ok(())
}

fn load_agent_attempts(
    connection: &Connection,
    task: &mut Task,
) -> Result<(), RegistrySnapshotError> {
    let mut statement = connection
        .prepare(
            "SELECT agent, launch_target, started_at_unix_seconds, finished_at_unix_seconds, \
             started_at_subsec_nanos, finished_at_subsec_nanos, status \
             FROM registry_agent_attempts WHERE task_id = ?1 ORDER BY sequence",
        )
        .map_err(database_error)?;
    let mut rows = statement
        .query(params![task.id.as_str()])
        .map_err(database_error)?;

    while let Some(row) = rows.next().map_err(database_error)? {
        let agent = parse_agent_client(&row.get::<_, String>(0).map_err(database_error)?)?;
        let launch_target = row.get::<_, String>(1).map_err(database_error)?;
        let started_at = unix_parts_to_system_time(
            row.get::<_, i64>(2).map_err(database_error)?,
            row.get::<_, u32>(4).map_err(database_error)?,
        )?;
        let finished_seconds = row.get::<_, Option<i64>>(3).map_err(database_error)?;
        let finished_nanos = row.get::<_, Option<u32>>(5).map_err(database_error)?;
        let finished_at = match (finished_seconds, finished_nanos) {
            (Some(seconds), Some(nanos)) => Some(unix_parts_to_system_time(seconds, nanos)?),
            (None, None) => None,
            _ => {
                return Err(RegistrySnapshotError::Decode(
                    "agent attempt finished timestamp is incomplete".to_string(),
                ))
            }
        };
        let status = parse_agent_runtime_status(&row.get::<_, String>(6).map_err(database_error)?)?;
        task.agent_attempts.push(AgentAttempt {
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

fn save_task(transaction: &Transaction<'_>, task: &Task) -> Result<(), RegistrySnapshotError> {
    let (created_at_seconds, created_at_nanos) = system_time_to_unix_parts(task.created_at)?;
    let (last_activity_seconds, last_activity_nanos) =
        system_time_to_unix_parts(task.last_activity_at)?;
    transaction
        .execute(
            "INSERT INTO registry_tasks \
             (task_id, repo, handle, title, branch, base_branch, worktree_path, tmux_session, \
              worktrunk_window, selected_agent, lifecycle_status, agent_status, \
              created_at_unix_seconds, created_at_subsec_nanos, last_activity_at_unix_seconds, \
              last_activity_at_subsec_nanos, live_status_kind, live_status_summary, \
              git_worktree_exists, git_branch_exists, git_current_branch, git_dirty, git_ahead, \
              git_behind, git_merged, git_untracked_files, git_unpushed_commits, git_conflicted, \
              git_last_commit, tmux_exists, tmux_session_name, worktrunk_exists, \
              worktrunk_window_name, worktrunk_current_path, worktrunk_points_at_expected_path) \
             VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, \
              ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, \
              ?34, ?35)",
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

#[cfg(test)]
mod tests {
    use super::{
        parse_agent_client, parse_agent_runtime_status, parse_lifecycle_status,
        parse_live_status_kind, parse_registry_event_kind, parse_side_flag, SqliteRegistryStore,
    };
    use crate::models::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
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

        assert_eq!(restored_task.lifecycle_status, LifecycleStatus::Waiting);
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

        assert_eq!(version, 2);
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
                supported: 2
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
