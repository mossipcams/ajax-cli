//! Watcher vocabulary: frames, decisions, verdicts, phases, and the judge
//! seam. Everything here is pure data; policy and state live beside it.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

use crate::canonical_agent_event::{AttentionReason, TurnOutcome};

/// The task frame the watcher supervises. Carries the objective text only;
/// task status and lifecycle truth stay in core and are never read here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskFrame {
    pub objective: String,
}

/// Watcher phase: the watcher's own opinion of the run. Deliberately
/// separate from task status — a `Recovering` task may still be
/// `Running`, and a `WaitingOnUser` task may be mid-turn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherPhase {
    #[default]
    #[serde(alias = "suspected_loop", alias = "premature_stop")]
    Healthy,
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
    #[serde(alias = "suspicious_completion")]
    PrematureStop,
    /// Second stop after a nudge with no meaningful activity in between.
    StalledAfterNudge,
    /// Judge judged the agent stuck.
    #[serde(alias = "repeated_signature")]
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
    SessionOpened,
    SessionClosed,
    ActivityStarted,
    ActivityFinished,
    Attention,
    AttentionCleared,
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
            Self::SessionOpened => "session_opened",
            Self::SessionClosed => "session_closed",
            Self::ActivityStarted => "activity_started",
            Self::ActivityFinished => "activity_finished",
            Self::Attention => "attention",
            Self::AttentionCleared => "attention_cleared",
            Self::TurnSettled => "turn_settled",
            Self::ChildStarted => "child_started",
            Self::ChildSettled => "child_settled",
            Self::Heartbeat => "heartbeat",
        }
    }
}

/// Bounded watcher state for one task/run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WatcherState {
    pub task_id: String,
    pub run_id: String,
    pub harness: String,
    pub objective: String,
    pub phase: WatcherPhase,
    /// Most recent event labels, oldest first. Bounded.
    pub recent_events: VecDeque<String>,
    /// Most recent activity signatures, oldest first. Bounded.
    pub recent_signatures: VecDeque<String>,
    /// Timestamp (ms) of the last meaningful activity, if any.
    pub last_meaningful_activity_ms: Option<u64>,
    pub pending_attention: Option<AttentionReason>,
    /// Checkpoint awaiting a judge verdict, if `step` asked for one.
    pub pending_checkpoint: Option<PendingCheckpoint>,
    /// Signature whose loop edge has already raised a checkpoint.
    #[serde(default)]
    pub loop_checkpoint_signature: Option<String>,
    /// Open tool ids. Bounded; the count is what policy uses.
    pub open_tools: Vec<String>,
    /// Start timestamps, bounded by the same open-tool ids.
    #[serde(default)]
    pub(super) open_tool_started_at_ms: HashMap<String, u64>,
    pub open_children: u32,
    #[serde(skip)]
    pub(super) child_started_at_ms: VecDeque<u64>,
    /// A completed reply without meaningful progress inside post-nudge grace.
    #[serde(skip)]
    pub settled_in_grace: bool,
    pub intervention_count: u32,
    pub premature_stop_nudges: u32,
    pub loop_nudges: u32,
    pub last_intervention_at_ms: Option<u64>,
    pub last_verdict: Option<WatcherVerdict>,
    pub grace_deadline_ms: Option<u64>,
    /// Recently seen event ids for dedupe. Bounded; oldest evicted first.
    pub seen_event_ids: VecDeque<String>,
    /// Monotonic index of the last applied event.
    pub event_index: u64,
    /// Monotonic nudge counter, never reset: nudge ids built from it stay
    /// unique across episode budget resets.
    #[serde(default)]
    pub nudge_seq: u64,
    /// Nudges ever issued for this run, never reset: the lifetime cap stops
    /// a loop that refills the per-episode budgets with fresh activity.
    #[serde(default)]
    pub lifetime_nudges: u64,
}

/// The persisted subset of [`WatcherState`], stored in task metadata.
/// Everything else (rings, counters of the moment) is rebuilt from event
/// replay or simply starts fresh.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherPersistedState {
    pub intervention_count: u32,
    pub premature_stop_nudges: u32,
    pub loop_nudges: u32,
    /// Watcher phase, so Escalated/WaitingOnUser survive refreshes.
    #[serde(default)]
    pub phase: WatcherPhase,
    /// Monotonic nudge counter, never reset: nudge ids built from it stay
    /// unique across episode budget resets.
    #[serde(default)]
    pub nudge_seq: u64,
    /// Nudges ever issued for this run, never reset: the lifetime cap stops
    /// a loop that refills the per-episode budgets with fresh activity.
    #[serde(default)]
    pub lifetime_nudges: u64,
    pub last_intervention_at_ms: Option<u64>,
    pub grace_deadline_ms: Option<u64>,
    pub last_verdict: Option<WatcherVerdict>,
    pub last_seen_event_id: Option<String>,
}

/// Caps and windows for the watcher. Intervention budgets are per episode:
/// meaningful non-repeating activity after a nudge, or a fresh user turn
/// after escalation, resets them. `max_lifetime_nudges` bounds the total
/// nudges across all episodes and never resets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatcherConfig {
    pub max_recent_events: usize,
    pub max_recent_signatures: usize,
    pub max_seen_event_ids: usize,
    /// Identical signatures within the recent window that count as a loop.
    pub repeat_threshold: u32,
    pub max_premature_stop_nudges: u32,
    pub max_loop_nudges: u32,
    pub max_total_interventions: u32,
    /// Nudges ever issued before the run escalates, even when fresh
    /// activity refills the per-episode budgets. Never resets.
    pub max_lifetime_nudges: u32,
    /// Window after a nudge in which the agent can still recover.
    pub grace_period_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            max_recent_events: 64,
            max_recent_signatures: 32,
            max_seen_event_ids: 512,
            repeat_threshold: 3,
            max_premature_stop_nudges: 1,
            max_loop_nudges: 1,
            max_total_interventions: 2,
            max_lifetime_nudges: 6,
            grace_period_ms: 120_000,
        }
    }
}
