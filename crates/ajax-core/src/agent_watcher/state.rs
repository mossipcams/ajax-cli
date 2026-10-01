//! Bounded watcher state: what the watcher remembers about one run.
//!
//! Every collection is capped so a long run cannot grow memory without
//! bound. The state is the watcher's own bookkeeping only — it never reads
//! or writes task status, lifecycle, or registry truth.

use std::collections::{HashMap, VecDeque};

use serde::{Deserialize, Serialize};

use crate::agent_watcher::types::{
    PendingCheckpoint, TaskFrame, WatcherPhase, WatcherSnapshot, WatcherVerdict,
};
use crate::canonical_agent_event::AttentionReason;

/// Hard cap on remembered open tool ids.
const MAX_OPEN_TOOLS: usize = 256;

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
    /// (signature, repeat count) pairs, least-recent first. Bounded.
    pub repeat_counts: Vec<(String, u32)>,
    pub failure_count: u32,
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
    pub settle_attempts: u32,
    pub intervention_count: u32,
    pub premature_stop_nudges: u32,
    pub loop_nudges: u32,
    pub last_intervention_event_index: Option<u64>,
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
    pub last_intervention_event_index: Option<u64>,
    pub last_intervention_at_ms: Option<u64>,
    pub grace_deadline_ms: Option<u64>,
    pub last_verdict: Option<WatcherVerdict>,
    pub last_seen_event_id: Option<String>,
}

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
            repeat_counts: Vec::new(),
            failure_count: 0,
            pending_attention: None,
            pending_checkpoint: None,
            loop_checkpoint_signature: None,
            open_tools: Vec::new(),
            open_tool_started_at_ms: HashMap::new(),
            open_children: 0,
            settle_attempts: 0,
            intervention_count: 0,
            premature_stop_nudges: 0,
            loop_nudges: 0,
            last_intervention_event_index: None,
            last_intervention_at_ms: None,
            last_verdict: None,
            grace_deadline_ms: None,
            seen_event_ids: VecDeque::new(),
            event_index: 0,
            nudge_seq: 0,
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

    /// Increment the repeat count for `signature` (LRU order) and return the
    /// new count. Evicts the least-recent signature when over `cap`.
    pub fn bump_repeat(&mut self, signature: &str, cap: usize) -> u32 {
        if let Some(slot) = self
            .repeat_counts
            .iter_mut()
            .find(|(sig, _)| sig == signature)
        {
            slot.1 += 1;
            let count = slot.1;
            let moved = self.repeat_counts.remove(
                self.repeat_counts
                    .iter()
                    .position(|(sig, _)| sig == signature)
                    .expect("position of the slot just found"),
            );
            self.repeat_counts.push(moved);
            return count;
        }
        if self.repeat_counts.len() >= cap {
            self.repeat_counts.remove(0);
        }
        self.repeat_counts.push((signature.to_string(), 1));
        1
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
            .retain(|_, at| now_ms.saturating_sub(*at) <= 600_000);
        self.open_tools
            .retain(|id| self.open_tool_started_at_ms.contains_key(id));
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
        self.loop_checkpoint_signature = None;
        self.intervention_count += 1;
        self.nudge_seq += 1;
        self.last_intervention_event_index = Some(self.event_index);
        self.last_intervention_at_ms = Some(now_ms);
        self.phase = WatcherPhase::Recovering;
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
            last_intervention_event_index: self.last_intervention_event_index,
            last_intervention_at_ms: self.last_intervention_at_ms,
            grace_deadline_ms: self.grace_deadline_ms,
            last_verdict: self.last_verdict.clone(),
            last_seen_event_id: self.seen_event_ids.back().cloned(),
            phase: self.phase,
            nudge_seq: self.nudge_seq,
        }
    }

    /// Fold a persisted subset back into the state. Rings and transient
    /// counters stay as they are; the caller replays events for those.
    pub fn apply_persisted(&mut self, persisted: &WatcherPersistedState) {
        self.intervention_count = persisted.intervention_count;
        self.premature_stop_nudges = persisted.premature_stop_nudges;
        self.loop_nudges = persisted.loop_nudges;
        self.last_intervention_event_index = persisted.last_intervention_event_index;
        self.last_intervention_at_ms = persisted.last_intervention_at_ms;
        self.grace_deadline_ms = persisted.grace_deadline_ms;
        self.last_verdict = persisted.last_verdict.clone();
        self.phase = persisted.phase;
        self.nudge_seq = persisted.nudge_seq;
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
