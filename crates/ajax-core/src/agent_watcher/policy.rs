//! Deterministic watcher policy.
//!
//! [`step`] is pure: it folds one event into bounded state and returns
//! either a decision or a request to run the judge at a checkpoint. The
//! judge runs only at checkpoints (suspicious completion, repeated
//! signature, grace expiry) — never on every event. Judge failures fail
//! open via [`apply_verdict`].

use crate::agent_watcher::state::WatcherState;
use crate::agent_watcher::types::{
    JudgeError, PendingCheckpoint, ProgressState, WatcherDecision, WatcherEvent,
    WatcherEventDetail, WatcherEventKind, WatcherPhase, WatcherReason, WatcherSnapshot,
    WatcherVerdict,
};
use crate::canonical_agent_event::TurnOutcome;

/// Caps and windows for the watcher. Intervention budgets are per episode:
/// meaningful non-repeating activity after a nudge, or a fresh user turn
/// after escalation, resets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WatcherConfig {
    pub max_recent_events: usize,
    pub max_recent_signatures: usize,
    pub max_seen_event_ids: usize,
    pub max_repeat_signatures: usize,
    /// Identical signatures within the recent window that count as a loop.
    pub repeat_threshold: u32,
    pub max_premature_stop_nudges: u32,
    pub max_loop_nudges: u32,
    pub max_total_interventions: u32,
    /// Window after a nudge in which the agent can still recover.
    pub grace_period_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            max_recent_events: 64,
            max_recent_signatures: 32,
            max_seen_event_ids: 512,
            max_repeat_signatures: 32,
            repeat_threshold: 3,
            max_premature_stop_nudges: 1,
            max_loop_nudges: 1,
            max_total_interventions: 2,
            grace_period_ms: 120_000,
        }
    }
}

/// One policy step's output.
#[derive(Debug)]
pub enum Step {
    Decision(WatcherDecision),
    /// Run the judge on this snapshot; fold the answer with [`apply_verdict`].
    NeedsJudge(WatcherSnapshot),
}

/// Fold one event into the state and produce the step output.
pub fn step(
    state: &mut WatcherState,
    event: &WatcherEvent,
    now_ms: u64,
    config: &WatcherConfig,
) -> Step {
    // Duplicate delivery (JSONL replay): ignore entirely.
    if state.has_seen_event_id(&event.event_id) {
        return Step::Decision(WatcherDecision::NoAction);
    }
    state.note_event_id(&event.event_id, config.max_seen_event_ids);
    state.event_index += 1;
    if event.kind != WatcherEventKind::Heartbeat {
        state.push_event_label(event.kind.label(), config.max_recent_events);
    }

    match event.kind {
        WatcherEventKind::Heartbeat => {
            if state.phase == WatcherPhase::Recovering
                && state.grace_has_expired(now_ms)
                && !state.meaningful_activity_since_intervention()
            {
                state.pending_checkpoint = Some(PendingCheckpoint::GraceExpiry);
                Step::NeedsJudge(state.snapshot(now_ms))
            } else {
                Step::Decision(WatcherDecision::NoAction)
            }
        }
        WatcherEventKind::TurnStarted => {
            state.pending_attention = None;
            // A new turn makes a pending judge verdict stale.
            state.pending_checkpoint = None;
            if state.phase == WatcherPhase::WaitingOnUser {
                state.phase = WatcherPhase::Healthy;
            }
            if state.phase == WatcherPhase::Escalated && !state.grace_is_active(now_ms) {
                // A fresh user turn after escalation starts a fresh episode:
                // the intervention budgets no longer carry over.
                state.phase = WatcherPhase::Healthy;
                state.reset_intervention_budgets();
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::Attention => {
            if let WatcherEventDetail::Attention { attention } = &event.detail {
                state.pending_attention = Some(attention.clone());
                state.phase = WatcherPhase::WaitingOnUser;
                state.grace_deadline_ms = None;
                // Permission/question is never overridden by a nudge.
                return Step::Decision(WatcherDecision::NeedsUser {
                    reason: WatcherReason::NeedsUser,
                });
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ActivityStarted => {
            if let WatcherEventDetail::Activity {
                activity_id: Some(id),
                ..
            } = &event.detail
            {
                state.open_tool(id);
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ActivityFinished => {
            let (activity_id, signature, success) = match &event.detail {
                WatcherEventDetail::Activity {
                    activity_id,
                    signature,
                    success,
                } => (activity_id.clone(), signature.clone(), *success),
                _ => (None, None, None),
            };
            if let Some(id) = activity_id {
                state.close_tool(&id);
            }
            if success == Some(false) {
                state.failure_count += 1;
            }
            if let Some(sig) = signature {
                state.push_signature(&sig, config.max_recent_signatures);
                state.bump_repeat(&sig, config.max_repeat_signatures);
                if state.recent_signature_hits(&sig) >= config.repeat_threshold {
                    // Repeated identical signature: loop checkpoint. A loop
                    // is never meaningful activity, so a nudged loop that
                    // keeps repeating cannot clear recovery pressure.
                    state.pending_checkpoint = Some(PendingCheckpoint::Loop);
                    return Step::NeedsJudge(state.snapshot(now_ms));
                }
            }
            // Only a non-failed activity that is not part of a detected loop
            // is meaningful progress.
            if success != Some(false) {
                state.record_meaningful_activity(event.occurred_at_ms);
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ChildStarted => {
            state.open_children = state.open_children.saturating_add(1);
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ChildSettled => {
            state.open_children = state.open_children.saturating_sub(1);
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::TurnSettled => {
            state.settle_attempts += 1;
            let outcome = match &event.detail {
                WatcherEventDetail::TurnSettled { outcome } => outcome.clone(),
                _ => TurnOutcome::Unknown,
            };
            if matches!(outcome, TurnOutcome::Failed) {
                state.failure_count += 1;
                return Step::Decision(WatcherDecision::NoAction);
            }
            if !matches!(outcome, TurnOutcome::Completed) {
                return Step::Decision(WatcherDecision::NoAction);
            }
            step_on_completed_settle(state, now_ms, config)
        }
    }
}

fn step_on_completed_settle(state: &mut WatcherState, now_ms: u64, config: &WatcherConfig) -> Step {
    // Waiting on the user is not a stop the watcher may override.
    if state.pending_attention.is_some() {
        return Step::Decision(WatcherDecision::NoAction);
    }
    // Children still running: the turn is not actually finished.
    if state.open_children > 0 {
        return Step::Decision(WatcherDecision::NoAction);
    }
    // Inside the post-nudge grace window: give the agent room to recover.
    if state.grace_is_active(now_ms) {
        return Step::Decision(WatcherDecision::NoAction);
    }
    // Already escalated: the run is the user's problem, not the watcher's.
    if state.phase == WatcherPhase::Escalated {
        return Step::Decision(WatcherDecision::NoAction);
    }
    // Suspicious completion: the recent activity is repetitive, so let the
    // judge decide whether the work is actually done.
    if state
        .recent_signatures
        .iter()
        .any(|sig| state.recent_signature_hits(sig) >= config.repeat_threshold)
    {
        state.pending_checkpoint = Some(PendingCheckpoint::Settle);
        return Step::NeedsJudge(state.snapshot(now_ms));
    }
    if state.meaningful_activity_since_intervention() {
        // Normal completion with progress: stop is fine.
        if state.phase == WatcherPhase::Recovering {
            state.phase = WatcherPhase::Healthy;
            state.grace_deadline_ms = None;
        }
        return Step::Decision(WatcherDecision::AllowStop);
    }
    // No meaningful activity since the last intervention.
    if state.last_intervention_at_ms.is_some() {
        // Second stop after a nudge with nothing in between: escalate
        // deterministically. Bounded by construction; no judge involved.
        state.phase = WatcherPhase::Escalated;
        return Step::Decision(WatcherDecision::Escalate {
            reason: WatcherReason::StalledAfterNudge,
        });
    }
    // First stop with no meaningful activity: suspicious completion. With
    // the budget exhausted there is nothing a nudge could do.
    if state.intervention_count >= config.max_total_interventions
        || state.premature_stop_nudges >= config.max_premature_stop_nudges
    {
        state.phase = WatcherPhase::Escalated;
        return Step::Decision(WatcherDecision::Escalate {
            reason: WatcherReason::InterventionCap,
        });
    }
    // Ask the judge first; nudge only if it says the work is not done.
    state.pending_checkpoint = Some(PendingCheckpoint::Settle);
    Step::NeedsJudge(state.snapshot(now_ms))
}

/// Fold a judge verdict (or judge failure) into a decision. Judge failures
/// and uncertainty fail open: no action. The pending checkpoint decides the
/// nudge reason and the budget a nudge counts against.
pub fn apply_verdict(
    state: &mut WatcherState,
    verdict: Result<WatcherVerdict, JudgeError>,
    now_ms: u64,
    config: &WatcherConfig,
) -> WatcherDecision {
    // Never override user-input states or an escalated run.
    if state.pending_attention.is_some()
        || state.phase == WatcherPhase::Escalated
        || state.grace_is_active(now_ms)
    {
        state.pending_checkpoint = None;
        return WatcherDecision::NoAction;
    }
    let checkpoint = state.pending_checkpoint.take();
    let verdict = match verdict {
        Ok(verdict) => verdict,
        Err(_) => return WatcherDecision::NoAction,
    };
    let progress = verdict.state;
    state.last_verdict = Some(verdict);
    match progress {
        ProgressState::Uncertain => WatcherDecision::NoAction,
        ProgressState::ProbablyDone => {
            state.phase = WatcherPhase::Healthy;
            state.grace_deadline_ms = None;
            WatcherDecision::AllowStop
        }
        ProgressState::Progressing => {
            state.phase = WatcherPhase::Healthy;
            state.grace_deadline_ms = None;
            WatcherDecision::NoAction
        }
        ProgressState::NeedsUser => {
            state.phase = WatcherPhase::WaitingOnUser;
            WatcherDecision::NeedsUser {
                reason: WatcherReason::NeedsUser,
            }
        }
        ProgressState::Stuck | ProgressState::OffTrack => {
            let reason = match progress {
                ProgressState::Stuck => WatcherReason::Stuck,
                ProgressState::OffTrack => WatcherReason::OffTrack,
                _ => return WatcherDecision::NoAction,
            };
            match checkpoint {
                // Suspicious completion and the judge says not done: the
                // completion was premature. Counts against the
                // premature-stop budget.
                Some(PendingCheckpoint::Settle) => nudge_premature_stop(state, now_ms, config),
                // Grace after a nudge expired with no activity. Counts
                // against the loop-nudge budget; escalates per caps.
                Some(PendingCheckpoint::GraceExpiry) => nudge_grace_expired(state, now_ms, config),
                // Loop checkpoint, or no checkpoint recorded: judge-style
                // nudge keyed by the verdict.
                Some(PendingCheckpoint::Loop) | None => {
                    nudge_for_verdict(state, reason, now_ms, config)
                }
            }
        }
    }
}

/// A not-done verdict at a settle checkpoint: the completion was premature.
/// Counts against the premature-stop budget; escalates per caps.
fn nudge_premature_stop(
    state: &mut WatcherState,
    now_ms: u64,
    config: &WatcherConfig,
) -> WatcherDecision {
    if state.intervention_count >= config.max_total_interventions
        || state.premature_stop_nudges >= config.max_premature_stop_nudges
    {
        state.phase = WatcherPhase::Escalated;
        return WatcherDecision::Escalate {
            reason: WatcherReason::InterventionCap,
        };
    }
    state.premature_stop_nudges += 1;
    state.record_intervention(now_ms);
    state.grace_deadline_ms = Some(now_ms.saturating_add(config.grace_period_ms));
    WatcherDecision::Nudge {
        reason: WatcherReason::PrematureStop,
    }
}

/// A not-done verdict at a grace-expiry checkpoint: no activity since the
/// last nudge. Counts against the loop-nudge budget; escalates per caps.
fn nudge_grace_expired(
    state: &mut WatcherState,
    now_ms: u64,
    config: &WatcherConfig,
) -> WatcherDecision {
    if state.intervention_count >= config.max_total_interventions
        || state.loop_nudges >= config.max_loop_nudges
    {
        state.phase = WatcherPhase::Escalated;
        return WatcherDecision::Escalate {
            reason: WatcherReason::InterventionCap,
        };
    }
    state.loop_nudges += 1;
    state.record_intervention(now_ms);
    state.grace_deadline_ms = Some(now_ms.saturating_add(config.grace_period_ms));
    WatcherDecision::Nudge {
        reason: WatcherReason::GraceExpired,
    }
}

fn nudge_for_verdict(
    state: &mut WatcherState,
    reason: WatcherReason,
    now_ms: u64,
    config: &WatcherConfig,
) -> WatcherDecision {
    if state.intervention_count >= config.max_total_interventions
        || state.loop_nudges >= config.max_loop_nudges
    {
        state.phase = WatcherPhase::Escalated;
        return WatcherDecision::Escalate {
            reason: WatcherReason::InterventionCap,
        };
    }
    state.loop_nudges += 1;
    state.record_intervention(now_ms);
    state.grace_deadline_ms = Some(now_ms.saturating_add(config.grace_period_ms));
    WatcherDecision::Nudge { reason }
}

/// Short deterministic nudge template per reason. The judge never authors
/// nudge text.
pub fn nudge_prompt(reason: &WatcherReason) -> &'static str {
    match reason {
        WatcherReason::PrematureStop => {
            "The turn settled without observable progress. Continue the objective or state precisely why it is complete."
        }
        WatcherReason::RepeatedSignature => {
            "You are repeating the same action. Change approach or explain why the repetition is required."
        }
        WatcherReason::SuspiciousCompletion => {
            "The completion does not match the objective. Finish the remaining work or point to the concrete evidence that it is done."
        }
        WatcherReason::StalledAfterNudge => {
            "The run stopped again after a nudge with no progress. Escalating to the operator."
        }
        WatcherReason::Stuck => {
            "You appear stuck. Try a different approach or surface the exact blocker."
        }
        WatcherReason::OffTrack => {
            "You appear to be drifting from the objective. Re-read the objective and realign."
        }
        WatcherReason::NeedsUser => {
            "The run needs the operator. Escalating to the user."
        }
        WatcherReason::GraceExpired => {
            "No progress since the last nudge. Continue the objective or state the blocker."
        }
        WatcherReason::InterventionCap => {
            "The watcher has exhausted its intervention budget. Escalating to the operator."
        }
    }
}
