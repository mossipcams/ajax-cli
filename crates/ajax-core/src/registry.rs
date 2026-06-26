use std::{collections::BTreeMap, error::Error, fmt, time::SystemTime};

use crate::lifecycle::{transition_lifecycle, LifecycleTransitionError, LifecycleTransitionReason};
use crate::models::{
    GitStatus, LifecycleStatus, LiveObservation, SideFlag, StepReceipt, StepReceiptIdentity, Task,
    TaskId, TmuxStatus, WorktrunkStatus,
};
use serde::{Deserialize, Serialize};

mod sqlite;

pub use sqlite::SqliteRegistryStore;

pub trait Registry {
    fn create_task(&mut self, task: Task) -> Result<(), RegistryError>;
    fn delete_task(&mut self, task_id: &TaskId) -> Result<(), RegistryError>;
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
    fn update_git_status(
        &mut self,
        task_id: &TaskId,
        status: GitStatus,
    ) -> Result<(), RegistryError>;
    fn update_tmux_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TmuxStatus>,
    ) -> Result<(), RegistryError>;
    fn update_worktrunk_status(
        &mut self,
        task_id: &TaskId,
        status: Option<WorktrunkStatus>,
    ) -> Result<(), RegistryError>;
    fn apply_live_observation(
        &mut self,
        task_id: &TaskId,
        observation: LiveObservation,
    ) -> Result<(), RegistryError>;
    fn list_events(&self) -> Vec<&RegistryEvent>;
    fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent>;
    fn record_step_receipt(&mut self, receipt: StepReceipt) -> Result<(), RegistryError>;
    fn step_receipts_for_task(&self, task_id: &TaskId) -> Vec<&StepReceipt>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryRegistry {
    tasks: BTreeMap<TaskId, Task>,
    events: Vec<RegistryEvent>,
    step_receipts: BTreeMap<StepReceiptIdentity, StepReceipt>,
}

impl Registry for InMemoryRegistry {
    fn create_task(&mut self, mut task: Task) -> Result<(), RegistryError> {
        let task_id = task.id.clone();

        if let Some(existing) = self.tasks.get(&task_id) {
            if existing.lifecycle_status != LifecycleStatus::Removed {
                return Err(RegistryError::DuplicateTask(task_id));
            }
        }

        task.refresh_runtime_projection();
        refresh_task_annotations(&mut task);
        self.tasks.insert(task_id.clone(), task);
        self.events.retain(|event| event.task_id != task_id);
        self.step_receipts
            .retain(|identity, _| identity.task_id != task_id);
        self.events.push(RegistryEvent::new(
            task_id,
            RegistryEventKind::TaskCreated,
            "task created",
        ));

        Ok(())
    }

    fn delete_task(&mut self, task_id: &TaskId) -> Result<(), RegistryError> {
        if self.tasks.remove(task_id).is_none() {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        }

        self.events.retain(|event| &event.task_id != task_id);
        self.step_receipts
            .retain(|identity, _| &identity.task_id != task_id);

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

        transition_lifecycle(task, status, LifecycleTransitionReason::Generic)
            .map_err(RegistryError::InvalidLifecycleTransition)?;

        task.last_activity_at = SystemTime::now();
        task.remove_side_flag(SideFlag::Stale);
        refresh_task_annotations(task);
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

    fn update_git_status(
        &mut self,
        task_id: &TaskId,
        status: GitStatus,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        task.apply_git_status(status);
        refresh_task_annotations(task);
        self.events.push(RegistryEvent::new(
            task_id.clone(),
            RegistryEventKind::SubstrateChanged,
            "git evidence changed",
        ));

        Ok(())
    }

    fn update_tmux_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TmuxStatus>,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        task.apply_tmux_status(status);
        refresh_task_annotations(task);
        self.events.push(RegistryEvent::new(
            task_id.clone(),
            RegistryEventKind::SubstrateChanged,
            "tmux evidence changed",
        ));

        Ok(())
    }

    fn update_worktrunk_status(
        &mut self,
        task_id: &TaskId,
        status: Option<WorktrunkStatus>,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        task.apply_worktrunk_status(status);
        refresh_task_annotations(task);
        self.events.push(RegistryEvent::new(
            task_id.clone(),
            RegistryEventKind::SubstrateChanged,
            "worktrunk evidence changed",
        ));

        Ok(())
    }

    fn apply_live_observation(
        &mut self,
        task_id: &TaskId,
        observation: LiveObservation,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };
        let previous_lifecycle = task.lifecycle_status;

        crate::live::apply_observation(task, observation);
        refresh_task_annotations(task);

        if task.lifecycle_status != previous_lifecycle {
            self.events.push(RegistryEvent::new(
                task_id.clone(),
                RegistryEventKind::LifecycleChanged,
                format!("lifecycle changed to {:?}", task.lifecycle_status),
            ));
        }

        Ok(())
    }

    fn list_events(&self) -> Vec<&RegistryEvent> {
        self.events.iter().collect()
    }

    fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent> {
        self.events
            .iter()
            .filter(|event| &event.task_id == task_id)
            .collect()
    }

    fn record_step_receipt(&mut self, receipt: StepReceipt) -> Result<(), RegistryError> {
        if !self.tasks.contains_key(&receipt.task_id) {
            return Err(RegistryError::TaskNotFound(receipt.task_id));
        }

        self.step_receipts.insert(receipt.identity(), receipt);

        Ok(())
    }

    fn step_receipts_for_task(&self, task_id: &TaskId) -> Vec<&StepReceipt> {
        let mut receipts = self
            .step_receipts
            .values()
            .filter(|receipt| &receipt.task_id == task_id)
            .collect::<Vec<_>>();
        receipts.sort_by_key(|receipt| {
            (
                receipt.created_at,
                receipt.operation,
                receipt.step_key.as_str(),
                receipt.target.as_str(),
            )
        });
        receipts
    }
}

fn refresh_task_annotations(task: &mut Task) {
    task.annotations = crate::attention::annotate(task);
}

pub trait RegistryStore {
    fn load(&self) -> Result<InMemoryRegistry, RegistrySnapshotError>;
    fn save(&self, registry: &InMemoryRegistry) -> Result<(), RegistrySnapshotError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateTask(TaskId),
    TaskNotFound(TaskId),
    InvalidLifecycleTransition(LifecycleTransitionError),
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
            Self::InvalidLifecycleTransition(error) => write!(
                formatter,
                "invalid lifecycle transition: {:?} -> {:?} ({:?})",
                error.from, error.to, error.reason
            ),
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
    RevisionConflict { expected: u64, actual: u64 },
    IncompatibleSchema { found: i64, supported: i64 },
    LegacySqlitePayloadSchema,
    EmptyRegistryOverwrite,
}

impl fmt::Display for RegistrySnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(message) => write!(formatter, "state encode failed: {message}"),
            Self::Decode(message) => write!(formatter, "state decode failed: {message}"),
            Self::Database(message) => write!(formatter, "database error: {message}"),
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::RevisionConflict { expected, actual } => write!(
                formatter,
                "state revision conflict: expected {expected}, found {actual}"
            ),
            Self::IncompatibleSchema { found, supported } => write!(
                formatter,
                "incompatible state schema: found {found}, supported {supported}"
            ),
            Self::LegacySqlitePayloadSchema => write!(
                formatter,
                "legacy SQLite payload schema is unsupported after the typed state rewrite; remove the state database to start fresh"
            ),
            Self::EmptyRegistryOverwrite => write!(
                formatter,
                "refusing to save empty registry over non-empty disk state"
            ),
        }
    }
}

impl Error for RegistrySnapshotError {}

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
    SubstrateChanged,
    UserNote,
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::{
        InMemoryRegistry, Registry, RegistryError, RegistryEventKind, RegistrySnapshotError,
    };
    use crate::models::{
        AgentClient, AnnotationKind, GitStatus, LifecycleStatus, LiveObservation, RuntimeHealth,
        SideFlag, StepReceipt, Task, TaskId, TaskOperationKind, TmuxStatus, WorktrunkStatus,
    };

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
    fn create_task_replaces_removed_task_tombstone() {
        let mut registry = InMemoryRegistry::default();
        let mut removed = task("task-1", "web", "fix-login");
        removed.lifecycle_status = LifecycleStatus::Removed;
        registry.create_task(removed).unwrap();

        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();

        let task = registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Created);
        assert_eq!(registry.events_for_task(&TaskId::new("task-1")).len(), 1);
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
                supported: 3,
            }
            .to_string(),
            "incompatible state schema: found 4, supported 3"
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
    fn listed_tasks_carry_annotations() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();

        registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Reviewable)
            .unwrap();

        let tasks = registry.list_tasks();

        assert_eq!(tasks[0].annotations.len(), 1);
        assert_eq!(tasks[0].annotations[0].kind, AnnotationKind::Reviewable);
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
    fn invalid_lifecycle_update_is_rejected_without_mutating_task() {
        let mut registry = InMemoryRegistry::default();
        let mut task = task("task-1", "web", "fix-login");
        task.lifecycle_status = LifecycleStatus::Merged;
        registry.create_task(task).unwrap();

        let result = registry.update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Active);

        assert!(result.is_err());
        assert_eq!(
            registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Merged
        );
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
    fn records_step_receipts_idempotently_by_logical_identity() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        let receipt = StepReceipt::succeeded(
            TaskId::new("task-1"),
            TaskOperationKind::Start,
            "worktree_created",
            "/tmp/worktrees/web-fix-login",
            r#"{"attempt":1}"#,
        );

        registry.record_step_receipt(receipt.clone()).unwrap();
        registry.record_step_receipt(receipt).unwrap();

        let receipts = registry.step_receipts_for_task(&TaskId::new("task-1"));
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].step_key, "worktree_created");
    }

    #[test]
    fn lifecycle_updates_record_central_registry_events() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();

        registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Active)
            .unwrap();

        let events = registry.events_for_task(&TaskId::new("task-1"));
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].kind, RegistryEventKind::LifecycleChanged);
        assert_eq!(events[1].message, "lifecycle changed to Active");
    }

    #[test]
    fn substrate_evidence_updates_record_central_registry_events() {
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
                    dirty: false,
                    ahead: 0,
                    behind: 0,
                    merged: false,
                    untracked_files: 0,
                    unpushed_commits: 0,
                    conflicted: false,
                    last_commit: None,
                },
            )
            .unwrap();
        registry
            .update_tmux_status(
                &TaskId::new("task-1"),
                Some(TmuxStatus::present("ajax-web-fix-login")),
            )
            .unwrap();
        registry
            .update_worktrunk_status(
                &TaskId::new("task-1"),
                Some(WorktrunkStatus::present("worktrunk", "/tmp/web")),
            )
            .unwrap();

        let events = registry.events_for_task(&TaskId::new("task-1"));
        assert_eq!(events.len(), 4);
        assert_eq!(events[1].kind, RegistryEventKind::SubstrateChanged);
        assert_eq!(events[1].message, "git evidence changed");
        assert_eq!(events[2].message, "tmux evidence changed");
        assert_eq!(events[3].message, "worktrunk evidence changed");
    }

    #[test]
    fn substrate_evidence_updates_maintain_side_flags() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();

        registry
            .update_git_status(
                &TaskId::new("task-1"),
                GitStatus {
                    worktree_exists: false,
                    branch_exists: false,
                    current_branch: None,
                    dirty: true,
                    ahead: 1,
                    behind: 0,
                    merged: false,
                    untracked_files: 2,
                    unpushed_commits: 1,
                    conflicted: true,
                    last_commit: None,
                },
            )
            .unwrap();
        registry
            .update_tmux_status(
                &TaskId::new("task-1"),
                Some(TmuxStatus {
                    exists: false,
                    session_name: "ajax-web-fix-login".to_string(),
                }),
            )
            .unwrap();
        registry
            .update_worktrunk_status(
                &TaskId::new("task-1"),
                Some(WorktrunkStatus {
                    exists: false,
                    window_name: "worktrunk".to_string(),
                    current_path: "/tmp/web".into(),
                    points_at_expected_path: false,
                }),
            )
            .unwrap();

        let task = registry.get_task(&TaskId::new("task-1")).unwrap();
        for flag in [
            SideFlag::WorktreeMissing,
            SideFlag::BranchMissing,
            SideFlag::Dirty,
            SideFlag::Conflicted,
            SideFlag::Unpushed,
            SideFlag::TmuxMissing,
            SideFlag::WorktrunkMissing,
        ] {
            assert!(task.has_side_flag(flag), "missing side flag: {flag:?}");
        }

        registry
            .update_git_status(
                &TaskId::new("task-1"),
                GitStatus {
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
                },
            )
            .unwrap();
        registry
            .update_tmux_status(
                &TaskId::new("task-1"),
                Some(TmuxStatus::present("ajax-web-fix-login")),
            )
            .unwrap();
        registry
            .update_worktrunk_status(
                &TaskId::new("task-1"),
                Some(WorktrunkStatus::present("worktrunk", "/tmp/web")),
            )
            .unwrap();

        let task = registry.get_task(&TaskId::new("task-1")).unwrap();
        for flag in [
            SideFlag::WorktreeMissing,
            SideFlag::BranchMissing,
            SideFlag::Dirty,
            SideFlag::Conflicted,
            SideFlag::Unpushed,
            SideFlag::TmuxMissing,
            SideFlag::WorktrunkMissing,
        ] {
            assert!(!task.has_side_flag(flag), "unexpected side flag: {flag:?}");
        }
    }

    #[test]
    fn registry_creation_refreshes_runtime_projection_from_hydrated_evidence() {
        let mut registry = InMemoryRegistry::default();
        let mut task = task("task-1", "web", "fix-login");
        task.git_status = Some(GitStatus {
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
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present("worktrunk", "/tmp/web"));

        registry.create_task(task).unwrap();

        assert_eq!(
            registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .runtime_projection
                .health,
            RuntimeHealth::Healthy
        );
    }

    #[test]
    fn live_observation_updates_runtime_without_lifecycle_event() {
        let mut registry = InMemoryRegistry::default();
        let mut task = task("task-1", "web", "fix-login");
        task.lifecycle_status = LifecycleStatus::Active;
        registry.create_task(task).unwrap();

        registry
            .apply_live_observation(
                &TaskId::new("task-1"),
                LiveObservation::new(crate::live::LiveStatusKind::WaitingForInput, "waiting"),
            )
            .unwrap();

        let task = registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(crate::live::LiveStatusKind::WaitingForInput)
        );
        let events = registry.events_for_task(&TaskId::new("task-1"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, RegistryEventKind::TaskCreated);
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
    fn delete_task_removes_task_events_and_receipts() {
        let mut registry = InMemoryRegistry::default();
        let task_id = TaskId::new("task-1");
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(task_id.clone(), RegistryEventKind::UserNote, "ready")
            .unwrap();
        registry
            .record_step_receipt(StepReceipt {
                task_id: task_id.clone(),
                operation: TaskOperationKind::Drop,
                step_key: "worktree_absent".to_string(),
                target: "/tmp/web".to_string(),
                status: crate::models::StepReceiptStatus::Succeeded,
                receipt_json: "{}".to_string(),
                created_at: SystemTime::UNIX_EPOCH,
            })
            .unwrap();

        registry.delete_task(&task_id).unwrap();

        assert!(registry.get_task(&task_id).is_none());
        assert!(registry.events_for_task(&task_id).is_empty());
        assert!(registry.step_receipts_for_task(&task_id).is_empty());
    }

    #[test]
    fn registry_exposes_typed_snapshot_parts_for_output_boundary() {
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(task("task-1", "web", "fix-login"))
            .unwrap();
        registry
            .record_event(TaskId::new("task-1"), RegistryEventKind::UserNote, "ready")
            .unwrap();

        let tasks = registry.list_tasks();
        let events = registry.list_events();

        assert_eq!(tasks[0].repo, "web");
        assert_eq!(tasks[0].handle, "fix-login");
        assert_eq!(events[1].message, "ready");
    }
}
