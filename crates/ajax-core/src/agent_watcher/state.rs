//! Bounded watcher state: what the watcher remembers about one run.
//!
//! Every collection is capped so a long run cannot grow memory without
//! bound. The state is the watcher's own bookkeeping only — it never reads
//! or writes task status, lifecycle, or registry truth.

use std::collections::{HashMap, VecDeque};

use crate::agent_watcher::types::{TaskFrame, WatcherPhase, WatcherSnapshot};

/// Hard cap on remembered open tool ids.
const MAX_OPEN_TOOLS: usize = 256;
const MAX_OPEN_CHILDREN: usize = 32;
const OPEN_WORK_EXPIRY_MS: u64 = 600_000;

pub use super::types::{WatcherPersistedState, WatcherState};

impl WatcherState {
    pub fn new(frame: &TaskFrame, task_id: &str, run_id: &str, harness: &str) -> Self {
        Self {
            task_id: task_id.to_string(),
            run_id: run_id.to_string(),
            harness: harness.to_string(),
            objective: frame.objective.clone(),
            phase: WatcherPhase::Healthy,
            recent_events: VecDeque::new(),
            recent_signatures: VecDeque::new(),
            last_meaningful_activity_ms: None,
            pending_attention: None,
            pending_checkpoint: None,
            loop_checkpoint_signature: None,
            open_tools: Vec::new(),
            open_tool_started_at_ms: HashMap::new(),
            open_children: 0,
            child_started_at_ms: VecDeque::new(),
            settled_in_grace: false,
            intervention_count: 0,
            premature_stop_nudges: 0,
            loop_nudges: 0,
            last_intervention_at_ms: None,
            last_verdict: None,
            grace_deadline_ms: None,
            seen_event_ids: VecDeque::new(),
            event_index: 0,
            nudge_seq: 0,
            lifetime_nudges: 0,
        }
    }

    pub fn has_seen_event_id(&self, event_id: &str) -> bool {
        self.seen_event_ids.iter().any(|seen| seen == event_id)
    }

    pub fn note_event_id(&mut self, event_id: &str, cap: usize) {
        if !self.has_seen_event_id(event_id) {
            self.seen_event_ids.push_back(event_id.to_string());
        }
        while self.seen_event_ids.len() > cap {
            self.seen_event_ids.pop_front();
        }
    }

    pub fn push_event_label(&mut self, label: &str, cap: usize) {
        self.recent_events.push_back(label.to_string());
        while self.recent_events.len() > cap {
            self.recent_events.pop_front();
        }
    }

    pub fn push_signature(&mut self, signature: &str, cap: usize) {
        self.recent_signatures.push_back(signature.to_string());
        while self.recent_signatures.len() > cap {
            self.recent_signatures.pop_front();
        }
    }

    /// How many of the recent signatures equal `signature`.
    pub fn recent_signature_hits(&self, signature: &str) -> u32 {
        self.recent_signatures
            .iter()
            .filter(|sig| *sig == signature)
            .count() as u32
    }

    pub fn record_meaningful_activity(&mut self, at_ms: u64) {
        self.last_meaningful_activity_ms = Some(at_ms);
        self.settled_in_grace = false;
        // New activity makes a pending judge verdict stale.
        self.pending_checkpoint = None;
        // Meaningful non-repeating activity after a nudge ends the episode:
        // recovery pressure clears and the intervention budgets reset, so a
        // long task is not nudged to death by an early loop.
        if self.phase == WatcherPhase::Recovering {
            self.phase = WatcherPhase::Healthy;
            self.grace_deadline_ms = None;
            self.reset_intervention_budgets();
        }
    }

    /// Reset the per-episode intervention budgets. `nudge_seq` is monotonic
    /// and never resets: nudge ids built from it stay unique across resets.
    pub fn reset_intervention_budgets(&mut self) {
        self.premature_stop_nudges = 0;
        self.loop_nudges = 0;
        self.intervention_count = 0;
    }

    pub fn open_tool(&mut self, tool_id: &str, now_ms: u64) {
        if !self.open_tools.iter().any(|id| id == tool_id) {
            self.open_tools.push(tool_id.to_string());
            self.open_tool_started_at_ms
                .insert(tool_id.to_string(), now_ms);
        }
        while self.open_tools.len() > MAX_OPEN_TOOLS {
            let id = self.open_tools.remove(0);
            self.open_tool_started_at_ms.remove(&id);
        }
    }

    pub fn close_tool(&mut self, tool_id: &str) {
        self.open_tool_started_at_ms.remove(tool_id);
        if let Some(pos) = self.open_tools.iter().position(|id| id == tool_id) {
            self.open_tools.remove(pos);
        }
    }

    pub(super) fn expire_open_tools(&mut self, now_ms: u64) {
        self.open_tool_started_at_ms
            .retain(|_, at| now_ms.saturating_sub(*at) <= OPEN_WORK_EXPIRY_MS);
        self.open_tools
            .retain(|id| self.open_tool_started_at_ms.contains_key(id));
    }

    pub(super) fn open_child(&mut self, at_ms: u64) {
        self.child_started_at_ms.push_back(at_ms);
        if self.child_started_at_ms.len() > MAX_OPEN_CHILDREN {
            self.child_started_at_ms.pop_front();
        }
        self.open_children = self.child_started_at_ms.len() as u32;
    }

    pub(super) fn expire_open_children(&mut self, at_ms: u64) {
        self.child_started_at_ms
            .retain(|start| at_ms.saturating_sub(*start) <= OPEN_WORK_EXPIRY_MS);
        self.open_children = self.child_started_at_ms.len() as u32;
    }

    /// True when meaningful activity happened after the last intervention.
    pub fn meaningful_activity_since_intervention(&self) -> bool {
        match (
            self.last_meaningful_activity_ms,
            self.last_intervention_at_ms,
        ) {
            (Some(activity), Some(intervention)) => activity > intervention,
            (Some(_), None) => true,
            _ => false,
        }
    }

    pub fn ms_since_meaningful_activity(&self, now_ms: u64) -> Option<u64> {
        self.last_meaningful_activity_ms
            .map(|at| now_ms.saturating_sub(at))
    }

    pub fn grace_is_active(&self, now_ms: u64) -> bool {
        self.grace_deadline_ms
            .is_some_and(|deadline| now_ms < deadline)
    }

    pub fn grace_has_expired(&self, now_ms: u64) -> bool {
        self.grace_deadline_ms
            .is_some_and(|deadline| now_ms >= deadline)
    }

    pub fn record_intervention(&mut self, now_ms: u64) {
        self.settled_in_grace = false;
        self.loop_checkpoint_signature = None;
        self.intervention_count += 1;
        self.nudge_seq += 1;
        self.lifetime_nudges += 1;
        self.last_intervention_at_ms = Some(now_ms);
        self.phase = WatcherPhase::Recovering;
    }

    /// Re-arm the loop checkpoint edge. The edge is only consumed by a
    /// nudge or a successful judged verdict: a fail-open outcome (judge
    /// error, skipped evaluation, discarded verdict) must let the next
    /// repeat raise the checkpoint again.
    pub fn rearm_loop_checkpoint(&mut self) {
        self.loop_checkpoint_signature = None;
        self.pending_checkpoint = None;
    }

    pub(super) fn clear_attention(&mut self) {
        self.pending_attention = None;
        if self.phase == WatcherPhase::WaitingOnUser {
            self.phase = if self.grace_deadline_ms.is_some() && self.intervention_count > 0 {
                WatcherPhase::Recovering
            } else {
                WatcherPhase::Healthy
            };
        }
    }

    pub(super) fn escalate(&mut self) {
        self.phase = WatcherPhase::Escalated;
        self.grace_deadline_ms = None;
        self.pending_checkpoint = None;
    }

    /// Compact, bounded view for a judge.
    pub fn snapshot(&self, now_ms: u64) -> WatcherSnapshot {
        WatcherSnapshot {
            objective: self.objective.clone(),
            task_id: self.task_id.clone(),
            run_id: self.run_id.clone(),
            harness: self.harness.clone(),
            phase: self.phase,
            recent_signatures: self.recent_signatures.iter().cloned().collect(),
            recent_events: self.recent_events.iter().cloned().collect(),
            pending_attention: self.pending_attention.clone(),
            open_children: self.open_children,
            intervention_count: self.intervention_count,
            last_verdict: self.last_verdict.clone(),
            ms_since_meaningful_activity: self.ms_since_meaningful_activity(now_ms),
        }
    }

    /// The persisted subset, for task metadata storage.
    pub fn persisted(&self) -> WatcherPersistedState {
        WatcherPersistedState {
            intervention_count: self.intervention_count,
            premature_stop_nudges: self.premature_stop_nudges,
            loop_nudges: self.loop_nudges,
            last_intervention_at_ms: self.last_intervention_at_ms,
            grace_deadline_ms: self.grace_deadline_ms,
            last_verdict: self.last_verdict.clone(),
            last_seen_event_id: self.seen_event_ids.back().cloned(),
            phase: self.phase,
            nudge_seq: self.nudge_seq,
            lifetime_nudges: self.lifetime_nudges,
        }
    }

    /// Fold a persisted subset back into the state. Rings and transient
    /// counters stay as they are; the caller replays events for those.
    pub fn apply_persisted(&mut self, persisted: &WatcherPersistedState) {
        self.intervention_count = persisted.intervention_count;
        self.premature_stop_nudges = persisted.premature_stop_nudges;
        self.loop_nudges = persisted.loop_nudges;
        self.last_intervention_at_ms = persisted.last_intervention_at_ms;
        self.grace_deadline_ms = persisted.grace_deadline_ms;
        self.last_verdict = persisted.last_verdict.clone();
        self.phase = persisted.phase;
        self.nudge_seq = persisted.nudge_seq;
        self.lifetime_nudges = persisted.lifetime_nudges;
        if let Some(id) = &persisted.last_seen_event_id {
            self.note_event_id(id, 512);
        }
    }
}

#[cfg(test)]
mod regression_tests {
    use crate::agent_watcher::test_support::*;
    use crate::agent_watcher::*;
    use crate::canonical_agent_event::AttentionReason;

    #[test]
    fn failed_loop_verdict_rearms_the_edge_for_the_next_repeat() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        let mut fires = 0;
        for at in 1..=4u64 {
            let event = activity_finished_with_result(&mut ids, at, "t", "sig", Some(false));
            if matches!(step(&mut s, &event, at, &c), Step::NeedsJudge(_)) {
                fires += 1;
                apply_verdict(&mut s, Err(JudgeError::Unavailable), at, &c);
            }
        }
        // The first repeat raised the checkpoint; the failed verdict
        // re-armed the edge, so the next repeat raises it again.
        assert_eq!(fires, 2);
        let event = activity_finished_with_result(&mut ids, 5, "t", "sig", Some(false));
        assert!(matches!(step(&mut s, &event, 5, &c), Step::NeedsJudge(_)));
    }

    #[test]
    fn judged_loop_verdict_keeps_the_edge_consumed() {
        for progress in [ProgressState::Progressing, ProgressState::Uncertain] {
            let (mut s, c, mut ids) = (state(), config(), Ids::new());
            for at in 1..=4u64 {
                let event = activity_finished_with_result(&mut ids, at, "t", "sig", Some(false));
                if matches!(step(&mut s, &event, at, &c), Step::NeedsJudge(_)) {
                    assert_eq!(
                        apply_verdict(&mut s, Ok(verdict(progress, 0.9)), at, &c),
                        WatcherDecision::NoAction
                    );
                }
            }
            // A successful judged verdict consumed the edge: repeats do
            // not re-fire the checkpoint.
            let event = activity_finished_with_result(&mut ids, 5, "t", "sig", Some(false));
            assert!(matches!(
                step(&mut s, &event, 5, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }
    }

    #[test]
    fn alternating_loop_and_fresh_activity_cannot_nudge_past_the_lifetime_cap() {
        let (mut s, c, mut ids) = (state(), config(), Ids::new());
        let mut nudges = 0;
        let mut at = 100u64;
        for cycle in 0..20 {
            // One failing loop repeat: the always-Stuck judge nudges, or
            // the lifetime cap escalates.
            let loop_event = activity_finished_with_result(&mut ids, at, "t", "loop", Some(false));
            at += 100;
            let decision = match step(&mut s, &loop_event, at, &c) {
                Step::NeedsJudge(_) => {
                    apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 1.0)), at, &c)
                }
                Step::Decision(decision) => decision,
            };
            if matches!(decision, WatcherDecision::Nudge { .. }) {
                nudges += 1;
            }
            if matches!(decision, WatcherDecision::Escalate { .. }) {
                break;
            }
            // One fresh successful read refills the per-episode budgets.
            // Its signature varies so the read itself is not a loop.
            let fresh = activity_finished(&mut ids, at, "read", &format!("fresh-read-{cycle}"));
            at += 100;
            assert!(matches!(
                step(&mut s, &fresh, at, &c),
                Step::Decision(WatcherDecision::NoAction)
            ));
        }
        // The episode budgets refill after every fresh read, but the
        // lifetime cap stops the nudging and escalates.
        assert_eq!(nudges, c.max_lifetime_nudges as usize);
        assert_eq!(s.phase, WatcherPhase::Escalated);
        assert_eq!(s.lifetime_nudges, c.max_lifetime_nudges as u64);
    }

    #[test]
    fn lifetime_nudges_survive_budget_resets_and_persist() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        // Two nudges separated by real progress: normal runs are
        // unaffected by the lifetime cap.
        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        assert!(matches!(
            apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c),
            WatcherDecision::Nudge { .. }
        ));
        let activity = activity_finished(&mut ids, 4000, "t2", "edit");
        step(&mut s, &activity, 4000, &c);
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.lifetime_nudges, 1);
        assert_eq!(s.intervention_count, 0);

        // The Escalated -> Healthy fresh-user-turn reset must not reset
        // the lifetime counter.
        s.phase = WatcherPhase::Escalated;
        s.lifetime_nudges = 5;
        step(&mut s, &turn_started(&mut ids, 5000), 5000, &c);
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.lifetime_nudges, 5);

        // The counter persists and survives old metadata without it.
        let persisted = s.persisted();
        assert_eq!(persisted.lifetime_nudges, 5);
        let mut fresh = state();
        fresh.apply_persisted(&persisted);
        assert_eq!(fresh.lifetime_nudges, 5);
        let old: WatcherPersistedState =
            serde_json::from_str(r#"{"intervention_count":1}"#).expect("old metadata");
        assert_eq!(old.lifetime_nudges, 0);
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
    #[test]
    fn state_is_bounded() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        for i in 0..100 {
            let sig = format!("sig-{i}");
            let event = activity_finished(&mut ids, 1000 + i as u64, "t", &sig);
            step(&mut s, &event, 1000 + i as u64, &c);
        }
        assert!(s.recent_signatures.len() <= c.max_recent_signatures);
        assert!(s.recent_events.len() <= c.max_recent_events);
        assert!(s.seen_event_ids.len() <= c.max_seen_event_ids);

        // Duplicate detection still works for recently seen ids.
        let last_id = s.seen_event_ids.back().unwrap().clone();
        let dup = heartbeat_with_id(999_999, last_id);
        assert!(matches!(
            step(&mut s, &dup, 999_999, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
    }

    #[test]
    fn meaningful_activity_after_nudge_clears_recovery_pressure() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let first = step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));
        assert_eq!(s.phase, WatcherPhase::Recovering);

        // Real activity clears the pressure.
        let activity = activity_finished(&mut ids, 4000, "t2", "edit");
        assert!(matches!(
            step(&mut s, &activity, 4000, &c),
            Step::Decision(WatcherDecision::NoAction)
        ));
        assert_eq!(s.phase, WatcherPhase::Healthy);
        assert_eq!(s.grace_deadline_ms, None);

        // The following completion is a normal stop, not an escalation.
        let settle = step(&mut s, &settled_completed(&mut ids, 5000), 5000, &c);
        assert!(matches!(settle, Step::Decision(WatcherDecision::AllowStop)));
    }
    #[test]
    fn duplicate_event_id_produces_no_duplicate_nudge() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        let settle = settled_completed(&mut ids, 2000);
        let first = step(&mut s, &settle, 2000, &c);
        assert!(matches!(first, Step::NeedsJudge(_)));
        let nudge = apply_verdict(&mut s, Ok(verdict(ProgressState::Stuck, 0.8)), 3000, &c);
        assert!(matches!(nudge, WatcherDecision::Nudge { .. }));
        let index_after_first = s.event_index;

        // Replayed delivery of the same event is ignored entirely.
        let dup = step(&mut s, &settle, 2000, &c);
        assert!(matches!(dup, Step::Decision(WatcherDecision::NoAction)));
        assert_eq!(s.intervention_count, 1);
        assert_eq!(s.event_index, index_after_first);
    }
}
