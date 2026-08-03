use std::{collections::BTreeMap, error::Error, fmt, time::SystemTime};

use crate::lifecycle::{transition_lifecycle, LifecycleTransitionError, LifecycleTransitionReason};
use crate::models::{
    GitStatus, LifecycleStatus, LiveObservation, SideFlag, StepReceipt, StepReceiptIdentity, Task,
    TaskId, TaskWindowStatus, TmuxStatus,
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
    fn update_task_window_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TaskWindowStatus>,
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

    fn adopt_task_branch(
        &mut self,
        task_id: &TaskId,
        expected_branch: &str,
        observed_branch: &str,
    ) -> Result<(), RegistryError> {
        if expected_branch == observed_branch {
            return Err(RegistryError::InvalidBranchAdoption(
                "expected and observed branches must differ".to_string(),
            ));
        }

        let Some(task) = self.get_task_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        if task.branch != expected_branch {
            return Err(RegistryError::InvalidBranchAdoption(format!(
                "task branch intent is {}; expected {expected_branch}",
                task.branch
            )));
        }

        if !task.has_checkout_mismatch() {
            return Err(RegistryError::InvalidBranchAdoption(
                "task no longer has a checkout mismatch".to_string(),
            ));
        }

        let Some(git_status) = task.git_status.clone() else {
            return Err(RegistryError::InvalidBranchAdoption(
                "git evidence is missing".to_string(),
            ));
        };

        if !git_status.worktree_exists {
            return Err(RegistryError::InvalidBranchAdoption(
                "worktree is not present".to_string(),
            ));
        }

        if git_status.current_branch.as_deref() != Some(observed_branch) {
            return Err(RegistryError::InvalidBranchAdoption(format!(
                "observed checkout is {}; expected {observed_branch}",
                git_status.current_branch.as_deref().unwrap_or("detached")
            )));
        }

        task.branch = observed_branch.to_string();
        let mut reconciled = git_status;
        reconciled.branch_exists = true;
        reconciled.current_branch = Some(observed_branch.to_string());
        task.apply_git_status(reconciled);
        refresh_task_annotations(task);

        self.record_event(
            task_id.clone(),
            RegistryEventKind::SubstrateChanged,
            format!("task branch adopted from {expected_branch} to {observed_branch}"),
        )
    }
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

    fn update_task_window_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TaskWindowStatus>,
    ) -> Result<(), RegistryError> {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return Err(RegistryError::TaskNotFound(task_id.clone()));
        };

        task.apply_task_window_status(status);
        refresh_task_annotations(task);
        self.events.push(RegistryEvent::new(
            task_id.clone(),
            RegistryEventKind::SubstrateChanged,
            "task evidence changed",
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateTask(TaskId),
    TaskNotFound(TaskId),
    InvalidLifecycleTransition(LifecycleTransitionError),
    InvalidBranchAdoption(String),
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
            Self::InvalidBranchAdoption(message) => {
                write!(formatter, "invalid branch adoption: {message}")
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
mod tests;
