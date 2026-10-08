//! Neutral ACP execution facts folded into provider lifecycle observations.
//! Transport adapters supply events and time; this module performs no I/O.

use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::agent_status::ActivityKind;
use crate::events::{live_observation_from_event, AgentEvent, MonitorEvent};
use crate::models::LiveStatusKind;

mod observe;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RootTurnState {
    #[default]
    Idle,
    Running,
    Settled,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderState {
    Running,
    RequiresAction,
    Idle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolKind {
    Execute,
    Other,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ToolStatus {
    #[default]
    Pending,
    /// The provider's `in_progress` status.
    Running,
    Completed,
    Failed,
}

/// Missing update fields retain their previous values. Invocation text is
/// classified on arrival and is not retained in execution state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ToolCallFields {
    pub kind: Option<ToolKind>,
    pub status: Option<ToolStatus>,
    pub command: Option<String>,
    pub title: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChildState {
    Running,
    WaitingApproval,
    WaitingInput,
}

/// Facts only: adapters normalize protocol spellings and reject malformed
/// payloads as `Unknown`. Children require explicit provider lineage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AcpExecutionEvent {
    PromptAccepted,
    ToolCall {
        tool_call_id: String,
        fields: ToolCallFields,
    },
    ToolCallUpdate {
        tool_call_id: String,
        fields: ToolCallFields,
    },
    PermissionRequested {
        request_id: String,
    },
    InputRequested {
        request_id: String,
    },
    RequestResolved {
        request_id: String,
    },
    StateUpdate(ProviderState),
    TurnEnded,
    TurnFailed,
    TurnCancelled,
    ChildStarted {
        run_id: String,
        completion_holding: bool,
    },
    ChildUpdated {
        run_id: String,
        completion_holding: bool,
        state: ChildState,
    },
    ChildEnded {
        run_id: String,
        completion_holding: bool,
    },
    MessageChunk,
    ThoughtChunk,
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ToolCall {
    kind: Option<ToolKind>,
    status: ToolStatus,
    is_tests: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ChildRun {
    state: ChildState,
    completion_holding: bool,
}

/// A flat execution snapshot, independent of sessions, registries and transport.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AcpExecutionState {
    turn: RootTurnState,
    // Supporting root evidence, including an uncorrelated requires_action.
    provider_state: Option<ProviderState>,
    tools: BTreeMap<String, ToolCall>,
    requests: BTreeMap<String, ActivityKind>,
    children: BTreeMap<String, ChildRun>,
    last_activity_at: Option<SystemTime>,
}

impl AcpExecutionState {
    pub fn turn(&self) -> RootTurnState {
        self.turn
    }

    pub fn last_activity_at(&self) -> Option<SystemTime> {
        self.last_activity_at
    }

    pub fn apply(&mut self, event: AcpExecutionEvent, now: SystemTime) {
        use AcpExecutionEvent::*;
        match event {
            Unknown => return,
            PromptAccepted => {
                self.turn = RootTurnState::Running;
                self.provider_state = None;
                self.tools.clear();
                self.requests.clear();
            }
            ToolCall {
                tool_call_id,
                mut fields,
            } => {
                fields.status = Some(fields.status.unwrap_or_default());
                self.update_tool(tool_call_id, fields);
            }
            ToolCallUpdate {
                tool_call_id,
                fields,
            } => self.update_tool(tool_call_id, fields),
            PermissionRequested { request_id } => {
                self.requests
                    .insert(request_id, ActivityKind::WaitingApproval);
            }
            InputRequested { request_id } => {
                self.requests.insert(request_id, ActivityKind::WaitingInput);
            }
            RequestResolved { request_id } => {
                if self.requests.remove(&request_id).is_some() {
                    self.provider_state = None;
                    if self.turn == RootTurnState::Idle {
                        self.turn = RootTurnState::Running;
                    }
                }
            }
            StateUpdate(state) => {
                self.provider_state = Some(state);
                if state == ProviderState::Running && self.turn == RootTurnState::Idle {
                    self.turn = RootTurnState::Running;
                }
            }
            TurnEnded | TurnFailed | TurnCancelled => {
                self.turn = match event {
                    TurnFailed => RootTurnState::Failed,
                    _ if self.turn == RootTurnState::Failed => RootTurnState::Failed,
                    TurnCancelled => RootTurnState::Cancelled,
                    _ => RootTurnState::Settled,
                };
                self.tools.clear();
                self.requests.clear();
                self.provider_state = None;
            }
            ChildStarted {
                run_id,
                completion_holding,
            } => {
                self.children.insert(
                    run_id,
                    ChildRun {
                        state: ChildState::Running,
                        completion_holding,
                    },
                );
            }
            ChildUpdated {
                run_id,
                completion_holding,
                state,
            } => {
                self.children.insert(
                    run_id,
                    ChildRun {
                        state,
                        completion_holding,
                    },
                );
            }
            ChildEnded { run_id, .. } => {
                self.children.remove(&run_id);
            }
            MessageChunk | ThoughtChunk => {}
        }
        self.last_activity_at = Some(now);
    }

    fn update_tool(&mut self, id: String, fields: ToolCallFields) {
        let tool = self.tools.entry(id).or_default();
        if let Some(kind) = fields.kind {
            tool.kind = Some(kind);
        }
        if let Some(status) = fields.status {
            tool.status = status;
        }
        if let Some(invocation) = fields.command {
            // The command is authoritative: it may set or clear the flag.
            tool.is_tests = Self::is_tests_invocation(&invocation);
        } else if let Some(title) = fields.title {
            // A bare title may only establish the flag, never clear it.
            tool.is_tests |= Self::is_tests_invocation(&title);
        }
    }

    /// Reuse the core recognizer through its public, pure entry point.
    fn is_tests_invocation(invocation: &str) -> bool {
        live_observation_from_event(&MonitorEvent::Agent(AgentEvent::ToolCall {
            name: invocation.to_string(),
        }))
        .is_some_and(|observation| observation.kind == LiveStatusKind::TestsRunning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use crate::agent_status::{reduce_agent_status, ParentPhase, ReduceInput};

    pub(super) fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(10_000)
    }

    pub(super) fn running() -> AcpExecutionState {
        let mut state = AcpExecutionState::default();
        state.apply(AcpExecutionEvent::PromptAccepted, now());
        state
    }

    pub(super) fn kind(state: &AcpExecutionState) -> Option<ActivityKind> {
        state
            .observations("root", now())
            .iter()
            .find(|o| o.run_id == "root")
            .map(|o| o.kind)
    }

    pub(super) fn tool(
        state: &mut AcpExecutionState,
        id: &str,
        kind: Option<ToolKind>,
        status: Option<ToolStatus>,
        command: Option<&str>,
    ) {
        state.apply(
            AcpExecutionEvent::ToolCall {
                tool_call_id: id.into(),
                fields: ToolCallFields {
                    kind,
                    status,
                    command: command.map(str::to_owned),
                    title: None,
                },
            },
            now(),
        );
    }

    pub(super) fn update(state: &mut AcpExecutionState, fields: ToolCallFields) {
        state.apply(
            AcpExecutionEvent::ToolCallUpdate {
                tool_call_id: "tool".into(),
                fields,
            },
            now(),
        );
    }

    pub(super) fn phase(state: &AcpExecutionState) -> ParentPhase {
        reduce_agent_status(ReduceInput {
            now: now(),
            primary_run_id: "root".into(),
            process_liveness: None,
            observations: &state.observations("root", now()),
        })
        .phase
    }

    #[test]
    fn fresh_state_has_only_empty_execution_facts() {
        let state = AcpExecutionState::default();
        assert_eq!(state.turn(), RootTurnState::Idle);
        assert!(state.tools.is_empty() && state.requests.is_empty() && state.children.is_empty());
        assert_eq!(state.last_activity_at(), None);
        assert_eq!(kind(&state), None);
    }

    #[test]
    fn prompt_acceptance_proves_working() {
        let state = running();
        assert_eq!(state.turn(), RootTurnState::Running);
        assert_eq!(kind(&state), Some(ActivityKind::Working));
        assert_eq!(state.last_activity_at(), Some(now()));
    }

    #[test]
    fn tool_call_defaults_to_pending_and_updates_existing_id() {
        let mut state = running();
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            None,
            Some("cargo test"),
        );
        assert_eq!(state.tools["tool"].status, ToolStatus::Pending);
        assert_eq!(kind(&state), Some(ActivityKind::Working));
        tool(&mut state, "tool", None, Some(ToolStatus::Running), None);
        assert_eq!(state.tools.len(), 1);
        assert_eq!(kind(&state), Some(ActivityKind::TestsRunning));
        tool(&mut state, "tool", None, None, None);
        assert_eq!(state.tools["tool"].status, ToolStatus::Pending);
    }

    #[test]
    fn tool_updates_merge_absent_fields_and_create_unknown_ids() {
        let mut state = running();
        update(
            &mut state,
            ToolCallFields {
                kind: Some(ToolKind::Execute),
                title: Some("cargo test".into()),
                ..Default::default()
            },
        );
        assert_eq!(state.tools["tool"].status, ToolStatus::Pending);
        for status in [
            ToolStatus::Running,
            ToolStatus::Completed,
            ToolStatus::Failed,
        ] {
            update(
                &mut state,
                ToolCallFields {
                    status: Some(status),
                    ..Default::default()
                },
            );
            let stored = &state.tools["tool"];
            assert_eq!(stored.kind, Some(ToolKind::Execute));
            assert_eq!(stored.status, status);
            assert!(stored.is_tests);
        }
        assert_eq!(kind(&state), Some(ActivityKind::Working));
    }

    #[test]
    fn running_execute_tool_proves_command_running() {
        let mut state = running();
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            Some(ToolStatus::Running),
            Some("cargo build"),
        );
        assert_eq!(kind(&state), Some(ActivityKind::CommandRunning));
    }

    #[test]
    fn tests_use_existing_core_recognizer_and_outrank_other_tools() {
        for command in ["cargo test", "cargo nextest run", "npm test", "pytest"] {
            let mut state = running();
            tool(
                &mut state,
                "build",
                Some(ToolKind::Execute),
                Some(ToolStatus::Running),
                Some("cargo build"),
            );
            tool(
                &mut state,
                "test",
                Some(ToolKind::Execute),
                Some(ToolStatus::Running),
                Some(command),
            );
            assert_eq!(kind(&state), Some(ActivityKind::TestsRunning));
        }
    }

    #[test]
    fn other_or_missing_tool_kind_degrades_to_working() {
        for tool_kind in [None, Some(ToolKind::Other)] {
            for status in [ToolStatus::Pending, ToolStatus::Running] {
                let mut state = running();
                tool(
                    &mut state,
                    "tool",
                    tool_kind,
                    Some(status),
                    Some("cargo test"),
                );
                assert_eq!(kind(&state), Some(ActivityKind::Working));
                assert!(state.children.is_empty());
            }
        }
    }

    #[test]
    fn pending_permission_or_input_outranks_tools_and_resolution_resumes() {
        for command in ["cargo build", "cargo test"] {
            for input in [false, true] {
                let mut state = running();
                tool(
                    &mut state,
                    "tool",
                    Some(ToolKind::Execute),
                    Some(ToolStatus::Running),
                    Some(command),
                );
                let before = kind(&state);
                let request_id = "request".into();
                state.apply(
                    if input {
                        AcpExecutionEvent::InputRequested { request_id }
                    } else {
                        AcpExecutionEvent::PermissionRequested { request_id }
                    },
                    now(),
                );
                state.apply(
                    AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction),
                    now(),
                );
                assert_eq!(
                    kind(&state),
                    Some(if input {
                        ActivityKind::WaitingInput
                    } else {
                        ActivityKind::WaitingApproval
                    })
                );
                state.apply(
                    AcpExecutionEvent::RequestResolved {
                        request_id: "request".into(),
                    },
                    now(),
                );
                assert_eq!(kind(&state), before);
            }
        }
    }
}
