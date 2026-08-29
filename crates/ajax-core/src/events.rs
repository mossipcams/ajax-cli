use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
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

/// Map a monitor event onto a live observation.
///
/// Event payloads are typed at the source (supervisor/agent parsers, process
/// exit, git snapshots), so text payloads are never keyword-classified here:
/// agent prose about failures, conflicts, or questions is not status
/// evidence. Text-bearing events fall back to their structural meaning — a
/// message is agent activity, a tool call is a running command, stderr is
/// process output — and only genuinely terminal events (exit, completion,
/// failure, hang, git conflict flag) assert actionable states.
pub fn live_observation_from_event(event: &MonitorEvent) -> Option<LiveObservation> {
    match event {
        MonitorEvent::Agent(AgentEvent::Started { .. })
        | MonitorEvent::Agent(AgentEvent::Thinking) => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        MonitorEvent::Agent(AgentEvent::Message { .. }) => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        )),
        MonitorEvent::Agent(AgentEvent::ToolCall { name }) => Some(tool_call_observation(name)),
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
        MonitorEvent::Process(ProcessEvent::Stdout { .. }) => Some(LiveObservation::new(
            LiveStatusKind::CommandRunning,
            "process running",
        )),
        MonitorEvent::Process(ProcessEvent::Stderr { line }) => Some(LiveObservation::new(
            LiveStatusKind::CommandRunning,
            format!("stderr: {line}"),
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

    crate::live::apply_trusted_observation(task, observation);
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

    if registry.get_task(task_id).is_none() {
        return Err(RegistryError::TaskNotFound(task_id.clone()));
    }
    let Some(observation) = live_observation_from_event(event) else {
        return Ok(changed);
    };

    let previous_lifecycle = registry
        .get_task(task_id)
        .map(|task| task.lifecycle_status)
        .ok_or_else(|| RegistryError::TaskNotFound(task_id.clone()))?;
    let current_lifecycle = {
        let task = registry
            .get_task_mut(task_id)
            .ok_or_else(|| RegistryError::TaskNotFound(task_id.clone()))?;
        crate::live::apply_trusted_observation(task, observation);
        task.annotations = crate::attention::annotate(task);
        task.lifecycle_status
    };
    if current_lifecycle != previous_lifecycle {
        registry.record_event(
            task_id.clone(),
            crate::registry::RegistryEventKind::LifecycleChanged,
            format!("lifecycle changed to {current_lifecycle:?}"),
        )?;
    }
    Ok(true)
}

fn apply_git_snapshot_to_task(task: &mut crate::models::Task, status: GitStatus) {
    task.apply_git_status(status);
}

/// A tool call carries the *invocation the agent is executing* (tool name
/// plus command/path), which is structured evidence — unlike prose. When
/// that invocation is a test-runner command, the observation is
/// `TestsRunning`; anything else is a generic running command.
fn tool_call_observation(name: &str) -> LiveObservation {
    if invokes_test_runner(name) {
        LiveObservation::new(
            LiveStatusKind::TestsRunning,
            format!("tests running: {name}"),
        )
    } else {
        LiveObservation::new(
            LiveStatusKind::CommandRunning,
            format!("tool running: {name}"),
        )
    }
}

fn invokes_test_runner(invocation: &str) -> bool {
    [
        "cargo test",
        "cargo nextest",
        "nextest run",
        "npm test",
        "pnpm test",
        "yarn test",
        "pytest",
        "rspec",
    ]
    .iter()
    .any(|runner| invocation.contains(runner))
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
            "task",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    fn claude_task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-claude"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Claude,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task
    }

    #[test]
    fn claude_message_busy_indicator_beats_stale_prompt_text() {
        let mut task = claude_task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::Message {
                text: "Run this command?\n❯ Yes\nctrl+c to interrupt".to_string(),
            })
        ));

        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
    }

    /// Message text is agent prose, never prompt evidence: a Codex composer
    /// pasted into a message is activity, not a waiting state.
    #[test]
    fn message_text_is_never_prompt_classified() {
        let mut task = task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::Message {
                text: "› Fix the tests\n\n  gpt-5.5 high · ~/Desktop/Projects/ajax".to_string(),
            })
        ));

        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    #[test]
    fn generic_live_observation_wrapper_preserves_existing_behavior() {
        for (event, expected) in [
            (
                MonitorEvent::Agent(AgentEvent::Started {
                    agent: "codex".to_string(),
                }),
                LiveStatusKind::AgentRunning,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Completed),
                LiveStatusKind::Done,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Failed {
                    message: "please login to continue".to_string(),
                }),
                LiveStatusKind::CommandFailed,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Started { pid: Some(7) }),
                LiveStatusKind::CommandRunning,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Exited { code: Some(0) }),
                LiveStatusKind::Done,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Exited { code: Some(1) }),
                LiveStatusKind::CommandFailed,
            ),
        ] {
            let observation = live_observation_from_event(&event)
                .expect("event should map to a live observation");

            assert_eq!(observation.kind, expected);
        }
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
                MonitorEvent::Agent(AgentEvent::ToolCall {
                    name: "shell: cargo test --all-features".to_string(),
                }),
                LiveStatusKind::TestsRunning,
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

    /// Text payloads are agent prose or process output, never status
    /// evidence: messages stay activity, process output stays process
    /// activity, and a failed event is a plain command failure regardless of
    /// which keywords its message contains. Actionable states come from
    /// typed events and substrate evidence, not text classification.
    #[test]
    fn text_bearing_monitor_events_never_assert_actionable_states() {
        for (event, expected) in [
            (
                MonitorEvent::Agent(AgentEvent::Message {
                    text: "Automatic merge failed; fix conflicts and then commit the result."
                        .to_string(),
                }),
                LiveStatusKind::AgentRunning,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Stdout {
                    line: "GitHub Actions failed: test.yml / build".to_string(),
                }),
                LiveStatusKind::CommandRunning,
            ),
            (
                MonitorEvent::Process(ProcessEvent::Stderr {
                    line: "CONFLICT (content): merge conflict in src/lib.rs".to_string(),
                }),
                LiveStatusKind::CommandRunning,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Failed {
                    message: "please login to continue".to_string(),
                }),
                LiveStatusKind::CommandFailed,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Failed {
                    message: "rate limit exceeded; try again later".to_string(),
                }),
                LiveStatusKind::CommandFailed,
            ),
            (
                MonitorEvent::Agent(AgentEvent::Failed {
                    message: "manual intervention required; blocked".to_string(),
                }),
                LiveStatusKind::CommandFailed,
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
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
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
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert!(task.has_side_flag(SideFlag::NeedsInput));

        let annotations = crate::attention::annotate(&task);
        assert!(annotations.iter().any(|annotation| {
            annotation.kind == crate::models::AnnotationKind::Broken
                && annotation.evidence
                    == crate::models::Evidence::LiveStatus(LiveStatusKind::CommandFailed)
        }));
    }

    #[test]
    fn trusted_monitor_completion_advances_active_task_to_reviewable() {
        let mut task = task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Agent(AgentEvent::Completed)
        ));

        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Done);
    }

    #[test]
    fn trusted_process_failure_preserves_workflow_lifecycle() {
        let mut task = task();

        assert!(apply_monitor_event_to_task(
            &mut task,
            &MonitorEvent::Process(ProcessEvent::Exited { code: Some(42) })
        ));

        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.agent_status, AgentRuntimeStatus::Blocked);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::CommandFailed)
        );
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

        let annotations = crate::attention::annotate(&task);
        assert!(annotations.iter().any(|annotation| {
            annotation.kind == crate::models::AnnotationKind::Broken
                && annotation.evidence
                    == crate::models::Evidence::LiveStatus(LiveStatusKind::MergeConflict)
        }));
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
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::MergeConflict)
        );
        let events = registry.events_for_task(&TaskId::new("task-1"));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, RegistryEventKind::TaskCreated);
        assert_eq!(events[1].kind, RegistryEventKind::SubstrateChanged);
        assert_eq!(events[1].message, "git evidence changed");
    }
}
