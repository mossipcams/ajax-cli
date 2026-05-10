#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt};

pub mod codex;
pub mod process;
pub mod renderer;
pub mod repo;
pub mod status;

pub use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};
pub use status::SupervisorStatusMachine;

#[derive(Debug)]
pub enum SupervisorError {
    Io(String),
    Json(String),
    Notify(String),
    Process(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Json(message) => write!(formatter, "json error: {message}"),
            Self::Notify(message) => write!(formatter, "notify error: {message}"),
            Self::Process(message) => write!(formatter, "process error: {message}"),
        }
    }
}

impl Error for SupervisorError {}

impl From<std::io::Error> for SupervisorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for SupervisorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<notify::Error> for SupervisorError {
    fn from(error: notify::Error) -> Self {
        Self::Notify(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ajax_core::models::{GitStatus, LiveStatusKind};

    use super::{
        AgentEvent, MonitorEvent, ProcessEvent, RepoEvent, SupervisorError, SupervisorStatusMachine,
    };

    #[test]
    fn supervisor_errors_have_operator_facing_display() {
        assert_eq!(
            SupervisorError::Process("codex exited".to_string()).to_string(),
            "process error: codex exited"
        );
        assert_eq!(
            SupervisorError::Json("expected value".to_string()).to_string(),
            "json error: expected value"
        );
    }

    #[test]
    fn supervisor_status_machine_reduces_monitor_event_sequences() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Completed));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "late stdout".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::Done)
        );

        status.apply(&MonitorEvent::Process(ProcessEvent::Exited {
            code: Some(1),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CommandFailed)
        );
    }

    #[test]
    fn supervisor_status_machine_preserves_conflict_over_late_output() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Thinking));
        status.apply(&MonitorEvent::Repo(RepoEvent::GitSnapshot {
            worktree_path: PathBuf::from("/tmp/worktree"),
            status: git_status_with_conflicts(),
            diff_stat: String::new(),
        }));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "still streaming logs".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::MergeConflict)
        );
    }

    #[test]
    fn supervisor_status_machine_preserves_ci_failure_over_late_output() {
        let mut status = SupervisorStatusMachine::default();

        status.apply(&MonitorEvent::Agent(AgentEvent::Thinking));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "GitHub Actions failed: test.yml / build".to_string(),
        }));
        status.apply(&MonitorEvent::Process(ProcessEvent::Stdout {
            line: "late stdout".to_string(),
        }));

        assert_eq!(
            status.observation().map(|observation| observation.kind),
            Some(LiveStatusKind::CiFailed)
        );
    }

    fn git_status_with_conflicts() -> GitStatus {
        GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: true,
            last_commit: None,
        }
    }
}
