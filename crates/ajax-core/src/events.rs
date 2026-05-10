use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::{
    live::classify_pane,
    models::{GitStatus, LiveObservation, LiveStatusKind},
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

    use super::{live_observation_from_event, AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};
    use crate::models::{GitStatus, LiveStatusKind};

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
}
