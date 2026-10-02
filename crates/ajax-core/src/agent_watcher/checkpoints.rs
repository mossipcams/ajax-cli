//! Signature-loop helpers and their regression tests.
//!
//! A loop is a *consecutive* run of identical signatures at the tail of the
//! recent-signature ring, not any repetition anywhere in the window: ordinary
//! edit/test cycles repeat the same command many times without ever looping.

use crate::agent_watcher::state::WatcherState;

impl WatcherState {
    /// The trailing run of identical signatures: `(signature, length)`, or
    /// `(None, 0)` when the ring is empty. A different signature in between
    /// breaks the run.
    pub(super) fn trailing_signature_run(&self) -> (Option<&str>, u32) {
        let last = match self.recent_signatures.back() {
            Some(last) => last,
            None => return (None, 0),
        };
        let run = self
            .recent_signatures
            .iter()
            .rev()
            .take_while(|sig| sig.as_str() == last.as_str())
            .count() as u32;
        (Some(last), run)
    }

    /// True when the trailing run of identical signatures reached `threshold`.
    pub(super) fn trailing_run_reaches(&self, threshold: u32) -> bool {
        self.trailing_signature_run().1 >= threshold
    }
}

#[cfg(test)]
mod tests {
    use crate::agent_watcher::policy::{step, Step};
    use crate::agent_watcher::state::WatcherState;
    use crate::agent_watcher::test_support::*;
    use crate::agent_watcher::types::{PendingCheckpoint, WatcherDecision};

    #[test]
    fn alternating_edit_test_cycles_do_not_loop() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        // `bash:test` repeats four times, but never three times in a row:
        // an ordinary edit/test cycle must not raise a loop checkpoint.
        let cycle = [
            ("t1", "bash:test"),
            ("e1", "edit:a"),
            ("t2", "bash:test"),
            ("e2", "edit:b"),
            ("t3", "bash:test"),
            ("e3", "edit:c"),
            ("t4", "bash:test"),
        ];
        for (i, (id, sig)) in cycle.iter().enumerate() {
            let at = 1100 + i as u64 * 100;
            let finished = activity_finished(&mut ids, at, id, sig);
            assert!(
                matches!(
                    step(&mut s, &finished, at, &c),
                    Step::Decision(WatcherDecision::NoAction)
                ),
                "cycle step {i} must not raise a loop checkpoint"
            );
        }
        assert_eq!(s.pending_checkpoint, None);
    }

    #[test]
    fn three_consecutive_identical_signatures_loop_once() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        for i in 0..3u64 {
            let at = 1100 + i * 100;
            let finished = activity_finished(&mut ids, at, &format!("a{i}"), "bash:test");
            let stepped = step(&mut s, &finished, at, &c);
            if i < 2 {
                assert!(matches!(stepped, Step::Decision(WatcherDecision::NoAction)));
            } else {
                assert!(matches!(stepped, Step::NeedsJudge(_)));
                assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Loop));
            }
        }
    }

    #[test]
    fn three_consecutive_failing_signatures_loop() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        for i in 0..3u64 {
            let at = 1100 + i * 100;
            let finished = activity_finished_with_result(
                &mut ids,
                at,
                &format!("f{i}"),
                "bash:test",
                Some(false),
            );
            let stepped = step(&mut s, &finished, at, &c);
            if i < 2 {
                assert!(matches!(stepped, Step::Decision(WatcherDecision::NoAction)));
            } else {
                assert!(matches!(stepped, Step::NeedsJudge(_)));
                assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Loop));
            }
        }
    }

    #[test]
    fn turn_with_activity_stops_cleanly() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let finished = activity_finished(&mut ids, 1100, "a", "work");
        assert!(matches!(
            step(&mut s, &finished, 1100, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        let settle = step(&mut s, &settled_completed(&mut ids, 1200), 1200, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::AllowStop)));
    }

    #[test]
    fn second_turn_without_activity_is_suspicious() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        // Turn 1 does real work and stops cleanly.
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let finished = activity_finished(&mut ids, 1100, "a", "work");
        assert!(matches!(
            step(&mut s, &finished, 1100, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        let first = step(&mut s, &settled_completed(&mut ids, 1200), 1200, &c);
        assert!(matches!(first, Step::Decision(WatcherDecision::AllowStop)));

        // Turn 2 starts and completes with zero tool activity: the activity
        // from turn 1 must not license this stop.
        step(&mut s, &turn_started(&mut ids, 2000), 2000, &c);
        let second = step(&mut s, &settled_completed(&mut ids, 2100), 2100, &c);
        assert!(matches!(second, Step::NeedsJudge(_)));
        assert_eq!(s.pending_checkpoint, Some(PendingCheckpoint::Settle));
    }

    #[test]
    fn pi_harness_stops_cleanly_without_activity() {
        let mut s = WatcherState::new(&frame(), "task-1", "run-1", "pi");
        let c = config();
        let mut ids = Ids::new();
        // Turn 1 does real work and stops cleanly.
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let finished = activity_finished(&mut ids, 1100, "a", "work");
        assert!(matches!(
            step(&mut s, &finished, 1100, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        let first = step(&mut s, &settled_completed(&mut ids, 1200), 1200, &c);
        assert!(matches!(first, Step::Decision(WatcherDecision::AllowStop)));

        // Turn 2 has no activity: the pi harness reports activity itself, so
        // the stop stays clean.
        step(&mut s, &turn_started(&mut ids, 2000), 2000, &c);
        let second = step(&mut s, &settled_completed(&mut ids, 2100), 2100, &c);
        assert!(matches!(second, Step::Decision(WatcherDecision::AllowStop)));
    }

    #[test]
    fn session_opened_does_not_reset_turn_activity() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let finished = activity_finished(&mut ids, 1100, "a", "work");
        assert!(matches!(
            step(&mut s, &finished, 1100, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        // A neutral session event between activity and stop must not clear
        // the current turn's activity.
        let opened = session_opened(&mut ids, 1150);
        assert!(matches!(
            step(&mut s, &opened, 1150, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        let settle = step(&mut s, &settled_completed(&mut ids, 1200), 1200, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::AllowStop)));
    }
}
