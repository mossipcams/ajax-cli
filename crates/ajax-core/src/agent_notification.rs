//! Transport-neutral notifications destined for the task's owning agent.

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
}

impl AgentNotification {
    pub fn id(&self) -> &str {
        match self {
            Self::CiFailed { episode_id, .. } => episode_id,
        }
    }

    pub fn task_id(&self) -> &TaskId {
        match self {
            Self::CiFailed { task_id, .. } => task_id,
        }
    }

    pub fn prompt(&self) -> String {
        let Self::CiFailed {
            pr_number,
            head_sha,
            failed_checks,
            ..
        } = self;
        let mut checks = failed_checks.clone();
        checks.sort();
        let rows = checks
            .iter()
            .map(|check| match (&check.link, &check.identity) {
                (Some(link), Some(identity)) => format!("- {} — {link} ({identity})", check.name),
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
    crate::runtime_refresh::ci_monitor::store_state(task, &state)
}

pub fn pending_for_task(task: &crate::models::Task) -> Option<AgentNotification> {
    crate::runtime_refresh::ci_monitor::pending_notification(task)
}

pub fn delivery_for_task(task: &crate::models::Task) -> Option<AgentNotificationDelivery> {
    crate::runtime_refresh::ci_monitor::load_state(task).delivery
}
