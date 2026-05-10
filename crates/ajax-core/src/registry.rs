use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::models::{
    AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
    LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
};
use rusqlite::{params, Connection, Row, Transaction};
use serde::{Deserialize, Serialize};

const SQLITE_SCHEMA_VERSION: i64 = 2;

pub trait Registry {
    fn create_task(&mut self, task: Task) -> Result<(), RegistryError>;
    fn get_task(&self, task_id: &TaskId) -> Option<&Task>;
    fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task>;
    fn list_tasks(&self) -> Vec<&Task>;
    fn update_lifecycle(
        &mut self,
        task_id: &TaskId,
        status: LifecycleStatus,
    ) -> Result<(), RegistryError>;
    fn record_event(
        &mut self,
        task_id: TaskId,
        kind: RegistryEventKind,
        message: impl Into<String>,
    ) -> Result<(), RegistryError>;
    fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryRegistry {
    tasks: BTreeMap<TaskId, Task>,
    events: Vec<RegistryEvent>,
}

impl Registry for InMemoryRegistry {
    fn create_task(&mut self, task: Task) -> Result<(), RegistryError> {
        let task_id = task.id.clone();

        if self.tasks.contains_key(&task_id) {
            return Err(RegistryError::DuplicateTask(task_id));
        }

        self.tasks.insert(task_id.clone(), task);
        self.events.push(RegistryEvent::new(
            task_id,
            RegistryEventKind::TaskCreated,
            "task created",
        ));

        Ok(())
    }

    fn get_task(&self, task_id: &TaskId) -> Option<&Task> {
        self.tasks.get(task_id)
    }

    fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task> {
        self.tasks.get_mut(task_id)
    }

    fn list_tasks(&self) -> Vec<&Task> {
        self.tasks.values().collect()
    }

    fn update_lifecycle(
        &mut self,
        task_id: &TaskId,
        status: LifecycleStatus,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        task.lifecycle_status = status;
        task.last_activity_at = SystemTime::now();
        task.remove_side_flag(SideFlag::Stale);
        self.events.push(RegistryEvent::new(
            task_id.clone(),
            RegistryEventKind::LifecycleChanged,
            format!("lifecycle changed to {status:?}"),
        ));

        Ok(())
    }

    fn record_event(
        &mut self,
        task_id: TaskId,
        kind: RegistryEventKind,
        message: impl Into<String>,
    ) -> Result<(), RegistryError> {
        if !self.tasks.contains_key(&task_id) {
            return Err(RegistryError::TaskNotFound(task_id));
        }

        self.events
            .push(RegistryEvent::new(task_id, kind, message.into()));

        Ok(())
    }

    fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent> {
        self.events
            .iter()
            .filter(|event| &event.task_id == task_id)
            .collect()
    }
}

impl InMemoryRegistry {
    pub fn export_json_snapshot(&self) -> Result<String, RegistrySnapshotError> {
        let snapshot = RegistrySnapshot {
            tasks: self.tasks.values().cloned().collect(),
            events: self.events.clone(),
        };

        serde_json::to_string_pretty(&snapshot)
            .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))
    }

    pub fn export_json_snapshot_file(&self, path: &Path) -> Result<(), RegistrySnapshotError> {
        let json = self.export_json_snapshot()?;
        std::fs::write(path, json).map_err(|error| RegistrySnapshotError::Io(error.to_string()))
    }
}

pub trait RegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError>;
    fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError>;
}

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
    task.lifecycle_status = lifecycle_status;
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
        RegistryEventKind::UserNote => "UserNote",
        RegistryEventKind::Reconciled => "Reconciled",
    }
}

fn parse_registry_event_kind(value: &str) -> Result<RegistryEventKind, RegistrySnapshotError> {
    match value {
        "TaskCreated" => Ok(RegistryEventKind::TaskCreated),
        "LifecycleChanged" => Ok(RegistryEventKind::LifecycleChanged),
        "UserNote" => Ok(RegistryEventKind::UserNote),
        "Reconciled" => Ok(RegistryEventKind::Reconciled),
        _ => Err(RegistrySnapshotError::Decode(format!(
            "unknown registry event kind: {value}"
        ))),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateTask(TaskId),
    TaskNotFound(TaskId),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTask(task_id) => {
                write!(formatter, "duplicate task: {}", task_id.as_str())
            }
            Self::TaskNotFound(task_id) => {
                write!(formatter, "task not found: {}", task_id.as_str())
            }
        }
    }
}

impl Error for RegistryError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrySnapshotError {
    Encode(String),
    Decode(String),
    Database(String),
    Io(String),
    IncompatibleSchema { found: i64, supported: i64 },
    LegacySqlitePayloadSchema,
}

impl fmt::Display for RegistrySnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(message) => write!(formatter, "state encode failed: {message}"),
            Self::Decode(message) => write!(formatter, "state decode failed: {message}"),
            Self::Database(message) => write!(formatter, "database error: {message}"),
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::IncompatibleSchema { found, supported } => write!(
                formatter,
                "incompatible state schema: found {found}, supported {supported}"
            ),
            Self::LegacySqlitePayloadSchema => write!(
                formatter,
                "legacy SQLite payload schema is unsupported after the typed state rewrite; remove the state database to start fresh"
            ),
        }
    }
}

impl Error for RegistrySnapshotError {}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
struct RegistrySnapshot {
    tasks: Vec<Task>,
    events: Vec<RegistryEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct RegistryEvent {
    pub task_id: TaskId,
    pub kind: RegistryEventKind,
    pub message: String,
    pub occurred_at: SystemTime,
}

impl RegistryEvent {
    pub fn new(task_id: TaskId, kind: RegistryEventKind, message: impl Into<String>) -> Self {
        Self {
            task_id,
            kind,
            message: message.into(),
            occurred_at: SystemTime::now(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RegistryEventKind {
    TaskCreated,
    LifecycleChanged,
    UserNote,
    Reconciled,
}

#[cfg(test)]
mod tests {
    use super::{
        InMemoryRegistry, Registry, RegistryError, RegistryEvent, RegistryEventKind,
        RegistrySnapshotError, RegistryStore, SqliteRegistryStore,
    };
    use crate::models::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
    };
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

    #[test]
    fn creates_and_lists_tasks_in_stable_order() {
        let mut registry = InMemoryRegistry::default();

        registry
            .create_task(task("task-2", "web", "b-task"))
            .unwrap();
        registry
            .create_task(task("task-1", "web", "a-task"))
            .unwrap();

        let tasks = registry.list_tasks();

        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id.as_str(), "task-1");
        assert_eq!(tasks[1].id.as_str(), "task-2");
    }

    #[test]
    fn rejects_duplicate_task_ids() {
        let mut registry = InMemoryRegistry::default();

        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let error = registry
            .create_task(task("task-1", "web", "fix-login-again"))
            .unwrap_err();

        assert_eq!(error, RegistryError::DuplicateTask(TaskId::new("task-1")));
    }

    #[test]
    fn registry_errors_have_operator_facing_display() {
        assert_eq!(
            RegistryError::DuplicateTask(TaskId::new("task-1")).to_string(),
            "duplicate task: task-1"
        );
        assert_eq!(
            RegistryError::TaskNotFound(TaskId::new("missing")).to_string(),
            "task not found: missing"
        );
    }

    #[test]
    fn registry_snapshot_errors_have_operator_facing_display() {
        assert_eq!(
            RegistrySnapshotError::Database("file is not a database".to_string()).to_string(),
            "database error: file is not a database"
        );
        assert_eq!(
            RegistrySnapshotError::IncompatibleSchema {
                found: 4,
                supported: 2,
            }
            .to_string(),
            "incompatible state schema: found 4, supported 2"
        );
    }

    #[test]
    fn updates_task_lifecycle() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();

        registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Reviewable)
            .unwrap();

        let updated = registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(updated.lifecycle_status, LifecycleStatus::Reviewable);
    }

    #[test]
    fn lifecycle_updates_clear_stale_attention() {
        let mut registry = InMemoryRegistry::default();
        let mut task = task("task-1", "web", "fix-login");
        task.add_side_flag(SideFlag::Stale);
        registry.create_task(task).unwrap();

        registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Active)
            .unwrap();

        let updated = registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!updated.has_side_flag(SideFlag::Stale));
    }

    #[test]
    fn records_event_history_for_task_changes() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(
                TaskId::new("task-1"),
                RegistryEventKind::UserNote,
                "ready for review",
            )
            .unwrap();

        let events = registry.events_for_task(&TaskId::new("task-1"));

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, RegistryEventKind::TaskCreated);
        assert_eq!(events[1].kind, RegistryEventKind::UserNote);
        assert_eq!(events[1].message, "ready for review");
    }

    #[test]
    fn missing_task_updates_return_explicit_error() {
        let mut registry = InMemoryRegistry::default();

        let error = registry
            .update_lifecycle(&TaskId::new("missing"), LifecycleStatus::Removed)
            .unwrap_err();

        assert_eq!(error, RegistryError::TaskNotFound(TaskId::new("missing")));
    }

    #[test]
    fn registry_exports_structured_json_snapshot() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
            .unwrap();

        let json = registry.export_json_snapshot().unwrap();

        assert!(json.contains("\"repo\": \"web\""));
        assert!(json.contains("\"handle\": \"fix-login\""));
        assert!(json.contains("\"message\": \"ready\""));
    }

    #[test]
    fn registry_exports_snapshot_file_without_importing_it() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-{}-{}.json",
            std::process::id(),
            "export"
        ));

        registry.export_json_snapshot_file(&path).unwrap();
        let snapshot = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert!(snapshot.contains("\"repo\": \"web\""));
        assert!(snapshot.contains("\"handle\": \"fix-login\""));
    }

    #[test]
    fn registry_has_no_legacy_json_state_import_surface() {
        let source =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/registry.rs"))
                .unwrap();
        let legacy_store = ["Json", "RegistryStore"].concat();
        let legacy_import = ["from", "_json_snapshot"].concat();
        let legacy_file_import = ["load", "_json_snapshot"].concat();

        assert!(!source.contains(&legacy_store), "{legacy_store}");
        assert!(!source.contains(&legacy_import), "{legacy_import}");
        assert!(
            !source.contains(&legacy_file_import),
            "{legacy_file_import}"
        );
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
            kind: RegistryEventKind::Reconciled,
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
        assert_eq!(events[1].kind, RegistryEventKind::Reconciled);
        assert_eq!(events[1].message, "second");
        assert_eq!(
            events[1].occurred_at,
            SystemTime::UNIX_EPOCH + Duration::new(1_700_000_040, 222)
        );
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
