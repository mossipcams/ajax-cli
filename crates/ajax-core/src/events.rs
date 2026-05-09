use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::models::{GitStatus, LiveObservation, LiveStatusKind};

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
        | MonitorEvent::Agent(AgentEvent::Thinking)
        | MonitorEvent::Agent(AgentEvent::Message { .. }) => Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
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
        MonitorEvent::Process(ProcessEvent::Started { .. })
        | MonitorEvent::Process(ProcessEvent::Stdout { .. }) => Some(LiveObservation::new(
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
        MonitorEvent::Repo(RepoEvent::FileChanged { .. })
        | MonitorEvent::Repo(RepoEvent::GitSnapshot { .. }) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{live_observation_from_event, AgentEvent, MonitorEvent, ProcessEvent};
    use crate::models::LiveStatusKind;

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
}
