//! Pure agent-watcher core: bounded state, deterministic policy, and the
//! judge seam.
//!
//! The watcher supervises an agent run from canonical events. It is a
//! separate opinion layer: it tracks its own [`WatcherPhase`] and its own
//! bounded bookkeeping, and it never reads or writes task status,
//! lifecycle, or registry truth. Hosts (watcher runtime) own event
//! transport and judge execution; everything here is pure and testable.

mod checkpoints;
mod policy;
mod state;
mod store;
mod types;

#[cfg(test)]
mod test_support;

pub use policy::{apply_verdict, nudge_prompt, step, Step, WatcherConfig};
pub use state::{WatcherPersistedState, WatcherState};
pub use store::{
    cancel_pending_watcher_nudge, enqueue_watcher_nudge, load_store, load_watcher_state,
    pending_watcher_nudge, record_watcher_delivery, record_watcher_delivery_at,
    store_watcher_state, WatcherPendingNudge, WatcherStore, DELIVERY_RETRY_INTERVAL_MILLIS,
    MAX_DELIVERY_ATTEMPTS, WATCHER_STATE_KEY,
};
pub use types::{
    AgentProgressJudge, JudgeError, PendingCheckpoint, ProgressState, TaskFrame, WatcherDecision,
    WatcherEvent, WatcherEventDetail, WatcherEventKind, WatcherPhase, WatcherReason,
    WatcherSnapshot, WatcherVerdict,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_watcher::test_support::*;
    use crate::canonical_agent_event::AttentionReason;

    #[test]
    fn normal_progress_never_intervenes() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        let step1 = step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        assert!(matches!(step1, Step::Decision(WatcherDecision::NoAction)));

        let started = activity_started(&mut ids, 1100, "t1");
        assert!(matches!(
            step(&mut s, &started, 1100, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));

        let finished = activity_finished(&mut ids, 1200, "t1", "bash");
        assert!(matches!(
            step(&mut s, &finished, 1200, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));

        let settle = step(&mut s, &settled_completed(&mut ids, 1300), 1300, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::AllowStop)));
        assert_eq!(s.intervention_count, 0);
        assert_eq!(s.phase, WatcherPhase::Healthy);
    }

    #[test]
    fn permission_attention_is_never_overridden() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        let ask = step(
            &mut s,
            &attention(&mut ids, 1000, AttentionReason::Permission),
            1000,
            &c,
        );
        assert!(matches!(
            ask,
            Step::Decision(WatcherDecision::NeedsUser {
                reason: WatcherReason::NeedsUser
            })
        ));
        assert_eq!(s.phase, WatcherPhase::WaitingOnUser);

        // A completion while waiting on permission is not a stop to nudge.
        let settle = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::NoAction)));
        assert_eq!(s.intervention_count, 0);

        // Question behaves the same.
        let ask = step(
            &mut s,
            &attention(&mut ids, 3000, AttentionReason::Question),
            3000,
            &c,
        );
        assert!(matches!(
            ask,
            Step::Decision(WatcherDecision::NeedsUser {
                reason: WatcherReason::NeedsUser
            })
        ));
    }

    #[test]
    fn first_suspicious_completion_asks_judge_then_nudges_once() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Settle));
        assert_eq!(s.intervention_count, 0);

        // Judge says not done: exactly one premature-stop nudge.
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(
            nudge,
            WatcherDecision::Nudge {
                reason: WatcherReason::PrematureStop
            }
        ));
        assert_eq!(s.intervention_count, 1);
        assert_eq!(s.premature_stop_nudges, 1);
        assert_eq!(s.phase, WatcherPhase::Recovering);

        // Second stop after the grace window with no activity: escalate,
        // never a second premature-stop nudge.
        let second = step(&mut s, &settled_completed(&mut ids, 200_000), 200_000, &c);
        assert!(matches!(
            second,
            Step::Decision(WatcherDecision::Escalate {
                reason: WatcherReason::StalledAfterNudge
            })
        ));
        assert_eq!(s.intervention_count, 1);
        assert_eq!(s.phase, WatcherPhase::Escalated);
    }

    #[test]
    fn judge_error_at_settle_checkpoint_fails_open() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));

        // Judge error: no action, zero interventions, checkpoint cleared.
        let failed = apply_verdict(&mut s, Err(JudgeError::Timeout), 3000, &c);
        assert!(matches!(failed, WatcherDecision::NoAction));
        assert_eq!(s.intervention_count, 0);
        assert_eq!(s.premature_stop_nudges, 0);
        assert_eq!(s.pending_checkpoint, None);
        assert_eq!(s.phase, WatcherPhase::Healthy);

        // A later suspicious completion still asks the judge, and a
        // done verdict allows the stop without any intervention.
        let settle = step(&mut s, &settled_completed(&mut ids, 4000), 4000, &c);
        assert!(matches!(settle, Step::NeedsJudge(_)));
        let done = apply_verdict(
            &mut s,
            Ok(verdict(ProgressState::ProbablyDone, 0.9)),
            5000,
            &c,
        );
        assert!(matches!(done, WatcherDecision::AllowStop));
        assert_eq!(s.intervention_count, 0);
    }

    #[test]
    fn repeated_identical_signature_is_detected() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(
            &mut s,
            &activity_finished(&mut ids, 1100, "a", "bash:ls"),
            1100,
            &c,
        );
        let second = step(
            &mut s,
            &activity_finished(&mut ids, 1200, "a", "bash:ls"),
            1200,
            &c,
        );
        assert!(matches!(first, Step::Decision(WatcherDecision::NoAction)));
        assert!(matches!(second, Step::Decision(WatcherDecision::NoAction)));

        let third = step(
            &mut s,
            &activity_finished(&mut ids, 1300, "a", "bash:ls"),
            1300,
            &c,
        );
        assert!(matches!(third, Step::NeedsJudge(_)));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Loop));
    }

    #[test]
    fn uncertain_and_judge_errors_fail_open() {
        let mut s = state();
        let c = config();

        let uncertain = apply_verdict(
            &mut s,
            Ok(verdict(ProgressState::Uncertain, 0.5)),
            10_000,
            &c,
        );
        assert!(matches!(uncertain, WatcherDecision::NoAction));

        let timeout = apply_verdict(&mut s, Err(JudgeError::Timeout), 10_000, &c);
        assert!(matches!(timeout, WatcherDecision::NoAction));

        let unavailable = apply_verdict(&mut s, Err(JudgeError::Unavailable), 10_000, &c);
        assert!(matches!(unavailable, WatcherDecision::NoAction));

        let malformed = apply_verdict(&mut s, Err(JudgeError::Malformed), 10_000, &c);
        assert!(matches!(malformed, WatcherDecision::NoAction));
        assert_eq!(s.intervention_count, 0);
    }

    #[test]
    fn verdicts_map_to_decisions() {
        let mut s = state();
        let c = config();

        let done = apply_verdict(
            &mut s,
            Ok(verdict(ProgressState::ProbablyDone, 0.9)),
            10_000,
            &c,
        );
        assert!(matches!(done, WatcherDecision::AllowStop));

        let stuck = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 20_000, &c);
        assert!(matches!(
            stuck,
            WatcherDecision::Nudge {
                reason: WatcherReason::Stuck
            }
        ));
        assert_eq!(s.loop_nudges, 1);
        assert_eq!(s.last_verdict.unwrap().state, ProgressState::Stuck);
    }

    #[test]
    fn intervention_caps_prevent_infinite_continuation() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        // Premature-stop nudge (1 of 1): suspicious completion, judge
        // says not done.
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(
            nudge,
            WatcherDecision::Nudge {
                reason: WatcherReason::PrematureStop
            }
        ));

        // Judge nudge (1 of 1 loop nudge, 2 of 2 total).
        let second = apply_verdict(
            &mut s,
            Ok(verdict(ProgressState::OffTrack, 0.7)),
            130_000,
            &c,
        );
        assert!(matches!(
            second,
            WatcherDecision::Nudge {
                reason: WatcherReason::OffTrack
            }
        ));
        assert_eq!(s.intervention_count, 2);

        // Budget exhausted: further suspicion escalates, never nudges.
        let third = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.9)), 250_000, &c);
        assert!(matches!(
            third,
            WatcherDecision::Escalate {
                reason: WatcherReason::InterventionCap
            }
        ));
        assert_eq!(s.phase, WatcherPhase::Escalated);
        assert_eq!(s.intervention_count, 2);

        // And an escalated run stops quietly.
        let settle = step(&mut s, &settled_completed(&mut ids, 300_000), 300_000, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::NoAction)));
    }

    #[test]
    fn open_children_mean_not_a_completion() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let child = child_started(&mut ids, 1100);
        step(&mut s, &child, 1100, &c);
        assert_eq!(s.open_children, 1);

        let activity = activity_finished(&mut ids, 1200, "t1", "bash");
        step(&mut s, &activity, 1200, &c);

        // Completed while a child is open: not a stop, no intervention.
        let settle = step(&mut s, &settled_completed(&mut ids, 1300), 1300, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::NoAction)));

        let child_settled = child_settled(&mut ids, 1400);
        step(&mut s, &child_settled, 1400, &c);

        // Now the completion is real and there was progress.
        let settle = step(&mut s, &settled_completed(&mut ids, 1500), 1500, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::AllowStop)));
    }

    #[test]
    fn grace_window_suppresses_stop() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));

        // A stop inside the grace window is left alone.
        let early = step(&mut s, &settled_completed(&mut ids, 10_000), 10_000, &c);
        assert!(matches!(early, Step::Decision(WatcherDecision::NoAction)));
        let late = step(&mut s, &heartbeat(&mut ids, 130_000), 130_000, &c);
        assert!(matches!(
            late,
            Step::Decision(WatcherDecision::Escalate {
                reason: WatcherReason::StalledAfterNudge
            })
        ));
        assert_eq!(s.phase, WatcherPhase::Escalated);
    }

    #[test]
    fn grace_expiry_without_a_stop_asks_judge() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));
        // Grace expiry with no activity is a judge checkpoint.
        let late = step(&mut s, &heartbeat(&mut ids, 130_000), 130_000, &c);
        assert!(matches!(late, Step::NeedsJudge(_)));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::GraceExpiry));
        // Not-done at the expiry checkpoint nudges once with GraceExpired.
        let expiry = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 131_000, &c);
        assert!(matches!(
            expiry,
            WatcherDecision::Nudge {
                reason: WatcherReason::GraceExpired
            }
        ));
        assert_eq!(s.intervention_count, 2);
        // Budget now exhausted: a later expiry escalates, never nudges.
        step(&mut s, &heartbeat(&mut ids, 260_000), 260_000, &c);
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::GraceExpiry));
        let capped = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 261_000, &c);
        assert!(matches!(
            capped,
            WatcherDecision::Escalate {
                reason: WatcherReason::InterventionCap
            }
        ));
    }

    #[test]
    fn repeated_failing_activity_reaches_loop_checkpoint() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        for i in 0..2 {
            let failing = activity_finished_with_result(
                &mut ids,
                1100 + i as u64,
                "a",
                "failing-call",
                Some(false),
            );
            assert!(matches!(
                step(&mut s, &failing, 1100 + i as u64, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }

        // The third identical failing signature is a loop checkpoint, even
        // though every one of them failed.
        let failing =
            activity_finished_with_result(&mut ids, 1300, "a", "failing-call", Some(false));
        assert!(matches!(
            step(&mut s, &failing, 1300, &c),
            Step::NeedsJudge(_)
        ));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Loop));
        // Failures never count as meaningful progress.
        assert!(s.last_meaningful_activity_ms.is_none());
    }

    #[test]
    fn failing_activity_is_never_meaningful_progress() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        s.phase = WatcherPhase::Recovering;
        s.grace_deadline_ms = Some(9_000);

        // A fresh failing activity must not clear recovery pressure.
        let failing = activity_finished_with_result(&mut ids, 1000, "a", "fresh-sig", Some(false));
        assert!(matches!(
            step(&mut s, &failing, 1000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Recovering);
        assert_eq!(s.grace_deadline_ms, Some(9_000));

        // A repeated failing loop checkpoints but still never clears it.
        for i in 0..2 {
            let failing = activity_finished_with_result(
                &mut ids,
                2000 + i as u64,
                "b",
                "failing-call",
                Some(false),
            );
            assert!(matches!(
                step(&mut s, &failing, 2000 + i as u64, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }
        let failing =
            activity_finished_with_result(&mut ids, 2200, "b", "failing-call", Some(false));
        assert!(matches!(
            step(&mut s, &failing, 2200, &c),
            Step::NeedsJudge(_)
        ));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Loop));
        assert_eq!(s.phase, WatcherPhase::Recovering);
        assert_eq!(s.grace_deadline_ms, Some(9_000));
    }

    #[test]
    fn meaningful_activity_after_nudge_resets_intervention_budgets() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        // First episode: a loop gets one nudge.
        for i in 0..2 {
            let looping = activity_finished(&mut ids, 1100 + i as u64, "a", "loop-sig");
            assert!(matches!(
                step(&mut s, &looping, 1100 + i as u64, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }
        let looping = activity_finished(&mut ids, 1300, "a", "loop-sig");
        assert!(matches!(
            step(&mut s, &looping, 1300, &c),
            Step::NeedsJudge(_)
        ));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 1300, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));
        assert_eq!(s.loop_nudges, 1);
        assert_eq!(s.intervention_count, 1);
        assert_eq!(s.phase, WatcherPhase::Recovering);

        // Meaningful non-repeating activity ends the episode and resets the
        // per-episode budgets.
        let progress = activity_finished(&mut ids, 2000, "b", "fresh-work");
        assert!(matches!(
            step(&mut s, &progress, 2000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.grace_deadline_ms, None);
        assert_eq!(s.loop_nudges, 0);
        assert_eq!(s.premature_stop_nudges, 0);
        assert_eq!(s.intervention_count, 0);

        // A later loop gets a fresh nudge, not an immediate escalation.
        for i in 0..2 {
            let looping = activity_finished(&mut ids, 3100 + i as u64, "c", "second-loop");
            assert!(matches!(
                step(&mut s, &looping, 3100 + i as u64, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }
        let looping = activity_finished(&mut ids, 3300, "c", "second-loop");
        assert!(matches!(
            step(&mut s, &looping, 3300, &c),
            Step::NeedsJudge(_)
        ));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3300, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));
        assert_eq!(s.intervention_count, 1);
    }

    #[test]
    fn escalated_phase_clears_on_fresh_turn() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        s.phase = WatcherPhase::Escalated;
        s.intervention_count = 2;
        s.loop_nudges = 1;
        s.premature_stop_nudges = 1;

        // A fresh user turn after escalation starts a fresh episode.
        let turn = turn_started(&mut ids, 1000);
        assert!(matches!(
            step(&mut s, &turn, 1000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.intervention_count, 0);
        assert_eq!(s.loop_nudges, 0);
        assert_eq!(s.premature_stop_nudges, 0);
    }

    #[test]
    fn escalated_phase_holds_while_grace_is_active() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        s.phase = WatcherPhase::Escalated;
        s.intervention_count = 2;
        s.grace_deadline_ms = Some(9_000);

        // Inside the intervention grace window the escalation holds.
        let turn = turn_started(&mut ids, 1000);
        assert!(matches!(
            step(&mut s, &turn, 1000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Escalated);
        assert_eq!(s.intervention_count, 2);

        // After the window, the next turn starts a fresh episode.
        let turn = turn_started(&mut ids, 10_000);
        assert!(matches!(
            step(&mut s, &turn, 10_000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.intervention_count, 0);
    }

    /// Guard: the watcher is a separate opinion layer. It must not import
    /// task models, task status, or lifecycle truth, so it can never become
    /// an alternate registry or status writer.
    #[test]
    fn watcher_module_has_no_task_status_dependency() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = [
            "agent_watcher.rs",
            "agent_watcher/types.rs",
            "agent_watcher/state.rs",
            "agent_watcher/policy.rs",
        ];
        // Tokens are built indirectly so this test's own source does not
        // trip the guard.
        let forbidden = [
            ["crate::", "models"].concat(),
            ["Task", "Status"].concat(),
            ["Lifecycle", "Status"].concat(),
            ["crate::", "lifecycle"].concat(),
        ];
        for file in files {
            let path = src_dir.join(file);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            for token in &forbidden {
                assert!(
                    !text.contains(token.as_str()),
                    "{file} must not depend on task status/lifecycle; found `{token}`"
                );
            }
        }
    }
}
