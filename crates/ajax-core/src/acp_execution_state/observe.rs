//! Projection half: lifecycle activity and observation sampling for a folded state.

use std::time::SystemTime;

use super::{AcpExecutionState, ChildState, ProviderState, RootTurnState, ToolKind, ToolStatus};
use crate::agent_status::{ActivityKind, StatusObservation};
use crate::canonical_agent_event::{observations_from_run_snapshot, AgentPhase, RunSnapshot};

impl AcpExecutionState {
    fn root_activity(&self) -> Option<ActivityKind> {
        match self.turn {
            RootTurnState::Failed => return Some(ActivityKind::Failed),
            RootTurnState::Settled | RootTurnState::Cancelled => return Some(ActivityKind::Done),
            RootTurnState::Idle | RootTurnState::Running => {}
        }
        // Match reducer wait precedence when multiple requests are open.
        if self
            .requests
            .values()
            .any(|kind| *kind == ActivityKind::WaitingInput)
        {
            return Some(ActivityKind::WaitingInput);
        }
        if !self.requests.is_empty() {
            return Some(ActivityKind::WaitingApproval);
        }
        if self.turn != RootTurnState::Running
            || self.provider_state == Some(ProviderState::RequiresAction)
        {
            return None;
        }
        let mut kind = ActivityKind::Working;
        for tool in self.tools.values() {
            if tool.kind == Some(ToolKind::Execute) && tool.status == ToolStatus::Running {
                if tool.is_tests {
                    return Some(ActivityKind::TestsRunning);
                }
                kind = ActivityKind::CommandRunning;
            }
        }
        Some(kind)
    }

    /// Sample the folded state using the canonical lifecycle confidence and
    /// expiry policy. `now` is supplied by the caller, just as for canonical
    /// snapshots; message timestamps never establish activity by themselves.
    /// A root Done is local completion evidence: the reducer holds the final
    /// Done projection while any completion-holding child remains open.
    pub fn observations(&self, root_run_id: &str, now: SystemTime) -> Vec<StatusObservation> {
        let mut observations = Vec::new();
        if let Some(kind) = self.root_activity() {
            observations.extend(observation(kind, root_run_id, None, now));
        }
        for (run_id, child) in &self.children {
            // A child cannot masquerade as the root if an adapter supplies bad lineage.
            if run_id == root_run_id {
                continue;
            }
            let kind = match child.state {
                ChildState::Running => ActivityKind::Working,
                ChildState::WaitingApproval => ActivityKind::WaitingApproval,
                ChildState::WaitingInput => ActivityKind::WaitingInput,
            };
            let parent = child.completion_holding.then_some(root_run_id);
            observations.extend(observation(kind, run_id, parent, now));
        }
        observations
    }
}

fn observation(
    kind: ActivityKind,
    run_id: &str,
    parent: Option<&str>,
    now: SystemTime,
) -> Vec<StatusObservation> {
    let snapshot = RunSnapshot {
        phase: match kind {
            ActivityKind::Done => AgentPhase::Settled,
            ActivityKind::Failed => AgentPhase::Failed,
            _ => AgentPhase::Active,
        },
        activity: None,
        blocker: None,
        outcome: None,
        active_tools: Default::default(),
        pending_attention: None,
    };
    let mut observations = observations_from_run_snapshot(&snapshot, now, run_id);
    for observation in &mut observations {
        observation.kind = kind;
        observation.parent_run_id = parent.map(str::to_owned);
    }
    observations
}

#[cfg(test)]
mod tests {
    use super::super::tests::{kind, now, phase, running, tool, update};
    use super::super::*;
    use crate::agent_status::{
        reduce_agent_status, Confidence, ObservationSource, ParentPhase, ReduceInput,
    };
    use crate::canonical_agent_event::{observations_from_run_snapshot, AgentPhase, RunSnapshot};
    use crate::models::LiveStatusKind;
    use std::time::Duration;

    #[test]
    fn resolution_without_tools_resumes_only_an_active_turn() {
        for active in [false, true] {
            let mut state = if active {
                running()
            } else {
                AcpExecutionState::default()
            };
            state.apply(
                AcpExecutionEvent::InputRequested {
                    request_id: "request".into(),
                },
                now(),
            );
            state.apply(
                AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction),
                now(),
            );
            state.apply(
                AcpExecutionEvent::RequestResolved {
                    request_id: "request".into(),
                },
                now(),
            );
            assert_eq!(kind(&state), Some(ActivityKind::Working));
        }
    }

    #[test]
    fn requests_are_independently_resolved_by_id() {
        let mut state = running();
        for request_id in ["a", "b"] {
            state.apply(
                AcpExecutionEvent::PermissionRequested {
                    request_id: request_id.into(),
                },
                now(),
            );
        }
        for request_id in ["unknown", "a"] {
            state.apply(
                AcpExecutionEvent::RequestResolved {
                    request_id: request_id.into(),
                },
                now(),
            );
            assert_eq!(kind(&state), Some(ActivityKind::WaitingApproval));
        }
        assert_eq!(state.requests.len(), 1);
    }

    #[test]
    fn resolving_a_request_with_no_prompt_means_the_agent_is_back_at_work() {
        let mut state = AcpExecutionState::default();
        state.apply(
            AcpExecutionEvent::PermissionRequested {
                request_id: "p".into(),
            },
            now(),
        );
        assert_eq!(kind(&state), Some(ActivityKind::WaitingApproval));
        state.apply(
            AcpExecutionEvent::RequestResolved {
                request_id: "p".into(),
            },
            now(),
        );
        assert_eq!(kind(&state), Some(ActivityKind::Working));
        assert_eq!(state.turn(), RootTurnState::Running);

        let mut idle = AcpExecutionState::default();
        idle.apply(
            AcpExecutionEvent::RequestResolved {
                request_id: "missing".into(),
            },
            now(),
        );
        assert_eq!(kind(&idle), None);
        assert_eq!(idle.turn(), RootTurnState::Idle);
    }

    #[test]
    fn provider_running_is_coarse_and_preserves_specific_evidence() {
        let mut state = AcpExecutionState::default();
        state.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::Running),
            now(),
        );
        assert_eq!(kind(&state), Some(ActivityKind::Working));
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            Some(ToolStatus::Running),
            None,
        );
        state.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::Running),
            now(),
        );
        assert_eq!(kind(&state), Some(ActivityKind::CommandRunning));
    }

    #[test]
    fn stray_provider_running_does_not_reopen_a_finished_turn() {
        let mut state = running();
        state.apply(AcpExecutionEvent::TurnEnded, now());
        state.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::Running),
            now(),
        );
        assert_eq!(state.turn(), RootTurnState::Settled);
        assert_eq!(kind(&state), Some(ActivityKind::Done));

        let mut cancelled = running();
        cancelled.apply(AcpExecutionEvent::TurnCancelled, now());
        cancelled.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::Running),
            now(),
        );
        assert_eq!(cancelled.turn(), RootTurnState::Cancelled);
        assert_eq!(kind(&cancelled), Some(ActivityKind::Done));

        let mut fresh = AcpExecutionState::default();
        fresh.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::Running),
            now(),
        );
        assert_eq!(fresh.turn(), RootTurnState::Running);
        assert_eq!(kind(&fresh), Some(ActivityKind::Working));
    }

    #[test]
    fn requires_action_without_request_round_trips_to_unknown() {
        let mut state = running();
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            Some(ToolStatus::Running),
            None,
        );
        state.apply(
            AcpExecutionEvent::StateUpdate(ProviderState::RequiresAction),
            now(),
        );
        assert!(state.observations("root", now()).is_empty());

        assert_eq!(phase(&state), ParentPhase::Unknown);
        let projection = reduce_agent_status(ReduceInput {
            now: now(),
            primary_run_id: "root".into(),
            process_liveness: None,
            observations: &state.observations("root", now()),
        });
        assert_eq!(projection.live.kind, LiveStatusKind::Unknown);
    }

    #[test]
    fn provider_idle_is_only_supporting_evidence() {
        let mut state = AcpExecutionState::default();
        state.apply(AcpExecutionEvent::StateUpdate(ProviderState::Idle), now());
        assert_eq!(kind(&state), None);
        assert_eq!(state.provider_state, Some(ProviderState::Idle));
        state.apply(AcpExecutionEvent::PromptAccepted, now());
        state.apply(AcpExecutionEvent::StateUpdate(ProviderState::Idle), now());
        assert_eq!(kind(&state), Some(ActivityKind::Working));
    }

    #[test]
    fn successful_turn_clears_tools_and_requests_and_completes() {
        let mut state = running();
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            Some(ToolStatus::Running),
            None,
        );
        state.apply(
            AcpExecutionEvent::PermissionRequested {
                request_id: "request".into(),
            },
            now(),
        );
        state.apply(AcpExecutionEvent::TurnEnded, now());
        assert_eq!(state.turn(), RootTurnState::Settled);
        assert!(state.tools.is_empty() && state.requests.is_empty());
        assert_eq!(kind(&state), Some(ActivityKind::Done));

        assert_eq!(phase(&state), ParentPhase::FullyCompleted);
    }

    #[test]
    fn turn_failure_is_terminal_and_cancel_cannot_mask_it() {
        let mut state = running();
        state.apply(AcpExecutionEvent::TurnFailed, now());
        assert_eq!(state.turn(), RootTurnState::Failed);
        assert_eq!(kind(&state), Some(ActivityKind::Failed));
        state.apply(AcpExecutionEvent::TurnCancelled, now());
        assert_eq!(kind(&state), Some(ActivityKind::Failed));
    }

    #[test]
    fn cancellation_is_settled_without_failure() {
        let mut state = running();
        tool(&mut state, "tool", None, None, None);
        state.apply(AcpExecutionEvent::TurnCancelled, now());
        assert_eq!(state.turn(), RootTurnState::Cancelled);
        assert_eq!(kind(&state), Some(ActivityKind::Done));
        assert!(state.tools.is_empty());
        state.apply(AcpExecutionEvent::TurnFailed, now());
        assert_eq!(kind(&state), Some(ActivityKind::Failed));
    }

    #[test]
    fn done_is_held_by_open_completion_holding_child_round_trip() {
        let mut state = running();
        state.apply(
            AcpExecutionEvent::ChildStarted {
                run_id: "child".into(),
                completion_holding: true,
            },
            now(),
        );
        state.apply(AcpExecutionEvent::TurnEnded, now());
        let observations = state.observations("root", now());
        assert_eq!(observations.len(), 2);
        assert_eq!(observations[1].parent_run_id.as_deref(), Some("root"));

        assert_eq!(phase(&state), ParentPhase::CompletedLocallyChildrenActive);

        state.apply(
            AcpExecutionEvent::ChildEnded {
                run_id: "child".into(),
                completion_holding: true,
            },
            now(),
        );

        assert_eq!(phase(&state), ParentPhase::FullyCompleted);
    }

    #[test]
    fn explicit_child_updates_track_lineage_and_detachment() {
        let mut state = running();
        for child_state in [
            ChildState::Running,
            ChildState::WaitingInput,
            ChildState::WaitingApproval,
        ] {
            state.apply(
                AcpExecutionEvent::ChildUpdated {
                    run_id: "child".into(),
                    completion_holding: true,
                    state: child_state,
                },
                now(),
            );
            assert_eq!(state.children.len(), 1);
            assert_eq!(state.children["child"].state, child_state);
        }
        state.apply(
            AcpExecutionEvent::ChildUpdated {
                run_id: "child".into(),
                completion_holding: false,
                state: ChildState::Running,
            },
            now(),
        );
        state.apply(AcpExecutionEvent::TurnEnded, now());
        assert_eq!(state.observations("root", now())[1].parent_run_id, None);

        assert_eq!(phase(&state), ParentPhase::FullyCompleted);
    }

    #[test]
    fn message_and_thought_chunks_only_update_activity_time() {
        for event in [
            AcpExecutionEvent::MessageChunk,
            AcpExecutionEvent::ThoughtChunk,
        ] {
            let mut state = AcpExecutionState::default();
            state.apply(event.clone(), now());
            assert_eq!(state.turn(), RootTurnState::Idle);
            assert_eq!(state.last_activity_at(), Some(now()));
            assert_eq!(kind(&state), None);
            assert!(state.children.is_empty());
            let mut active = running();
            let before = active.clone();
            active.apply(event, now());
            assert_eq!(active, before);
        }
    }

    #[test]
    fn unknown_event_changes_nothing_including_timestamp() {
        let mut state = running();
        let before = state.clone();
        state.apply(AcpExecutionEvent::Unknown, now() + Duration::from_secs(1));
        assert_eq!(state, before);
    }

    #[test]
    fn title_update_preserves_tests_flag_and_command_reclassifies() {
        let mut state = running();
        tool(
            &mut state,
            "tool",
            Some(ToolKind::Execute),
            Some(ToolStatus::Running),
            Some("cargo test"),
        );
        assert_eq!(kind(&state), Some(ActivityKind::TestsRunning));
        update(
            &mut state,
            ToolCallFields {
                title: Some("Running command".into()),
                ..Default::default()
            },
        );
        assert_eq!(kind(&state), Some(ActivityKind::TestsRunning));
        update(
            &mut state,
            ToolCallFields {
                command: Some("ls".into()),
                ..Default::default()
            },
        );
        assert_eq!(kind(&state), Some(ActivityKind::CommandRunning));
    }

    #[test]
    fn observations_reuse_canonical_confidence_and_expiry_and_do_not_mutate() {
        let mut state = running();
        for (event, phase) in [
            (AcpExecutionEvent::PromptAccepted, AgentPhase::Active),
            (AcpExecutionEvent::TurnEnded, AgentPhase::Settled),
            (AcpExecutionEvent::TurnFailed, AgentPhase::Failed),
        ] {
            state.apply(event, now());
            let before = state.clone();
            let sampled_at = now() + Duration::from_secs(10);
            let observations = state.observations("root", sampled_at);
            let snapshot = RunSnapshot {
                phase,
                activity: None,
                blocker: None,
                outcome: None,
                active_tools: Default::default(),
                pending_attention: None,
            };
            assert_eq!(
                observations,
                observations_from_run_snapshot(&snapshot, sampled_at, "root")
            );
            assert_eq!(observations[0].source, ObservationSource::ProviderLifecycle);
            assert_eq!(observations[0].confidence, Confidence::High);
            assert_eq!(state, before);
        }
    }
}
