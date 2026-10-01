//! Watcher vocabulary: frames, decisions, verdicts, phases, and the judge
//! seam. Everything here is pure data; policy and state live beside it.

use serde::{Deserialize, Serialize};

use crate::canonical_agent_event::{AttentionReason, TurnOutcome};

/// The task frame the watcher supervises. Carries the objective text only;
/// task status and lifecycle truth stay in core and are never read here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFrame {
    pub objective: String,
}

/// Watcher phase: the watcher's own opinion of the run. Deliberately
/// separate from task status — a `SuspectedLoop` task may still be
/// `Running`, and a `WaitingOnUser` task may be mid-turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherPhase {
    #[default]
    Healthy,
    SuspectedLoop,
    PrematureStop,
    Recovering,
    WaitingOnUser,
    Escalated,
}

/// Why the watcher intervened or asked for the user.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherReason {
    /// Turn settled as completed with no meaningful activity since the
    /// previous intervention or turn start.
    PrematureStop,
    /// A repeated identical activity signature with no meaningful change.
    RepeatedSignature,
    /// Judge reviewed a suspicious completion and found the work not done.
    SuspiciousCompletion,
    /// Second stop after a nudge with no meaningful activity in between.
    StalledAfterNudge,
    /// Judge judged the agent stuck.
    Stuck,
    /// Judge judged the agent drifting from the objective.
    OffTrack,
    /// Judge judged the run needs the operator, not a nudge.
    NeedsUser,
    /// Grace window after a nudge expired with no meaningful activity.
    GraceExpired,
    /// Intervention budget exhausted; the run is left for the user.
    InterventionCap,
}

/// Watcher output for one step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatcherDecision {
    NoAction,
    Nudge { reason: WatcherReason },
    NeedsUser { reason: WatcherReason },
    AllowStop,
    Escalate { reason: WatcherReason },
}

/// Progress state a judge may report for a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressState {
    Progressing,
    Stuck,
    OffTrack,
    NeedsUser,
    ProbablyDone,
    Uncertain,
}

/// The checkpoint the judge was asked about, recorded in state when a
/// step returns `NeedsJudge`. `apply_verdict` uses it to map a verdict to
/// the right nudge reason and intervention budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PendingCheckpoint {
    /// A completed turn settled with no meaningful activity since the last
    /// intervention or turn start.
    Settle,
    /// A repeated identical activity signature with no meaningful change.
    Loop,
    /// The grace window after a nudge expired with no meaningful activity.
    GraceExpiry,
}

/// Judge output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatcherVerdict {
    pub state: ProgressState,
    /// Judge confidence in `state`, 0.0..=1.0.
    pub confidence: f64,
}

/// Judge failure modes. All fail open: the watcher takes no action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JudgeError {
    /// Judge command missing, refused to start, or exited non-zero.
    Unavailable,
    /// Judge did not answer within the host's timeout.
    Timeout,
    /// Judge answered but the payload was not a valid verdict.
    Malformed,
}

/// Synchronous judge seam. The host (watcher runtime) owns transport and
/// runs this off the event loop; the policy never calls it directly.
pub trait AgentProgressJudge {
    fn evaluate(&self, snapshot: &WatcherSnapshot) -> Result<WatcherVerdict, JudgeError>;
}

/// Compact, bounded view of the run handed to a judge. Everything is a
/// bounded summary: no transcripts, no unbounded event lists.
#[derive(Clone, Debug, PartialEq)]
pub struct WatcherSnapshot {
    pub objective: String,
    pub task_id: String,
    pub run_id: String,
    pub harness: String,
    pub phase: WatcherPhase,
    /// Most recent activity signatures, oldest first.
    pub recent_signatures: Vec<String>,
    /// Most recent event kinds (as short labels), oldest first.
    pub recent_events: Vec<String>,
    pub pending_attention: Option<AttentionReason>,
    pub open_children: u32,
    pub intervention_count: u32,
    pub last_verdict: Option<WatcherVerdict>,
    /// Milliseconds since the last meaningful activity, if any.
    pub ms_since_meaningful_activity: Option<u64>,
}

/// One watcher input event. Defined locally on purpose: the canonical event
/// detail does not yet carry a signature/success (added later), and the
/// watcher must not depend on CLI/web translate code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatcherEventKind {
    TurnStarted,
    ActivityStarted,
    ActivityFinished,
    Attention,
    TurnSettled,
    ChildStarted,
    ChildSettled,
    Heartbeat,
}

/// Detail for a [`WatcherEvent`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WatcherEventDetail {
    /// `activity_id` identifies the open tool; `signature` is a stable
    /// fingerprint of the activity (tool name plus a hash of a normalised
    /// summary); `success` is the observed outcome.
    Activity {
        activity_id: Option<String>,
        signature: Option<String>,
        success: Option<bool>,
    },
    Attention {
        attention: AttentionReason,
    },
    TurnSettled {
        outcome: TurnOutcome,
    },
    None,
}

/// One event fed to the policy step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatcherEvent {
    pub kind: WatcherEventKind,
    pub detail: WatcherEventDetail,
    pub occurred_at_ms: u64,
    pub event_id: String,
}

impl WatcherEventKind {
    /// Short label used in [`WatcherSnapshot::recent_events`].
    pub fn label(self) -> &'static str {
        match self {
            Self::TurnStarted => "turn_started",
            Self::ActivityStarted => "activity_started",
            Self::ActivityFinished => "activity_finished",
            Self::Attention => "attention",
            Self::TurnSettled => "turn_settled",
            Self::ChildStarted => "child_started",
            Self::ChildSettled => "child_settled",
            Self::Heartbeat => "heartbeat",
        }
    }
}
