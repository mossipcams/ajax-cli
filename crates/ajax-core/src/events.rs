use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    live::classify_pane,
    models::{GitStatus, LiveObservation, LiveStatusKind},
    registry::{Registry, RegistryError},
};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AgentEvent {
    Started { agent: String },
    Thinking,
    ToolCall { name: String },
    WaitingForApproval { command: Option<String> },
    WaitingForInput { prompt: String },
    Message { text: String },
    Completed,
    Failed { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum RepoEvent {
    FileChanged {
        path: PathBuf,
    },
    GitSnapshot {
        worktree_path: PathBuf,
        status: GitStatus,
        diff_stat: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum ProcessEvent {
    Started { pid: Option<u32> },
    Stdout { line: String },
    Stderr { line: String },
    Exited { code: Option<i32> },
    Hung { quiet_for: Duration },
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum MonitorEvent {
    Agent(AgentEvent),
    Repo(RepoEvent),
    Process(ProcessEvent),
}

pub fn live_observation_from_event(event: &MonitorEvent) -> Option<LiveObservation> {
    match event {
        MonitorEvent::Agent(AgentEvent::Started { .. })
        | MonitorEvent::Agent(AgentEvent::Thinking) => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        MonitorEvent::Agent(AgentEvent::Message { text }) => Some(classify_text_or_else(
            text,
            LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
        )),
        MonitorEvent::Agent(AgentEvent::ToolCall { name }) => Some(LiveObservation::new(
            LiveStatusKind::CommandRunning,
            format!("tool running: {name}"),
        )),
        MonitorEvent::Agent(AgentEvent::WaitingForApproval { .. }) => Some(LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        )),
        MonitorEvent::Agent(AgentEvent::WaitingForInput { .. }) => Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        )),
        MonitorEvent::Agent(AgentEvent::Completed) => {
            Some(LiveObservation::new(LiveStatusKind::Done, "done"))
        }
        MonitorEvent::Agent(AgentEvent::Failed { message }) => Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            format!("agent failed: {message}"),
        )),
        MonitorEvent::Process(ProcessEvent::Started { .. }) => Some(LiveObservation::new(
            LiveStatusKind::CommandRunning,
            "process running",
        )),
        MonitorEvent::Process(ProcessEvent::Stdout { line }) => Some(classify_text_or_else(
            line,
            LiveObservation::new(LiveStatusKind::CommandRunning, "process running"),
        )),
        MonitorEvent::Process(ProcessEvent::Stderr { line }) => Some(classify_text_or_else(
            line,
            LiveObservation::new(LiveStatusKind::CommandRunning, format!("stderr: {line}")),
        )),
        MonitorEvent::Process(ProcessEvent::Exited { code }) => {
            if code == &Some(0) {
                Some(LiveObservation::new(LiveStatusKind::Done, "done"))
            } else {
                Some(LiveObservation::new(
                    LiveStatusKind::CommandFailed,
                    "process failed",
                ))
            }
        }
        MonitorEvent::Process(ProcessEvent::Hung { .. }) => Some(LiveObservation::new(
            LiveStatusKind::Blocked,
            "process hung",
        )),
        MonitorEvent::Repo(RepoEvent::FileChanged { .. }) => None,
        MonitorEvent::Repo(RepoEvent::GitSnapshot { status, .. }) => status.conflicted.then(|| {
            LiveObservation::new(
                LiveStatusKind::MergeConflict,
                "git conflict needs attention",
            )
        }),
    }
}

pub fn apply_monitor_event_to_task(task: &mut crate::models::Task, event: &MonitorEvent) -> bool {
    let mut changed = false;
    if let MonitorEvent::Repo(RepoEvent::GitSnapshot { status, .. }) = event {
        apply_git_snapshot_to_task(task, status.clone());
        changed = true;
    }

    let Some(observation) = live_observation_from_event(event) else {
        return changed;
    };

    crate::live::apply_observation(task, observation);
    true
}

pub fn apply_monitor_event_to_registry<R: Registry>(
    registry: &mut R,
    task_id: &crate::models::TaskId,
    event: &MonitorEvent,
) -> Result<bool, RegistryError> {
    let mut changed = false;
    if let MonitorEvent::Repo(RepoEvent::GitSnapshot { status, .. }) = event {
        registry.update_git_status(task_id, status.clone())?;
        changed = true;
    }

    let Some(observation) = live_observation_from_event(event) else {
        return Ok(changed);
    };

    registry.apply_live_observation(task_id, observation)?;
    Ok(true)
}

fn apply_git_snapshot_to_task(task: &mut crate::models::Task, status: GitStatus) {
    task.apply_git_status(status);
}

fn classify_text_or_else(text: &str, fallback: LiveObservation) -> LiveObservation {
    let observation = classify_pane(text);
    if observation.kind == LiveStatusKind::Unknown {
        fallback
    } else {
        observation
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        apply_monitor_event_to_task, live_observation_from_event, AgentEvent, MonitorEvent,
        ProcessEvent, RepoEvent,
    };
    use crate::models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveStatusKind, SideFlag,
        Task, TaskId,
    };
    use crate::registry::{InMemoryRegistry, Registry, RegistryEventKind};

    fn task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    #[test]
    fn monitor_events_map_to_live_observations() {
        for (event, expected) in [
            (
                MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                    command: Some("cargo test".to_string()),
                }),
                LiveStatusKind::WaitingForApproval,
            ),
            (
                MonitorEvent::Agent(AgentEvent::ToolCall {
                    name: "shell".to_string(),
                }),
                LiveStatusKind::CommandRunning,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Exited { code: Some(1) }),
                LiveStatusKind::CommandFailed,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Completed),
                LiveStatusKind::Done,
            ),
        ] {
            let observation = live_observation_from_event(&event)
                .expect("event should map to a live observation");

            assert_eq!(observation.kind, expected);
        }
    }

    #[test]
    fn text_bearing_monitor_events_use_live_classifier() {
        for (event, expected) in [
            (
                MonitorEvent::Agent(AgentEvent::Message {
                    text: "Automatic merge failed; fix conflicts and then commit the result."
                        .to_string(),
                }),
                LiveStatusKind::MergeConflict,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Stdout {
                    line: "GitHub Actions failed: test.yml / build".to_string(),
                }),
                LiveStatusKind::CiFailed,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Stderr {
                    line: "CONFLICT (content): merge conflict in src/lib.rs".to_string(),
                }),
                LiveStatusKind::MergeConflict,
            ),
        ] {
            let observation = live_observation_from_event(&event)
                .expect("event should map to a live observation");

            assert_eq!(observation.kind, expected);
        }
    }

    #[test]
    fn generic_stderr_text_does_not_override_process_state() {
        let event = MonitorEvent::Process(ProcessEvent::Stderr {
            line: "warning: previous error: message was printed by the tool".to_string(),
        });

        let observation =
            live_observation_from_event(&event).expect("stderr should keep process activity");

        assert_eq!(observation.kind, LiveStatusKind::CommandRunning);
    }

    #[test]
    fn conflicted_git_snapshots_are_authoritative_status_evidence() {
        let event = MonitorEvent::Repo(RepoEvent::GitSnapshot {
            worktree_path: PathBuf::from("/tmp/worktree"),
            status: GitStatus {
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
            },
            diff_stat: String::new(),
        });

        let observation =
            live_observation_from_event(&event).expect("conflict should become live status");

        assert_eq!(observation.kind, LiveStatusKind::MergeConflict);
    }

    #[test]
    fn clean_git_snapshots_do_not_emit_live_status() {
        let event = MonitorEvent::Repo(RepoEvent::GitSnapshot {
            worktree_path: PathBuf::from("/tmp/worktree"),
            status: GitStatus {
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
            diff_stat: String::new(),
        });

        assert_eq!(live_observation_from_event(&event), None);
    }

    #[test]
    fn agent_monitor_events_apply_live_status_to_task() {
        let mut task = task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::Started {
                agent: "codex".to_string(),
            })
        ));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert!(task.has_side_flag(SideFlag::AgentRunning));

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string()),
            })
        ));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForApproval)
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Waiting);
        assert!(task.has_side_flag(SideFlag::NeedsInput));

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::Completed)
        ));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::Done)
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn process_failure_event_applies_command_failed_attention_state() {
        let mut task = task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Process(ProcessEvent::Exited { code: Some(42) })
        ));

        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(task.has_side_flag(SideFlag::NeedsInput));

        let attention = crate::attention::derive_attention_items(&[task]);
        assert!(attention
            .iter()
            .any(|item| item.reason == "command failed" && item.task_handle == "web/fix-login"));
    }

    #[test]
    fn git_snapshot_event_applies_task_evidence_and_attention_state() {
        let mut task = task();
        let status = GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 1,
            behind: 0,
            merged: false,
            untracked_files: 2,
            unpushed_commits: 1,
            conflicted: true,
            last_commit: Some("abc123".to_string()),
        };

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Repo(RepoEvent::GitSnapshot {
                worktree_path: PathBuf::from("/tmp/worktrees/web-fix-login"),
                status: status.clone(),
                diff_stat: "src/lib.rs | 2 +".to_string(),
            })
        ));

        assert_eq!(task.git_status, Some(status));
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));
        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::Unpushed));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::MergeConflict)
        );

        let attention = crate::attention::derive_attention_items(&[task]);
        assert!(attention
            .iter()
            .any(|item| item.reason == "merge conflict needs attention"));
    }

    #[test]
    fn registry_monitor_event_application_records_evented_updates() {
        let mut registry = InMemoryRegistry::default();
        registry.create_task(task()).unwrap();
        let status = GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 1,
            behind: 0,
            merged: false,
            untracked_files: 2,
            unpushed_commits: 1,
            conflicted: true,
            last_commit: Some("abc123".to_string()),
        };

        let changed = super::apply_monitor_event_to_registry(
            &mut registry,
            &TaskId::new("task-1"),
            &MonitorEvent::Repo(RepoEvent::GitSnapshot {
                worktree_path: PathBuf::from("/tmp/worktrees/web-fix-login"),
                status: status.clone(),
                diff_stat: "src/lib.rs | 2 +".to_string(),
            }),
        )
        .unwrap();

        assert!(changed);
        let task = registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.git_status, Some(status));
        assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::MergeConflict)
        );
        let events = registry.events_for_task(&TaskId::new("task-1"));
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].kind, RegistryEventKind::TaskCreated);
        assert_eq!(events[1].kind, RegistryEventKind::SubstrateChanged);
        assert_eq!(events[1].message, "git evidence changed");
        assert_eq!(events[2].kind, RegistryEventKind::LifecycleChanged);
        assert_eq!(events[2].message, "lifecycle changed to Error");
    }
}
