use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    adapters::CiChecksObservation,
    live::{self, LiveObservation, LiveStatusKind},
    models::{LifecycleStatus, LiveStatusClass, Task},
};

pub(super) const CI_PROBE_ERROR_KEY: &str = "ci_probe_error";

const CI_CHECKS_PROBED_AT_KEY: &str = "ci_checks_probed_at";
const GITHUB_CI_FAILED_PREFIX: &str = "ci failed";

pub(super) fn github_probe_is_retired(task: &Task) -> bool {
    matches!(
        task.lifecycle_status,
        LifecycleStatus::Removed | LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) || task.branch.trim().is_empty()
        || task.has_side_flag(crate::models::SideFlag::WorktreeMissing)
}

pub(super) fn apply_github_checks_observation(
    task: &mut Task,
    observation: CiChecksObservation,
    now: SystemTime,
) {
    task.metadata.insert(
        CI_CHECKS_PROBED_AT_KEY.to_string(),
        unix_seconds(now).to_string(),
    );

    match observation {
        CiChecksObservation::Failed { summary } => {
            task.metadata.remove(CI_PROBE_ERROR_KEY);
            if can_apply_github_override(task) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::CiFailed, format!("ci failed: {summary}")),
                );
            }
        }
        CiChecksObservation::Pending => {
            task.metadata.remove(CI_PROBE_ERROR_KEY);
            if can_apply_github_override(task) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::CiPending, "ci running"),
                );
            }
        }
        CiChecksObservation::Healthy => {
            task.metadata.remove(CI_PROBE_ERROR_KEY);
            clear_github_ci_evidence(task);
        }
        CiChecksObservation::Unobservable { reason } => {
            task.metadata.insert(CI_PROBE_ERROR_KEY.to_string(), reason);
            if task
                .live_status
                .as_ref()
                .is_some_and(|status| status.kind == LiveStatusKind::CiPending)
            {
                task.live_status = None;
                task.live_status_observed_at = None;
            }
        }
    }
}

pub(super) fn clear_github_ci_evidence(task: &mut Task) {
    if task.live_status.as_ref().is_some_and(is_github_owned_ci) {
        task.live_status = None;
        task.live_status_observed_at = None;
    }
    if !task
        .live_status
        .as_ref()
        .is_some_and(is_local_check_failure)
    {
        task.remove_side_flag(crate::models::SideFlag::TestsFailed);
    }
}

fn can_apply_github_override(task: &Task) -> bool {
    match task.live_status.as_ref() {
        None => true,
        Some(status) if is_github_ci_failure(status) => true,
        Some(status) if is_unacknowledged_attention_gate(task, status) => false,
        Some(status) => !matches!(
            status.kind.class(),
            LiveStatusClass::Error | LiveStatusClass::MissingSubstrate
        ),
    }
}

fn is_unacknowledged_attention_gate(task: &Task, status: &LiveObservation) -> bool {
    matches!(
        status.kind,
        LiveStatusKind::WaitingForApproval | LiveStatusKind::WaitingForInput
    ) && !matches!(
        (task.live_status_observed_at, task.attention_acknowledged_at),
        (Some(observed_at), Some(acknowledged_at)) if observed_at <= acknowledged_at
    )
}

pub(super) fn is_github_ci_failure(status: &LiveObservation) -> bool {
    status.kind == LiveStatusKind::CiFailed && status.summary.starts_with(GITHUB_CI_FAILED_PREFIX)
}

fn is_github_owned_ci(status: &LiveObservation) -> bool {
    is_github_ci_failure(status) || status.kind == LiveStatusKind::CiPending
}

fn is_local_check_failure(status: &LiveObservation) -> bool {
    status.kind == LiveStatusKind::CiFailed && status.summary == "check failed"
}

fn unix_seconds(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
