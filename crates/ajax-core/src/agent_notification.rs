//! Transport-neutral notifications destined for the task's owning agent.

use crate::agent_watcher::{nudge_prompt, WatcherReason};
use crate::models::TaskId;
use serde::{Deserialize, Serialize};

pub const CI_MONITOR_STATE_KEY: &str = "ajax_ci_monitor";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CiFailedCheck {
    pub name: String,
    pub link: Option<String>,
    pub identity: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentNotification {
    CiFailed {
        episode_id: String,
        task_id: TaskId,
        pr_number: u64,
        head_sha: String,
        failed_checks: Vec<CiFailedCheck>,
    },
    /// The agent watcher asked the owning agent to continue or explain.
    WatcherNudge {
        id: String,
        task_id: TaskId,
        reason: WatcherReason,
    },
}

impl AgentNotification {
    pub fn id(&self) -> &str {
        match self {
            Self::CiFailed { episode_id, .. } => episode_id,
            Self::WatcherNudge { id, .. } => id,
        }
    }

    pub fn task_id(&self) -> &TaskId {
        match self {
            Self::CiFailed { task_id, .. } => task_id,
            Self::WatcherNudge { task_id, .. } => task_id,
        }
    }

    pub fn prompt(&self) -> String {
        match self {
            Self::WatcherNudge { reason, .. } => nudge_prompt(reason).to_string(),
            Self::CiFailed {
                pr_number,
                head_sha,
                failed_checks,
                ..
            } => {
                let mut checks = failed_checks.clone();
                checks.sort();
                let rows = checks
                    .iter()
                    .map(|check| match (&check.link, &check.identity) {
                        (Some(link), Some(identity)) => {
                            format!("- {} — {link} ({identity})", check.name)
                        }
                        (Some(link), None) => format!("- {} — {link}", check.name),
                        (None, Some(identity)) => format!("- {} ({identity})", check.name),
                        (None, None) => format!("- {}", check.name),
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                format!(
                    "CI failed for PR #{pr_number} at head {head_sha}.\n\nFailed checks:\n{rows}\n\nInspect the logs for every failed check, determine whether each failure is caused by this branch, fix every relevant failure, run the repository's local verification, commit the fix, and push the branch. If a failure is unrelated, report the evidence instead of changing unrelated code."
                )
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentNotificationDeliveryStatus {
    Queued,
    Accepted,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentNotificationDelivery {
    pub notification_id: String,
    pub status: AgentNotificationDeliveryStatus,
    pub detail: Option<String>,
}

pub fn record_delivery(
    task: &mut crate::models::Task,
    delivery: AgentNotificationDelivery,
) -> bool {
    // Watcher nudges keep their bookkeeping in the watcher store; everything
    // else is a CI notification and keeps the existing CI monitor state.
    if crate::agent_watcher::pending_watcher_nudge(task)
        .is_some_and(|pending| pending.id() == delivery.notification_id)
    {
        if !crate::agent_watcher::record_watcher_delivery(task, delivery) {
            return false;
        }
        // The nudge is now the most recent delivery: drop the older CI
        // delivery so `delivery_for_task` reports the newest one.
        let mut state = crate::runtime_refresh::ci_monitor::load_state(task);
        if state.delivery.is_some() {
            state.delivery = None;
            return crate::runtime_refresh::ci_monitor::store_state(task, &state);
        }
        return true;
    }
    let mut state = crate::runtime_refresh::ci_monitor::load_state(task);
    if state.delivery.as_ref() == Some(&delivery) {
        return false;
    }
    if matches!(
        delivery.status,
        AgentNotificationDeliveryStatus::Queued | AgentNotificationDeliveryStatus::Accepted
    ) {
        state.last_notified_failure = Some(delivery.notification_id.clone());
    }
    state.delivery = Some(delivery);
    let stored = crate::runtime_refresh::ci_monitor::store_state(task, &state);
    if stored {
        // The CI notification is now the most recent delivery: drop the
        // older watcher delivery so `delivery_for_task` reports the newest.
        crate::agent_watcher::clear_watcher_delivery(task);
    }
    stored
}

pub fn pending_for_task(task: &crate::models::Task) -> Option<AgentNotification> {
    crate::runtime_refresh::ci_monitor::pending_notification(task)
        .or_else(|| crate::agent_watcher::pending_watcher_nudge(task))
}

/// The delivery belonging to the most recent notification. Recording a
/// delivery on one side invalidates the other side's older delivery, so at
/// most one store holds a current delivery; if both are present (legacy or
/// hand-set state), a still-pending watcher nudge is the newest.
pub fn delivery_for_task(task: &crate::models::Task) -> Option<AgentNotificationDelivery> {
    let ci_delivery = crate::runtime_refresh::ci_monitor::load_state(task).delivery;
    let watcher_delivery = crate::agent_watcher::load_store(task).delivery;
    match (ci_delivery, watcher_delivery) {
        (Some(ci), Some(watcher)) => {
            let watcher_current = crate::agent_watcher::pending_watcher_nudge(task)
                .is_some_and(|pending| pending.id() == watcher.notification_id);
            Some(if watcher_current { watcher } else { ci })
        }
        (ci, watcher) => ci.or(watcher),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_watcher::{enqueue_watcher_nudge, WatcherReason};
    use crate::models::{AgentClient, Task};

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

    fn ci_notification() -> AgentNotification {
        AgentNotification::CiFailed {
            episode_id: "ci-failed:task-1:9:abc:1".to_string(),
            task_id: TaskId::new("task-1"),
            pr_number: 9,
            head_sha: "abc".to_string(),
            failed_checks: Vec::new(),
        }
    }

    fn nudge_notification() -> AgentNotification {
        AgentNotification::WatcherNudge {
            id: "watcher-nudge:task-1:1".to_string(),
            task_id: TaskId::new("task-1"),
            reason: WatcherReason::Stuck,
        }
    }

    fn delivery(id: &str, status: AgentNotificationDeliveryStatus) -> AgentNotificationDelivery {
        AgentNotificationDelivery {
            notification_id: id.to_string(),
            status,
            detail: None,
        }
    }

    #[test]
    fn watcher_nudge_id_task_and_prompt_match_store() {
        let notification = nudge_notification();
        assert_eq!(notification.id(), "watcher-nudge:task-1:1");
        assert_eq!(notification.task_id().as_str(), "task-1");
        assert_eq!(notification.prompt(), nudge_prompt(&WatcherReason::Stuck));
        assert!(notification.prompt().contains("different approach"));
    }

    #[test]
    fn pending_for_task_prefers_ci_over_watcher_nudge() {
        let mut task = task();
        enqueue_watcher_nudge(&mut task, "watcher-nudge:task-1:1", WatcherReason::Stuck);
        // No CI state yet: the watcher nudge is the pending notification.
        assert_eq!(
            pending_for_task(&task).as_ref(),
            Some(&nudge_notification())
        );

        // A pending CI notification shadows the watcher nudge.
        let mut state = crate::runtime_refresh::ci_monitor::load_state(&task);
        state.pr_number = Some(9);
        state.head_sha = Some("abc".to_string());
        state.episode_id = Some("ci-failed:task-1:9:abc:1".to_string());
        state.status = crate::runtime_refresh::ci_monitor::CiAttemptStatus::Failed;
        state.last_notified_failure = None;
        crate::runtime_refresh::ci_monitor::store_state(&mut task, &state);
        assert_eq!(pending_for_task(&task).as_ref(), Some(&ci_notification()));
    }

    #[test]
    fn prompt_covers_every_notification_variant() {
        // The watcher nudge prompt is the deterministic template.
        let nudge = nudge_notification();
        assert_eq!(nudge.prompt(), nudge_prompt(&WatcherReason::Stuck));

        // The CI prompt renders the failure details.
        let ci = ci_notification();
        assert!(ci.prompt().contains("CI failed for PR #9"));
        assert!(ci.prompt().contains("abc"));
    }

    #[test]
    fn delivery_for_task_prefers_the_most_recent_notification() {
        // A CI failure is notified and delivered.
        let mut task = task();
        let ci = ci_notification();
        assert!(record_delivery(
            &mut task,
            delivery(ci.id(), AgentNotificationDeliveryStatus::Accepted)
        ));
        assert_eq!(
            delivery_for_task(&task).as_ref(),
            Some(&delivery(
                ci.id(),
                AgentNotificationDeliveryStatus::Accepted
            ))
        );

        // A later watcher nudge is pending: its delivery is the most recent
        // and must win over the older CI delivery.
        enqueue_watcher_nudge(&mut task, "watcher-nudge:task-1:1", WatcherReason::Stuck);
        let nudge = nudge_notification();
        assert!(record_delivery(
            &mut task,
            delivery(nudge.id(), AgentNotificationDeliveryStatus::Queued)
        ));
        assert_eq!(
            delivery_for_task(&task).as_ref(),
            Some(&delivery(
                nudge.id(),
                AgentNotificationDeliveryStatus::Queued
            ))
        );
    }

    #[test]
    fn ci_record_delivery_is_unchanged_without_watcher_pending() {
        let mut task = task();
        let ci = ci_notification();
        assert!(record_delivery(
            &mut task,
            delivery(ci.id(), AgentNotificationDeliveryStatus::Accepted)
        ));
        assert_eq!(
            delivery_for_task(&task).as_ref(),
            Some(&delivery(
                ci.id(),
                AgentNotificationDeliveryStatus::Accepted
            ))
        );
        let ci_state = crate::runtime_refresh::ci_monitor::load_state(&task);
        assert_eq!(ci_state.last_notified_failure.as_deref(), Some(ci.id()));
        assert!(crate::agent_watcher::load_store(&task)
            .pending_nudge
            .is_none());
    }

    #[test]
    fn record_delivery_routes_nudge_to_watcher_store() {
        let mut task = task();
        enqueue_watcher_nudge(&mut task, "watcher-nudge:task-1:1", WatcherReason::Stuck);
        let nudge = nudge_notification();
        assert!(record_delivery(
            &mut task,
            delivery(nudge.id(), AgentNotificationDeliveryStatus::Accepted)
        ));
        // The CI monitor state is untouched.
        assert!(crate::runtime_refresh::ci_monitor::load_state(&task)
            .delivery
            .is_none());
        let store = crate::agent_watcher::load_store(&task);
        assert!(store.pending_nudge.is_none());
        assert_eq!(
            store.delivery.expect("watcher delivery").status,
            AgentNotificationDeliveryStatus::Accepted
        );
        assert_eq!(
            delivery_for_task(&task).as_ref(),
            Some(&delivery(
                nudge.id(),
                AgentNotificationDeliveryStatus::Accepted
            ))
        );
    }
}
