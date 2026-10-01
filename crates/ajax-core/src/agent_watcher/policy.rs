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
    state.expire_open_tools(now_ms);
    state.event_index += 1;
    if event.kind != WatcherEventKind::Heartbeat {
        state.push_event_label(event.kind.label(), config.max_recent_events);
    }

    match event.kind {
        WatcherEventKind::SessionClosed => {
            state.open_tools.clear();
            state.open_tool_started_at_ms.clear();
            state.open_children = 0;
            Step::Decision(WatcherDecision::NoAction)
        }
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
            state.clear_attention();
            // A new turn makes a pending judge verdict stale.
            state.pending_checkpoint = None;
            if state.phase == WatcherPhase::Escalated && !state.grace_is_active(now_ms) {
                // A fresh user turn after escalation starts a fresh episode:
                // the intervention budgets no longer carry over.
                state.phase = WatcherPhase::Healthy;
                state.reset_intervention_budgets();
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::AttentionCleared => {
            state.clear_attention();
            state.pending_checkpoint = None;
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::Attention => {
            if let WatcherEventDetail::Attention { attention } = &event.detail {
                state.pending_attention = Some(attention.clone());
                state.phase = WatcherPhase::WaitingOnUser;
                state.pending_checkpoint = None;
                // Permission/question is never overridden by a nudge.
                return Step::Decision(WatcherDecision::NeedsUser {
                    reason: WatcherReason::NeedsUser,
                });
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ActivityStarted => {
            state.clear_attention();
            if let WatcherEventDetail::Activity {
                activity_id: Some(id),
                ..
            } = &event.detail
            {
                state.open_tool(id, now_ms);
            }
            Step::Decision(WatcherDecision::NoAction)
        }
        WatcherEventKind::ActivityFinished => {
            state.clear_attention();
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
            if let Some(sig) = signature {
                if state.recent_signatures.back() != Some(&sig) {
                    state.loop_checkpoint_signature = None;
                }
                state.push_signature(&sig, config.max_recent_signatures);
                if state.recent_signature_hits(&sig) >= config.repeat_threshold {
                    // Repeated identical signature: loop checkpoint. A loop
                    // is never meaningful activity, so a nudged loop that
                    // keeps repeating cannot clear recovery pressure.
                    if state.loop_checkpoint_signature.as_ref() != Some(&sig) {
                        state.loop_checkpoint_signature = Some(sig);
                        state.pending_checkpoint = Some(PendingCheckpoint::Loop);
                        return Step::NeedsJudge(state.snapshot(now_ms));
                    }
                    return Step::Decision(WatcherDecision::NoAction);
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
            let outcome = match &event.detail {
                WatcherEventDetail::TurnSettled { outcome } => outcome.clone(),
                _ => TurnOutcome::Unknown,
            };
            if !matches!(outcome, TurnOutcome::Completed) {
                return Step::Decision(WatcherDecision::NoAction);
            }
            step_on_completed_settle(state, now_ms, config)
        }
    }
}

fn step_on_completed_settle(state: &mut WatcherState, now_ms: u64, config: &WatcherConfig) -> Step {
    // Waiting on the user is not a stop the watcher may override.
    if state.pending_attention.is_some() || state.phase == WatcherPhase::WaitingOnUser {
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
        state.escalate();
        return Step::Decision(WatcherDecision::Escalate {
            reason: WatcherReason::StalledAfterNudge,
        });
    }
    // First stop with no meaningful activity: suspicious completion. With
    // the budget exhausted there is nothing a nudge could do.
    if state.intervention_count >= config.max_total_interventions
        || state.premature_stop_nudges >= config.max_premature_stop_nudges
    {
        state.escalate();
        return Step::Decision(WatcherDecision::Escalate {
            reason: WatcherReason::InterventionCap,
        });
    }
    // Harnesses whose events carry no tool-activity evidence cannot show a
    // premature stop: without a repeated-signature loop above, a completed
    // stop is not suspicious by itself, so the judge never runs.
    if !harness_reports_activity(&state.harness) {
        return Step::Decision(WatcherDecision::AllowStop);
    }
    // Ask the judge first; nudge only if it says the work is not done.
    state.pending_checkpoint = Some(PendingCheckpoint::Settle);
    Step::NeedsJudge(state.snapshot(now_ms))
}

/// True when the harness's hook events carry tool-activity evidence the
/// policy can use to spot a premature stop. `pi` only emits turn-level
/// events (`before_agent_start`/`agent_settled`), so its activity rings stay
/// empty and a first completed stop is not suspicious by itself.
pub fn harness_reports_activity(harness: &str) -> bool {
    harness != "pi"
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
        || matches!(
            state.phase,
            WatcherPhase::Escalated | WatcherPhase::WaitingOnUser
        )
        || state.grace_is_active(now_ms)
    {
        state.pending_checkpoint = None;
        return WatcherDecision::NoAction;
    }
    let checkpoint = state.pending_checkpoint.take();
    // Fail-open grace outcomes must wait another window before retrying.
    if checkpoint == Some(PendingCheckpoint::GraceExpiry) {
        state.grace_deadline_ms = Some(now_ms.saturating_add(config.grace_period_ms));
    }
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
            if checkpoint != Some(PendingCheckpoint::GraceExpiry) {
                state.grace_deadline_ms = None;
            }
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
        state.escalate();
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
        state.escalate();
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
        state.escalate();
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

        WatcherReason::Stuck => {
            "You appear stuck. Try a different approach or surface the exact blocker."
        }
        WatcherReason::OffTrack => {
            "You appear to be drifting from the objective. Re-read the objective and realign."
        }
        WatcherReason::GraceExpired => {
            "No progress since the last nudge. Continue the objective or state the blocker."
        }
        // These are operator decisions, never agent nudges.
        WatcherReason::StalledAfterNudge | WatcherReason::NeedsUser | WatcherReason::InterventionCap => "",
    }
}

#[cfg(test)]
mod journal_regression_tests {
    use crate::agent_watcher::{
        nudge_prompt, step, test_support::*, Step, WatcherEventKind, WatcherReason,
    };

    #[test]
    fn session_close_clears_open_work_without_resetting_caps() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        step(&mut s, &activity_started(&mut ids, 1, "orphan"), 1, &c);
        step(&mut s, &child_started(&mut ids, 2), 2, &c);
        s.record_intervention(2);
        let mut closed = heartbeat(&mut ids, 3);
        closed.kind = WatcherEventKind::SessionClosed;
        step(&mut s, &closed, 3, &c);
        assert!(s.open_tools.is_empty());
        assert!(s.open_tool_started_at_ms.is_empty());
        assert_eq!(s.open_children, 0);
        assert_eq!(s.intervention_count, 1);
    }

    #[test]
    fn tool_timestamps_are_bounded_and_removed_on_finish() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        for index in 0..300 {
            step(
                &mut s,
                &activity_started(&mut ids, 1, &index.to_string()),
                1,
                &c,
            );
        }
        assert_eq!(s.open_tools.len(), 256);
        assert_eq!(s.open_tool_started_at_ms.len(), 256);
        step(
            &mut s,
            &activity_finished(&mut ids, 2, "299", "edit"),
            2,
            &c,
        );
        assert_eq!(s.open_tool_started_at_ms.len(), 255);
        assert!(!s.open_tool_started_at_ms.contains_key("299"));
    }

    #[test]
    fn orphaned_tool_expires_only_after_ten_minutes() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        step(&mut s, &activity_started(&mut ids, 10, "orphan"), 10, &c);
        step(&mut s, &heartbeat(&mut ids, 600_010), 600_010, &c);
        assert_eq!(s.open_tools, ["orphan"]);
        step(
            &mut s,
            &activity_started(&mut ids, 600_010, "new"),
            600_010,
            &c,
        );
        step(&mut s, &heartbeat(&mut ids, 600_011), 600_011, &c);
        assert_eq!(s.open_tools, ["new"]);
        step(&mut s, &heartbeat(&mut ids, 1), 1, &c);
        assert_eq!(s.open_tools, ["new"]);
        assert!(matches!(
            step(
                &mut s,
                &settled_completed(&mut ids, 1_200_011),
                1_200_011,
                &c
            ),
            Step::NeedsJudge(_)
        ));
        assert!(s.open_tools.is_empty());
    }
    #[test]
    fn nudge_prompts_are_deterministic_templates() {
        for reason in [
            WatcherReason::PrematureStop,
            WatcherReason::Stuck,
            WatcherReason::OffTrack,
            WatcherReason::GraceExpired,
        ] {
            let prompt = nudge_prompt(&reason);
            assert!(!prompt.is_empty());
            assert_eq!(prompt, nudge_prompt(&reason));
        }
    }
}
