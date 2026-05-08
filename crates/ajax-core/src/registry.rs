use std::{collections::BTreeMap, path::Path, time::SystemTime};

use crate::models::{LifecycleStatus, Task, TaskId};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateTask(TaskId),
    TaskNotFound(TaskId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistrySnapshotError {
    Encode(String),
    Decode(String),
    Io(String),
}

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
    use super::{InMemoryRegistry, Registry, RegistryError, RegistryEventKind};
    use crate::models::{AgentClient, LifecycleStatus, Task, TaskId};

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
}
