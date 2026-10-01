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
                state.open_tool(id);
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
            if success == Some(false) {
                state.failure_count += 1;
            }
            if let Some(sig) = signature {
                if state.recent_signatures.back() != Some(&sig) {
                    state.loop_checkpoint_signature = None;
                }
                state.push_signature(&sig, config.max_recent_signatures);
                state.bump_repeat(&sig, config.max_repeat_signatures);
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

#[cfg(test)]
mod regression_tests {
    use super::*;
    use crate::agent_watcher::test_support::*;
    use crate::canonical_agent_event::AttentionReason;

    #[test]
    fn loop_checkpoint_fires_once_and_rearms_after_change_or_nudge() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        let mut checkpoints = 0;
        for signature in ["repeat"; 40].into_iter().chain(["different"; 3]) {
            let event = activity_finished(&mut ids, 1, "tool", signature);
            if matches!(step(&mut s, &event, 1, &c), Step::NeedsJudge(_)) {
                checkpoints += 1;
                apply_verdict(&mut s, Err(JudgeError::Unavailable), 1, &c);
            }
        }
        assert_eq!(checkpoints, 2);
        s.record_intervention(2);
        assert!(matches!(
            step(
                &mut s,
                &activity_finished(&mut ids, 3, "tool", "different"),
                3,
                &c
            ),
            Step::NeedsJudge(_)
        ));
        assert_eq!(s.phase, WatcherPhase::Recovering);
    }

    #[test]
    fn grace_expiry_no_action_rearms_deadline_for_uncertain_errors_and_progress() {
        for result in [
            Err(JudgeError::Unavailable),
            Err(JudgeError::Timeout),
            Err(JudgeError::Malformed),
            Ok(verdict(ProgressState::Uncertain, 0.1)),
            Ok(verdict(ProgressState::Progressing, 1.0)),
        ] {
            let (mut s, c, mut ids) = (state(), config(), Ids::new());
            s.record_intervention(0);
            s.grace_deadline_ms = Some(1);
            assert!(matches!(
                step(&mut s, &heartbeat(&mut ids, 1), 1, &c),
                Step::NeedsJudge(_)
            ));
            assert_eq!(
                apply_verdict(&mut s, result, 1, &c),
                WatcherDecision::NoAction
            );
            assert_eq!(s.grace_deadline_ms, Some(1 + c.grace_period_ms));
            for now in 2..8 {
                assert!(matches!(
                    step(&mut s, &heartbeat(&mut ids, now), now, &c),
                    Step::Decision(WatcherDecision::NoAction)
                ));
            }
        }
    }

    #[test]
    fn resumed_activity_clears_permission_and_loop_stop_is_judged() {
        for resume in [
            WatcherEventKind::ActivityStarted,
            WatcherEventKind::ActivityFinished,
        ] {
            let (mut s, c, mut ids) = (state(), config(), Ids::new());
            step(
                &mut s,
                &attention(&mut ids, 1, AttentionReason::Permission),
                1,
                &c,
            );
            let mut event = activity_finished(&mut ids, 2, "tool", "repeat");
            event.kind = resume;
            step(&mut s, &event, 2, &c);
            assert!(s.pending_attention.is_none());
            assert_eq!(s.phase, WatcherPhase::Healthy);
            for at in 3..6 {
                step(
                    &mut s,
                    &activity_finished(&mut ids, at, "tool", "repeat"),
                    at,
                    &c,
                );
            }
            assert!(matches!(
                step(&mut s, &settled_completed(&mut ids, 6), 6, &c),
                Step::NeedsJudge(_)
            ));
            assert!(matches!(
                apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 1.0)), 6, &c),
                WatcherDecision::Nudge { .. }
            ));
        }
    }

    #[test]
    fn attention_inside_grace_preserves_recovery_deadline() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 1.0)), 1, &c);
        let deadline = s.grace_deadline_ms;
        step(
            &mut s,
            &attention(&mut ids, 2, AttentionReason::Question),
            2,
            &c,
        );
        assert_eq!(s.grace_deadline_ms, deadline);
        step(&mut s, &activity_started(&mut ids, 3, "tool"), 3, &c);
        assert!(s.pending_attention.is_none());
        assert_eq!(s.phase, WatcherPhase::Recovering);
        assert_eq!(
            apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 1.0)), 4, &c),
            WatcherDecision::NoAction
        );
    }

    #[test]
    fn pi_first_stop_is_allowed_without_judge_and_claude_still_is_judged() {
        let (c, mut ids) = (config(), Ids::new());
        // pi emits only turn-level events: a first completed stop with no
        // activity evidence must not become a Settle checkpoint, so the
        // judge never runs.
        let mut pi = WatcherState::new(&frame(), "task-1", "run-1", "pi");
        assert!(matches!(
            step(&mut pi, &settled_completed(&mut ids, 1), 1, &c),
            Step::Decision(WatcherDecision::AllowStop)
        ));
        assert!(pi.pending_checkpoint.is_none());
        // claude reports activity, so the same first stop is still judged.
        let mut claude = WatcherState::new(&frame(), "task-2", "run-2", "claude");
        assert!(matches!(
            step(&mut claude, &settled_completed(&mut ids, 2), 2, &c),
            Step::NeedsJudge(_)
        ));
        assert_eq!(claude.pending_checkpoint, Some(PendingCheckpoint::Settle));
    }

    #[test]
    fn every_escalation_clears_recovery_deadline() {
        for checkpoint in [
            None,
            Some(PendingCheckpoint::Settle),
            Some(PendingCheckpoint::Loop),
            Some(PendingCheckpoint::GraceExpiry),
        ] {
            let (mut s, c) = (state(), config());
            s.intervention_count = c.max_total_interventions;
            s.grace_deadline_ms = Some(1);
            s.pending_checkpoint = checkpoint;
            assert!(matches!(
                apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 1.0)), 2, &c),
                WatcherDecision::Escalate { .. }
            ));
            assert_eq!(s.grace_deadline_ms, None);
        }
        for prior_intervention in [None, Some(1)] {
            let (mut s, c, mut ids) = (state(), config(), Ids::new());
            s.intervention_count = c.max_total_interventions;
            s.last_intervention_at_ms = prior_intervention;
            s.grace_deadline_ms = Some(1);
            assert!(matches!(
                step(&mut s, &settled_completed(&mut ids, 2), 2, &c),
                Step::Decision(WatcherDecision::Escalate { .. })
            ));
            assert_eq!(s.grace_deadline_ms, None);
        }
    }
}
