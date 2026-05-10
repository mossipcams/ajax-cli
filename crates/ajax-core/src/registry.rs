use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::models::{LifecycleStatus, SideFlag, Task, TaskId};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

const SQLITE_SCHEMA_VERSION: i64 = 1;

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
    pub fn to_json_snapshot(&self) -> Result<String, RegistrySnapshotError> {
        let snapshot = RegistrySnapshot {
            tasks: self.tasks.values().cloned().collect(),
            events: self.events.clone(),
        };

        serde_json::to_string_pretty(&snapshot)
            .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))
    }

    pub fn from_json_snapshot(json: &str) -> Result<Self, RegistrySnapshotError> {
        let snapshot: RegistrySnapshot = serde_json::from_str(json)
            .map_err(|error| RegistrySnapshotError::Decode(error.to_string()))?;

        Ok(Self {
            tasks: snapshot
                .tasks
                .into_iter()
                .map(|task| (task.id.clone(), task))
                .collect(),
            events: snapshot.events,
        })
    }

    pub fn save_json_snapshot(&self, path: &Path) -> Result<(), RegistrySnapshotError> {
        let json = self.to_json_snapshot()?;
        std::fs::write(path, json).map_err(|error| RegistrySnapshotError::Io(error.to_string()))
    }

    pub fn load_json_snapshot(path: &Path) -> Result<Self, RegistrySnapshotError> {
        let json = std::fs::read_to_string(path)
            .map_err(|error| RegistrySnapshotError::Io(error.to_string()))?;
        Self::from_json_snapshot(&json)
    }
}

pub trait RegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError>;
    fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JsonRegistryStore {
    path: PathBuf,
}

impl JsonRegistryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl RegistryStore for JsonRegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError> {
        InMemoryRegistry::load_json_snapshot(&self.path)
    }

    fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError> {
        registry.save_json_snapshot(&self.path)
    }
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
        if user_version > SQLITE_SCHEMA_VERSION {
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
                    payload TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS registry_events (
                    sequence INTEGER PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    message TEXT NOT NULL,
                    payload TEXT NOT NULL
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

        let mut task_statement = connection
            .prepare("SELECT payload FROM registry_tasks ORDER BY task_id")
            .map_err(database_error)?;
        let task_payloads = task_statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        let tasks = task_payloads
            .into_iter()
            .map(|payload| {
                serde_json::from_str::<Task>(&payload)
                    .map_err(|error| RegistrySnapshotError::Decode(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut event_statement = connection
            .prepare("SELECT payload FROM registry_events ORDER BY sequence")
            .map_err(database_error)?;
        let event_payloads = event_statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(database_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(database_error)?;
        let events = event_payloads
            .into_iter()
            .map(|payload| {
                serde_json::from_str::<RegistryEvent>(&payload)
                    .map_err(|error| RegistrySnapshotError::Decode(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;

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
            .execute("DELETE FROM registry_tasks", [])
            .map_err(database_error)?;

        for task in registry.tasks.values() {
            let payload = serde_json::to_string(task)
                .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))?;
            transaction
                .execute(
                    "INSERT INTO registry_tasks (task_id, payload) VALUES (?1, ?2)",
                    params![task.id.as_str(), payload],
                )
                .map_err(database_error)?;
        }

        for (sequence, event) in registry.events.iter().enumerate() {
            let payload = serde_json::to_string(event)
                .map_err(|error| RegistrySnapshotError::Encode(error.to_string()))?;
            transaction
                .execute(
                    "INSERT INTO registry_events \
                     (sequence, task_id, kind, message, payload) \
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        sequence as i64,
                        event.task_id.as_str(),
                        format!("{:?}", event.kind),
                        event.message,
                        payload
                    ],
                )
                .map_err(database_error)?;
        }

        transaction.commit().map_err(database_error)
    }
}

fn database_error(error: rusqlite::Error) -> RegistrySnapshotError {
    RegistrySnapshotError::Database(error.to_string())
}

fn sqlite_user_version(connection: &Connection) -> Result<i64, RegistrySnapshotError> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)
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
        InMemoryRegistry, JsonRegistryStore, Registry, RegistryError, RegistryEventKind,
        RegistrySnapshotError, RegistryStore, SqliteRegistryStore,
    };
    use crate::models::{AgentClient, LifecycleStatus, SideFlag, Task, TaskId};

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
                supported: 1,
            }
            .to_string(),
            "incompatible state schema: found 4, supported 1"
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
    fn registry_exports_and_restores_structured_snapshot() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
            .unwrap();

        let json = registry.to_json_snapshot().unwrap();
        let restored = InMemoryRegistry::from_json_snapshot(&json).unwrap();

        assert_eq!(restored.list_tasks().len(), 1);
        assert_eq!(restored.list_tasks()[0].qualified_handle(), "web/fix-login");
        assert_eq!(restored.events_for_task(&TaskId::new("task-1")).len(), 2);
    }

    #[test]
    fn registry_saves_and_loads_snapshot_file() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-{}-{}.json",
            std::process::id(),
            "save-load"
        ));

        registry.save_json_snapshot(&path).unwrap();
        let restored = InMemoryRegistry::load_json_snapshot(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(restored.list_tasks().len(), 1);
        assert_eq!(restored.list_tasks()[0].qualified_handle(), "web/fix-login");
    }

    #[test]
    fn registry_store_abstraction_saves_and_loads_registry_state() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "ajax-registry-store-{}-{}.json",
            std::process::id(),
            "save-load"
        ));
        let store = JsonRegistryStore::new(&path);

        store.save(&registry).unwrap();
        let restored = store.load().unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(restored.list_tasks().len(), 1);
        assert_eq!(restored.list_tasks()[0].qualified_handle(), "web/fix-login");
        assert_eq!(restored.events_for_task(&TaskId::new("task-1")).len(), 2);
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

        assert_eq!(version, 1);
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
                supported: 1
            }
        );
    }
}
