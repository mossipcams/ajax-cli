//! Canonical agent-event kinds, envelope fold, and snapshot projection.
//!
//! Owns facts→snapshot reduction; CLI keeps translate/write/JSONL I/O.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::agent_status::{
    ActivityKind as StatusActivityKind, Confidence, ObservationSource, StatusObservation,
};

/// Freshness window for non-terminal structured provider lifecycle events.
const LIFECYCLE_FRESH_FOR: Duration = Duration::from_secs(30 * 60);
/// Terminal lifecycle events persist until superseded by newer evidence.
const LIFECYCLE_TERMINAL_FRESH_FOR: Duration = Duration::from_secs(365 * 24 * 3600);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalEventKind {
    TurnStarted,
    ActivityStarted,
    ActivityFinished,
    AttentionRequested,
    AttentionCleared,
    TurnSettled,
    SessionOpened,
    SessionClosed,
    ChildStarted,
    ChildSettled,
    Heartbeat,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    Permission,
    Question,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOutcome {
    Completed,
    Interrupted,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CanonicalEventDetail {
    Activity {
        activity: ActivityKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        activity_id: Option<String>,
    },
    Attention {
        attention: AttentionReason,
    },
    Outcome {
        outcome: TurnOutcome,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentPhase {
    Active,
    Blocked,
    Settled,
    Failed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunSnapshot {
    pub phase: AgentPhase,
    pub activity: Option<ActivityKind>,
    pub blocker: Option<AttentionReason>,
    pub outcome: Option<TurnOutcome>,
    pub active_tools: HashMap<String, ()>,
    pub pending_attention: Option<AttentionReason>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ParsedEnvelope {
    pub kind: CanonicalEventKind,
    #[serde(default)]
    pub detail: Option<CanonicalEventDetail>,
    pub received_at_unix_millis: u128,
}

struct FoldState {
    phase: AgentPhase,
    activity: Option<ActivityKind>,
    blocker: Option<AttentionReason>,
    outcome: Option<TurnOutcome>,
    active_tools: HashMap<String, ()>,
    pending_attention: Option<AttentionReason>,
    turn_started: bool,
    turn_settled: bool,
}

impl FoldState {
    fn new() -> Self {
        Self {
            phase: AgentPhase::Unknown,
            activity: None,
            blocker: None,
            outcome: None,
            active_tools: HashMap::new(),
            pending_attention: None,
            turn_started: false,
            turn_settled: false,
        }
    }

    fn snapshot(&self) -> RunSnapshot {
        RunSnapshot {
            phase: self.phase.clone(),
            activity: self.activity.clone(),
            blocker: self.blocker.clone(),
            outcome: self.outcome.clone(),
            active_tools: self.active_tools.clone(),
            pending_attention: self.pending_attention.clone(),
        }
    }

    fn clear_pending_attention(&mut self) {
        self.pending_attention = None;
        self.blocker = None;
        if self.phase == AgentPhase::Blocked && !self.turn_settled {
            self.phase = AgentPhase::Active;
        }
    }

    fn apply_turn_settled(&mut self, outcome: TurnOutcome) {
        self.turn_settled = true;
        self.outcome = Some(outcome.clone());
        if matches!(outcome, TurnOutcome::Failed) {
            self.phase = AgentPhase::Failed;
            return;
        }
        if !self.active_tools.is_empty() {
            self.phase = AgentPhase::Active;
        } else {
            self.phase = AgentPhase::Settled;
        }
    }
}

pub fn fold_envelopes(events: &[ParsedEnvelope]) -> RunSnapshot {
    let mut state = FoldState::new();
    for event in events.iter() {
        if matches!(event.kind, CanonicalEventKind::Heartbeat) {
            continue;
        }
        match &event.kind {
            CanonicalEventKind::TurnStarted => {
                state.turn_started = true;
                state.clear_pending_attention();
                state.phase = AgentPhase::Active;
            }
            CanonicalEventKind::ActivityStarted => {
                if let Some(CanonicalEventDetail::Activity {
                    activity: ActivityKind::Tool,
                    activity_id,
                }) = &event.detail
                {
                    if let Some(id) = activity_id {
                        state.active_tools.insert(id.clone(), ());
                    }
                    state.activity = Some(ActivityKind::Tool);
                    state.phase = AgentPhase::Active;
                }
            }
            CanonicalEventKind::ActivityFinished => {
                if let Some(CanonicalEventDetail::Activity {
                    activity: ActivityKind::Tool,
                    activity_id,
                }) = &event.detail
                {
                    match activity_id {
                        Some(id) => {
                            state.active_tools.remove(id);
                        }
                        None => {
                            state
                                .active_tools
                                .retain(|key, _| !key.starts_with("anon-"));
                        }
                    }
                }
                if state.active_tools.is_empty() {
                    state.activity = None;
                    if state.turn_settled && state.phase != AgentPhase::Failed {
                        if state.pending_attention.is_some() {
                            state.phase = AgentPhase::Blocked;
                        } else {
                            state.phase = AgentPhase::Settled;
                        }
                    } else if state.pending_attention.is_none()
                        && (state.turn_started || !state.active_tools.is_empty())
                    {
                        state.phase = AgentPhase::Active;
                    }
                }
            }
            CanonicalEventKind::AttentionRequested => {
                if let Some(CanonicalEventDetail::Attention { attention }) = &event.detail {
                    state.pending_attention = Some(attention.clone());
                    state.blocker = Some(attention.clone());
                    state.phase = AgentPhase::Blocked;
                }
            }
            CanonicalEventKind::AttentionCleared => {
                state.clear_pending_attention();
            }
            CanonicalEventKind::TurnSettled => {
                let outcome = event
                    .detail
                    .as_ref()
                    .and_then(|detail| {
                        if let CanonicalEventDetail::Outcome { outcome } = detail {
                            Some(outcome.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(TurnOutcome::Unknown);
                state.apply_turn_settled(outcome);
            }
            CanonicalEventKind::SessionOpened | CanonicalEventKind::ChildStarted => {
                state.phase = AgentPhase::Active;
            }
            CanonicalEventKind::SessionClosed => {
                state.active_tools.clear();
                state.activity = None;
                if state.phase != AgentPhase::Failed {
                    state.phase = AgentPhase::Settled;
                }
            }
            CanonicalEventKind::ChildSettled | CanonicalEventKind::Heartbeat => {}
        }
    }
    state.snapshot()
}

/// Map a folded [`RunSnapshot`] onto reducer-ready [`StatusObservation`]s.
pub fn observations_from_run_snapshot(
    snapshot: &RunSnapshot,
    now: SystemTime,
    run_id: &str,
) -> Vec<StatusObservation> {
    let kind = match snapshot.phase {
        AgentPhase::Unknown => return Vec::new(),
        AgentPhase::Active => StatusActivityKind::Working,
        AgentPhase::Blocked => {
            let is_permission = snapshot
                .pending_attention
                .as_ref()
                .is_some_and(|reason| *reason == AttentionReason::Permission)
                || snapshot
                    .blocker
                    .as_ref()
                    .is_some_and(|reason| *reason == AttentionReason::Permission);
            if is_permission {
                StatusActivityKind::WaitingApproval
            } else {
                StatusActivityKind::WaitingInput
            }
        }
        AgentPhase::Settled => StatusActivityKind::Done,
        AgentPhase::Failed => StatusActivityKind::Failed,
    };

    let expires_at = match kind {
        StatusActivityKind::Done | StatusActivityKind::Failed => now + LIFECYCLE_TERMINAL_FRESH_FOR,
        _ => now + LIFECYCLE_FRESH_FOR,
    };

    vec![StatusObservation {
        source: ObservationSource::ProviderLifecycle,
        observed_at: now,
        expires_at,
        confidence: Confidence::High,
        run_id: run_id.to_string(),
        parent_run_id: None,
        kind,
    }]
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use super::{
        fold_envelopes, observations_from_run_snapshot, ActivityKind, AgentPhase, AttentionReason,
        CanonicalEventDetail, CanonicalEventKind, ParsedEnvelope, RunSnapshot, TurnOutcome,
    };
    use crate::agent_status::{reduce_agent_status, ReduceInput};
    use crate::models::LiveStatusKind;

    /// Compact folded-phase label used by the fold tests. Mirrors how the CLI's
    /// legacy string projection read a `RunSnapshot`; kept test-local since no
    /// production code projects a snapshot to a string anymore.
    fn phase_label(snapshot: &RunSnapshot) -> Option<&'static str> {
        match snapshot.phase {
            AgentPhase::Failed => Some("failed"),
            AgentPhase::Blocked => match snapshot
                .pending_attention
                .as_ref()
                .or(snapshot.blocker.as_ref())
            {
                Some(AttentionReason::Permission) => Some("ask"),
                _ => Some("wait"),
            },
            AgentPhase::Settled => Some("done"),
            AgentPhase::Active => Some("working"),
            AgentPhase::Unknown => None,
        }
    }

    fn envelope(
        kind: CanonicalEventKind,
        detail: Option<CanonicalEventDetail>,
        received_at: u128,
    ) -> ParsedEnvelope {
        ParsedEnvelope {
            kind,
            detail,
            received_at_unix_millis: received_at,
        }
    }

    fn tool_started(id: &str, received_at: u128) -> ParsedEnvelope {
        envelope(
            CanonicalEventKind::ActivityStarted,
            Some(CanonicalEventDetail::Activity {
                activity: ActivityKind::Tool,
                activity_id: Some(id.to_string()),
            }),
            received_at,
        )
    }

    fn tool_finished(id: &str, received_at: u128) -> ParsedEnvelope {
        envelope(
            CanonicalEventKind::ActivityFinished,
            Some(CanonicalEventDetail::Activity {
                activity: ActivityKind::Tool,
                activity_id: Some(id.to_string()),
            }),
            received_at,
        )
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(10_000)
    }

    const PRIMARY: &str = "primary";

    #[test]
    fn fold_two_tools_one_finish_stays_working() {
        let events = vec![
            envelope(CanonicalEventKind::TurnStarted, None, 1),
            tool_started("a", 2),
            tool_started("b", 3),
            tool_finished("a", 4),
        ];
        let snapshot = fold_envelopes(&events);
        assert_eq!(phase_label(&snapshot), Some("working"));
        assert!(snapshot.active_tools.contains_key("b"));
    }

    #[test]
    fn fold_attention_then_tool_finish_keeps_ask() {
        let events = vec![
            envelope(
                CanonicalEventKind::AttentionRequested,
                Some(CanonicalEventDetail::Attention {
                    attention: AttentionReason::Permission,
                }),
                1,
            ),
            tool_finished("a", 2),
        ];
        let snapshot = fold_envelopes(&events);
        assert_eq!(phase_label(&snapshot), Some("ask"));
    }

    #[test]
    fn fold_turn_settled_with_open_tool_stays_working() {
        let started = vec![
            tool_started("a", 1),
            envelope(
                CanonicalEventKind::TurnSettled,
                Some(CanonicalEventDetail::Outcome {
                    outcome: TurnOutcome::Completed,
                }),
                2,
            ),
        ];
        let snapshot = fold_envelopes(&started);
        assert_eq!(phase_label(&snapshot), Some("working"));

        let finished = vec![
            tool_started("a", 1),
            envelope(
                CanonicalEventKind::TurnSettled,
                Some(CanonicalEventDetail::Outcome {
                    outcome: TurnOutcome::Completed,
                }),
                2,
            ),
            tool_finished("a", 3),
        ];
        let snapshot = fold_envelopes(&finished);
        assert_eq!(phase_label(&snapshot), Some("done"));
    }

    #[test]
    fn run_snapshot_feeds_reduce_agent_running() {
        let events = vec![envelope(CanonicalEventKind::TurnStarted, None, 1)];
        let snapshot = fold_envelopes(&events);
        let observations = observations_from_run_snapshot(&snapshot, now(), PRIMARY);
        let projection = reduce_agent_status(ReduceInput {
            observations: &observations,
            primary_run_id: PRIMARY.to_string(),
            now: now(),
            process_liveness: None,
        });
        assert_eq!(projection.live.kind, LiveStatusKind::AgentRunning);
    }

    #[test]
    fn fold_activity_started_without_id_then_settled_is_done() {
        let events = vec![
            envelope(
                CanonicalEventKind::ActivityStarted,
                Some(CanonicalEventDetail::Activity {
                    activity: ActivityKind::Tool,
                    activity_id: None,
                }),
                1,
            ),
            envelope(
                CanonicalEventKind::TurnSettled,
                Some(CanonicalEventDetail::Outcome {
                    outcome: TurnOutcome::Completed,
                }),
                2,
            ),
        ];
        let snapshot = fold_envelopes(&events);
        assert!(snapshot.active_tools.is_empty());
        assert_eq!(phase_label(&snapshot), Some("done"));
    }

    #[test]
    fn fold_activity_finished_without_id_does_not_clear_named_tools() {
        let events = vec![
            tool_started("a", 1),
            envelope(
                CanonicalEventKind::ActivityFinished,
                Some(CanonicalEventDetail::Activity {
                    activity: ActivityKind::Tool,
                    activity_id: None,
                }),
                2,
            ),
            envelope(
                CanonicalEventKind::TurnSettled,
                Some(CanonicalEventDetail::Outcome {
                    outcome: TurnOutcome::Completed,
                }),
                3,
            ),
        ];
        let snapshot = fold_envelopes(&events);
        assert!(snapshot.active_tools.contains_key("a"));
        assert_eq!(phase_label(&snapshot), Some("working"));
    }

    #[test]
    fn run_snapshot_feeds_reduce_waiting_approval() {
        let events = vec![envelope(
            CanonicalEventKind::AttentionRequested,
            Some(CanonicalEventDetail::Attention {
                attention: AttentionReason::Permission,
            }),
            1,
        )];
        let snapshot = fold_envelopes(&events);
        let observations = observations_from_run_snapshot(&snapshot, now(), PRIMARY);
        let projection = reduce_agent_status(ReduceInput {
            observations: &observations,
            primary_run_id: PRIMARY.to_string(),
            now: now(),
            process_liveness: None,
        });
        assert_eq!(projection.live.kind, LiveStatusKind::WaitingForApproval);
    }
}
