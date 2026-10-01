//! Per-task watcher bookkeeping stored in task metadata.
//!
//! The watcher reuses the existing CI notification delivery paths; this store
//! only keeps the bookkeeping: the persisted watcher state, an optional
//! pending nudge, and the last delivery. Delivery itself is owned by the
//! host runtime (tmux send-keys or the TaskSession queue).

use crate::agent_notification::{
    AgentNotification, AgentNotificationDelivery, AgentNotificationDeliveryStatus,
};
use crate::agent_watcher::{WatcherPersistedState, WatcherReason};
use crate::models::Task;
use serde::{Deserialize, Serialize};

/// Task metadata key holding the watcher store JSON.
pub const WATCHER_STATE_KEY: &str = "ajax_watcher";

/// Failed delivery attempts after which a pending nudge is dropped so a
/// broken transport cannot retry forever.
pub const MAX_DELIVERY_ATTEMPTS: u32 = 3;

/// A nudge the watcher asked for that has not yet been delivered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatcherPendingNudge {
    pub id: String,
    pub reason: WatcherReason,
    #[serde(default)]
    pub delivery_attempts: u32,
}

/// Everything the watcher persists for one task.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherStore {
    pub state: Option<WatcherPersistedState>,
    pub pending_nudge: Option<WatcherPendingNudge>,
    pub delivery: Option<AgentNotificationDelivery>,
}

/// Read the watcher store from task metadata. Missing or malformed JSON
/// yields an empty store.
pub fn load_store(task: &Task) -> WatcherStore {
    task.metadata
        .get(WATCHER_STATE_KEY)
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

/// Persist the watcher store, returning whether the metadata changed.
fn persist(task: &mut Task, store: &WatcherStore) -> bool {
    let Ok(json) = serde_json::to_string(store) else {
        return false;
    };
    if task.metadata.get(WATCHER_STATE_KEY).map(String::as_str) == Some(json.as_str()) {
        return false;
    }
    task.metadata.insert(WATCHER_STATE_KEY.to_string(), json);
    true
}

/// Read the persisted watcher state, if any.
pub fn load_watcher_state(task: &Task) -> Option<WatcherPersistedState> {
    load_store(task).state
}

/// Persist the watcher state, idempotently. Returns whether the task changed.
pub fn store_watcher_state(task: &mut Task, state: &WatcherPersistedState) -> bool {
    let mut store = load_store(task);
    if store.state.as_ref() == Some(state) {
        return false;
    }
    store.state = Some(state.clone());
    persist(task, &store)
}

/// The pending watcher nudge as a transport-neutral notification, if any.
pub fn pending_watcher_nudge(task: &Task) -> Option<AgentNotification> {
    load_store(task)
        .pending_nudge
        .map(|pending| AgentNotification::WatcherNudge {
            id: pending.id,
            task_id: task.id.clone(),
            reason: pending.reason,
        })
}

/// Cancel a nudge made stale by newer watcher evidence, preserving delivery history.
pub fn cancel_pending_watcher_nudge(task: &mut Task) -> bool {
    let mut store = load_store(task);
    if store.pending_nudge.take().is_none() {
        return false;
    }
    persist(task, &store)
}

/// Queue a nudge for delivery. Returns whether the task changed.
///
/// A task has at most one pending nudge: enqueueing while one is pending, or
/// re-enqueueing an id that was already delivered, is a no-op so duplicate
/// watcher events cannot double-nudge.
pub fn enqueue_watcher_nudge(task: &mut Task, id: &str, reason: WatcherReason) -> bool {
    if crate::agent_watcher::nudge_prompt(&reason).is_empty() {
        return false;
    }
    let mut store = load_store(task);
    if store.pending_nudge.is_some() {
        return false;
    }
    if store
        .delivery
        .as_ref()
        .is_some_and(|delivery| delivery.notification_id == id)
    {
        return false;
    }
    store.pending_nudge = Some(WatcherPendingNudge {
        id: id.to_string(),
        reason,
        delivery_attempts: 0,
    });
    persist(task, &store)
}

/// Record a delivery of the pending watcher nudge. Returns whether the task
/// changed. Accepted/Queued clears the pending nudge; Error keeps it but
/// increments the attempt counter and drops it after
/// [`MAX_DELIVERY_ATTEMPTS`] failures.
pub fn record_watcher_delivery(task: &mut Task, delivery: AgentNotificationDelivery) -> bool {
    let mut store = load_store(task);
    let Some(pending) = store.pending_nudge.as_mut() else {
        return false;
    };
    match delivery.status {
        AgentNotificationDeliveryStatus::Error => {
            pending.delivery_attempts += 1;
            if pending.delivery_attempts >= MAX_DELIVERY_ATTEMPTS {
                store.pending_nudge = None;
            }
        }
        AgentNotificationDeliveryStatus::Queued | AgentNotificationDeliveryStatus::Accepted => {
            store.pending_nudge = None;
        }
    }
    store.delivery = Some(delivery);
    persist(task, &store)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_watcher::{test_support::*, *};
    use crate::models::{AgentClient, TaskId};

    fn task() -> Task {
        Task::new(
            TaskId::new("task-1"),
            "web",
            "t1",
            "title",
            "branch",
            "main",
            "/tmp/wt",
            "web-t1",
            "task",
            AgentClient::Cursor,
        )
    }

    fn watcher_state() -> WatcherPersistedState {
        WatcherPersistedState {
            intervention_count: 1,
            premature_stop_nudges: 0,
            loop_nudges: 0,
            phase: Default::default(),
            nudge_seq: 0,
            last_intervention_at_ms: None,
            grace_deadline_ms: None,
            last_verdict: None,
            last_seen_event_id: None,
        }
    }

    fn delivery(status: AgentNotificationDeliveryStatus, id: &str) -> AgentNotificationDelivery {
        AgentNotificationDelivery {
            notification_id: id.to_string(),
            status,
            detail: None,
        }
    }

    #[test]
    fn store_state_is_idempotent() {
        let mut task = task();
        let state = watcher_state();
        assert!(store_watcher_state(&mut task, &state));
        assert_eq!(load_watcher_state(&task).as_ref(), Some(&state));
        assert!(!store_watcher_state(&mut task, &state));
    }

    #[test]
    fn enqueue_dedupes_by_id_and_never_overwrites_pending() {
        let mut task = task();
        assert!(enqueue_watcher_nudge(&mut task, "n1", WatcherReason::Stuck));
        assert!(!enqueue_watcher_nudge(
            &mut task,
            "n1",
            WatcherReason::Stuck
        ));
        assert!(!enqueue_watcher_nudge(
            &mut task,
            "n2",
            WatcherReason::Stuck
        ));

        let pending = load_store(&task).pending_nudge.expect("pending nudge");
        assert_eq!(pending.id, "n1");
        assert_eq!(pending.reason, WatcherReason::Stuck);
        assert_eq!(pending.delivery_attempts, 0);
    }

    #[test]
    fn already_delivered_id_cannot_be_re_enqueued() {
        let mut task = task();
        assert!(enqueue_watcher_nudge(&mut task, "n1", WatcherReason::Stuck));
        assert!(record_watcher_delivery(
            &mut task,
            delivery(AgentNotificationDeliveryStatus::Accepted, "n1")
        ));
        assert!(!enqueue_watcher_nudge(
            &mut task,
            "n1",
            WatcherReason::Stuck
        ));
        assert!(enqueue_watcher_nudge(&mut task, "n2", WatcherReason::Stuck));
    }

    #[test]
    fn accepted_and_queued_clear_pending_and_record_delivery() {
        for status in [
            AgentNotificationDeliveryStatus::Accepted,
            AgentNotificationDeliveryStatus::Queued,
        ] {
            let mut task = task();
            enqueue_watcher_nudge(&mut task, "n1", WatcherReason::Stuck);
            assert!(record_watcher_delivery(
                &mut task,
                delivery(status.clone(), "n1")
            ));
            let store = load_store(&task);
            assert!(store.pending_nudge.is_none());
            assert_eq!(store.delivery.expect("delivery").status, status);
        }
    }

    #[test]
    fn error_keeps_pending_until_attempt_cap_then_drops() {
        let mut task = task();
        enqueue_watcher_nudge(&mut task, "n1", WatcherReason::Stuck);

        for expected_attempts in 1..MAX_DELIVERY_ATTEMPTS {
            assert!(record_watcher_delivery(
                &mut task,
                delivery(AgentNotificationDeliveryStatus::Error, "n1")
            ));
            let pending = load_store(&task).pending_nudge.expect("still pending");
            assert_eq!(pending.delivery_attempts, expected_attempts);
        }

        assert!(record_watcher_delivery(
            &mut task,
            delivery(AgentNotificationDeliveryStatus::Error, "n1")
        ));
        let store = load_store(&task);
        assert!(store.pending_nudge.is_none());
        assert_eq!(
            store.delivery.expect("delivery").status,
            AgentNotificationDeliveryStatus::Error
        );
        // The dropped id is remembered, so duplicate events cannot requeue it.
        assert!(!enqueue_watcher_nudge(
            &mut task,
            "n1",
            WatcherReason::Stuck
        ));
    }

    #[test]
    fn record_delivery_without_pending_nudge_is_noop() {
        let mut task = task();
        assert!(!record_watcher_delivery(
            &mut task,
            delivery(AgentNotificationDeliveryStatus::Accepted, "n1")
        ));
        assert!(!task.metadata.contains_key(WATCHER_STATE_KEY));
    }
    #[test]
    fn cancellation_preserves_state_and_delivery_and_is_idempotent() {
        let mut task = task();
        assert!(!cancel_pending_watcher_nudge(&mut task));
        assert!(task.metadata.is_empty());
        store_watcher_state(&mut task, &watcher_state());
        enqueue_watcher_nudge(&mut task, "first", WatcherReason::Stuck);
        record_watcher_delivery(
            &mut task,
            delivery(AgentNotificationDeliveryStatus::Accepted, "first"),
        );
        enqueue_watcher_nudge(&mut task, "second", WatcherReason::Stuck);
        let before = load_store(&task);
        assert!(cancel_pending_watcher_nudge(&mut task));
        assert!(pending_watcher_nudge(&task).is_none());
        let after = load_store(&task);
        assert_eq!(after.state, before.state);
        assert_eq!(after.delivery, before.delivery);
        assert!(!cancel_pending_watcher_nudge(&mut task));
    }
    #[test]
    fn persisted_subset_round_trips() {
        let mut s = state();
        let c = config();
        let mut ids = Ids::new();

        step(&mut s, &turn_started(&mut ids, 1000), 1000, &c);
        step(&mut s, &settled_completed(&mut ids, 2000), 2000, &c);
        s.last_verdict = Some(WatcherVerdict {
            state: ProgressState::Stuck,
            confidence: 0.8,
        });

        let persisted = s.persisted();
        let json = serde_json::to_string(&persisted).expect("serialize persisted subset");
        let restored: WatcherPersistedState =
            serde_json::from_str(&json).expect("deserialize persisted subset");
        assert_eq!(persisted, restored);
        assert_eq!(persisted.intervention_count, 0);
        assert!(persisted.last_seen_event_id.is_some());

        let mut fresh = state();
        fresh.apply_persisted(&restored);
        assert_eq!(fresh.intervention_count, 0);
        assert_eq!(fresh.last_verdict, s.last_verdict);
        assert!(
            fresh.has_seen_event_id(restored.last_seen_event_id.as_deref().expect("id present"))
        );
    }

    #[test]
    fn persisted_state_carries_phase_and_nudge_seq() {
        let mut s = state();
        s.phase = WatcherPhase::Escalated;
        s.nudge_seq = 7;

        let persisted = s.persisted();
        assert_eq!(persisted.phase, WatcherPhase::Escalated);
        assert_eq!(persisted.nudge_seq, 7);

        let json = serde_json::to_string(&persisted).expect("serialize persisted subset");
        let restored: WatcherPersistedState =
            serde_json::from_str(&json).expect("deserialize persisted subset");
        let mut fresh = state();
        fresh.apply_persisted(&restored);
        assert_eq!(fresh.phase, WatcherPhase::Escalated);
        assert_eq!(fresh.nudge_seq, 7);
    }

    #[test]
    fn persisted_state_defaults_phase_and_nudge_seq_for_old_metadata() {
        // Metadata written before phase/nudge_seq existed still loads.
        let json = r#"{"intervention_count":1}"#;
        let restored: WatcherPersistedState =
            serde_json::from_str(json).expect("deserialize old persisted subset");
        assert_eq!(restored.phase, WatcherPhase::Healthy);
        assert_eq!(restored.nudge_seq, 0);
        assert_eq!(restored.intervention_count, 1);
    }

    #[test]
    fn legacy_state_fields_are_ignored_without_losing_live_state() {
        let mut task = task();
        task.metadata.insert(
            WATCHER_STATE_KEY.into(),
            serde_json::json!({
                "state": {
                    "intervention_count": 2, "nudge_seq": 7, "phase": "recovering",
                    "last_intervention_event_index": 42, "failure_count": 3,
                    "settle_attempts": 4, "repeat_counts": [["old", 2]],
                    "grace_deadline_ms": 12345
                }
            })
            .to_string(),
        );
        let restored = load_watcher_state(&task).unwrap();
        assert_eq!(restored.intervention_count, 2);
        assert_eq!(restored.nudge_seq, 7);
        assert_eq!(restored.phase, WatcherPhase::Recovering);
        assert_eq!(restored.grace_deadline_ms, Some(12345));
        for old_phase in ["suspected_loop", "premature_stop"] {
            let state: WatcherPersistedState =
                serde_json::from_value(serde_json::json!({"phase": old_phase})).unwrap();
            assert_eq!(state.phase, WatcherPhase::Healthy);
        }
    }

    #[test]
    fn operator_decisions_cannot_be_queued_as_nudges() {
        let mut task = task();
        for reason in [
            WatcherReason::NeedsUser,
            WatcherReason::StalledAfterNudge,
            WatcherReason::InterventionCap,
        ] {
            assert!(!enqueue_watcher_nudge(&mut task, "nudge", reason));
        }
        assert!(pending_watcher_nudge(&task).is_none());
    }
}
