use std::{borrow::Cow, time::SystemTime};

use serde::{Deserialize, Serialize};

use crate::models::{LiveObservation, LiveStatusKind};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AcpActionKind {
    Permission,
    Input,
    Authentication,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AcpStopReason {
    EndTurn,
    Cancelled,
    MaxTokens,
    MaxTurnRequests,
    Refusal,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AcpSessionState {
    Connecting,
    Running,
    RequiresAction(AcpActionKind),
    Idle(Option<AcpStopReason>),
    Failed,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AcpStatusObservation {
    pub state: AcpSessionState,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedAcpStatus {
    pub observation: AcpStatusObservation,
    pub observed_at: SystemTime,
}

pub fn project_acp_status(observation: &AcpStatusObservation) -> LiveObservation {
    let (kind, fallback): (LiveStatusKind, Cow<'_, str>) = match &observation.state {
        AcpSessionState::Connecting => (
            LiveStatusKind::AgentRunning,
            Cow::Borrowed("Connecting to agent"),
        ),
        AcpSessionState::Running => (LiveStatusKind::AgentRunning, Cow::Borrowed("Agent running")),
        AcpSessionState::RequiresAction(AcpActionKind::Permission) => (
            LiveStatusKind::WaitingForApproval,
            Cow::Borrowed("Approval required"),
        ),
        AcpSessionState::RequiresAction(AcpActionKind::Input) => (
            LiveStatusKind::WaitingForInput,
            Cow::Borrowed("Input required"),
        ),
        AcpSessionState::RequiresAction(AcpActionKind::Authentication) => (
            LiveStatusKind::AuthRequired,
            Cow::Borrowed("Authentication required"),
        ),
        AcpSessionState::Idle(None) => (LiveStatusKind::ShellIdle, Cow::Borrowed("Agent idle")),
        AcpSessionState::Idle(Some(AcpStopReason::EndTurn)) => {
            (LiveStatusKind::Done, Cow::Borrowed("Response ready"))
        }
        AcpSessionState::Idle(Some(AcpStopReason::Cancelled)) => {
            (LiveStatusKind::ShellIdle, Cow::Borrowed("Cancelled"))
        }
        AcpSessionState::Idle(Some(AcpStopReason::MaxTokens)) => (
            LiveStatusKind::ContextLimit,
            Cow::Borrowed("Token limit reached"),
        ),
        AcpSessionState::Idle(Some(AcpStopReason::MaxTurnRequests)) => (
            LiveStatusKind::ContextLimit,
            Cow::Borrowed("Turn limit reached"),
        ),
        AcpSessionState::Idle(Some(AcpStopReason::Refusal)) => (
            LiveStatusKind::Blocked,
            Cow::Borrowed("Agent refused the request"),
        ),
        AcpSessionState::Idle(Some(AcpStopReason::Other(reason))) => (
            LiveStatusKind::Unknown,
            Cow::Owned(format!("Unknown ACP stop reason: {reason}")),
        ),
        AcpSessionState::Failed => (
            LiveStatusKind::CommandFailed,
            Cow::Borrowed("ACP session failed"),
        ),
        AcpSessionState::Other(state) => (
            LiveStatusKind::Unknown,
            Cow::Owned(format!("Unknown ACP state: {state}")),
        ),
    };
    let summary = observation
        .detail
        .as_deref()
        .filter(|detail| !detail.trim().is_empty())
        .map(Cow::Borrowed)
        .unwrap_or(fallback);

    LiveObservation::new(kind, summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LiveStatusKind;

    #[test]
    fn projects_known_states_and_stop_reasons() {
        let cases = [
            (
                AcpSessionState::Connecting,
                LiveStatusKind::AgentRunning,
                "Connecting to agent",
            ),
            (
                AcpSessionState::Running,
                LiveStatusKind::AgentRunning,
                "Agent running",
            ),
            (
                AcpSessionState::RequiresAction(AcpActionKind::Permission),
                LiveStatusKind::WaitingForApproval,
                "Approval required",
            ),
            (
                AcpSessionState::RequiresAction(AcpActionKind::Input),
                LiveStatusKind::WaitingForInput,
                "Input required",
            ),
            (
                AcpSessionState::RequiresAction(AcpActionKind::Authentication),
                LiveStatusKind::AuthRequired,
                "Authentication required",
            ),
            (
                AcpSessionState::Idle(None),
                LiveStatusKind::ShellIdle,
                "Agent idle",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::EndTurn)),
                LiveStatusKind::Done,
                "Response ready",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::Cancelled)),
                LiveStatusKind::ShellIdle,
                "Cancelled",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::MaxTokens)),
                LiveStatusKind::ContextLimit,
                "Token limit reached",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::MaxTurnRequests)),
                LiveStatusKind::ContextLimit,
                "Turn limit reached",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::Refusal)),
                LiveStatusKind::Blocked,
                "Agent refused the request",
            ),
            (
                AcpSessionState::Failed,
                LiveStatusKind::CommandFailed,
                "ACP session failed",
            ),
            (
                AcpSessionState::Other("future-state".to_owned()),
                LiveStatusKind::Unknown,
                "Unknown ACP state: future-state",
            ),
            (
                AcpSessionState::Idle(Some(AcpStopReason::Other("future-reason".to_owned()))),
                LiveStatusKind::Unknown,
                "Unknown ACP stop reason: future-reason",
            ),
        ];

        for (state, expected_kind, expected_summary) in cases {
            let observation = AcpStatusObservation {
                state,
                detail: None,
            };

            assert_eq!(
                project_acp_status(&observation),
                LiveObservation::new(expected_kind, expected_summary),
                "state: {:?}",
                observation.state
            );
        }
    }

    #[test]
    fn preserves_non_empty_detail_as_summary() {
        let observation = AcpStatusObservation {
            state: AcpSessionState::Running,
            detail: Some("  provider detail  ".to_owned()),
        };

        assert_eq!(
            project_acp_status(&observation),
            LiveObservation::new(LiveStatusKind::AgentRunning, "  provider detail  ")
        );
    }

    #[test]
    fn empty_detail_uses_mapping_fallback() {
        let observation = AcpStatusObservation {
            state: AcpSessionState::Running,
            detail: Some(" \t\n".to_owned()),
        };

        assert_eq!(
            project_acp_status(&observation),
            LiveObservation::new(LiveStatusKind::AgentRunning, "Agent running")
        );
    }
}
