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

/// Queue a nudge for delivery. Returns whether the task changed.
///
/// A task has at most one pending nudge: enqueueing while one is pending, or
/// re-enqueueing an id that was already delivered, is a no-op so duplicate
/// watcher events cannot double-nudge.
pub fn enqueue_watcher_nudge(task: &mut Task, id: &str, reason: WatcherReason) -> bool {
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
            last_intervention_event_index: None,
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
}
