//! GitHub PR-check probes during Full-tier runtime refresh.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    adapters::{CiChecksObservation, CommandRunner, GithubChecksAdapter},
    commands::CommandContext,
    live::{self, LiveObservation, LiveStatusKind},
    models::{LifecycleStatus, LiveStatusClass, Task, TaskId},
    registry::Registry,
};

use super::refresh_cached_annotations;

#[allow(dead_code)]
const CI_CHECKS_PROBE_INTERVAL: Duration = Duration::from_secs(300);
#[allow(dead_code)]
const CI_CHECKS_FAILED_PROBE_INTERVAL: Duration = Duration::from_secs(30);
const CI_CHECKS_PROBED_AT_KEY: &str = "ci_checks_probed_at";
pub(super) const CI_PROBE_ERROR_KEY: &str = "ci_probe_error";
const GITHUB_CI_FAILED_PREFIX: &str = "ci failed";

#[allow(dead_code)]
pub(super) fn refresh_github_check_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    now: SystemTime,
    registered_runtime_tasks: &[(TaskId, String, String, PathBuf)],
    github_ci_failure_at_refresh_start: &BTreeMap<TaskId, bool>,
    changed: &mut bool,
) {
    let github = GithubChecksAdapter::new("gh");

    for (task_id, _repo, branch, worktree_path) in registered_runtime_tasks {
        let had_github_ci_failure = github_ci_failure_at_refresh_start
            .get(task_id)
            .copied()
            .unwrap_or(false);
        // Retired probes leave evidence nothing can ever confirm again: drop it
        // so a merged task stops projecting "CI running" (plan §7).
        if let Some(task) = context.registry.get_task_mut(task_id) {
            if github_probe_is_retired(task)
                && task.live_status.as_ref().is_some_and(is_github_owned_ci)
            {
                let previous = task.clone();
                clear_github_ci_evidence(task);
                refresh_cached_annotations(task);
                *changed |= *task != previous;
            }
        }

        let should_probe = context
            .registry
            .get_task_mut(task_id)
            .is_some_and(|task| should_probe_github_checks(task, now, had_github_ci_failure));
        if !should_probe || branch.trim().is_empty() {
            continue;
        }

        let command = github.pr_checks(&worktree_path.to_string_lossy(), branch);
        let result = runner.run(&command);
        let observation = GithubChecksAdapter::parse_pr_checks(&result);

        if let Some(task) = context.registry.get_task_mut(task_id) {
            let previous = task.clone();
            apply_github_checks_observation(task, observation, now);
            refresh_cached_annotations(task);
            *changed |= *task != previous;
        }
    }
}

/// True when this task will never be probed again, so any GitHub-owned CI
/// evidence it still holds can no longer be confirmed and must not keep
/// projecting (plan §7: rows 5/6 require "relevant + not stale").
pub(super) fn github_probe_is_retired(task: &Task) -> bool {
    matches!(
        task.lifecycle_status,
        LifecycleStatus::Removed | LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) || task.branch.trim().is_empty()
        || task.has_side_flag(crate::models::SideFlag::WorktreeMissing)
}

#[allow(dead_code)]
fn should_probe_github_checks(task: &Task, now: SystemTime, had_github_ci_failure: bool) -> bool {
    if github_probe_is_retired(task) {
        return false;
    }

    let Some(probed_at) = task
        .metadata
        .get(CI_CHECKS_PROBED_AT_KEY)
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return true;
    };

    let interval =
        if task.live_status.as_ref().is_some_and(is_github_ci_failure) || had_github_ci_failure {
            CI_CHECKS_FAILED_PROBE_INTERVAL
        } else {
            CI_CHECKS_PROBE_INTERVAL
        };
    unix_seconds(now).saturating_sub(probed_at) > interval.as_secs()
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
            // Pending checks are a relevant GitHub result: surface them as
            // Running with a CI explanation (override the native phase), but
            // never over an existing error or missing-substrate state.
            task.metadata.remove(CI_PROBE_ERROR_KEY);
            if can_apply_github_override(task) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::CiPending, "ci running"),
                );
            }
        }
        CiChecksObservation::Healthy => {
            // Passing checks clear the GitHub override and reveal the native
            // hook-derived status. Passing CI alone is not Done.
            task.metadata.remove(CI_PROBE_ERROR_KEY);
            clear_github_ci_evidence(task);
        }
        CiChecksObservation::Unobservable { reason } => {
            // A probe that cannot observe the PR can no longer vouch for a run
            // it previously reported pending; leaving it would project
            // "CI running" indefinitely (plan §7 staleness). A failure we did
            // observe stays until a later probe supersedes it.
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

/// True when the task is parked on an approval/input gate the operator has not
/// acknowledged yet.
///
/// Such a gate is the only actionable signal the operator receives, and a
/// `Running` projection can never notify (`attention.rs` clears the notify
/// candidate for `Running`). A GitHub override would therefore make the gate
/// both invisible and unnotified for as long as CI runs, so it yields here.
/// This narrows plan §6 row 6, which ranks display and does not model
/// notification.
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

/// GitHub-owned CI live status (failure or pending) that a passing probe clears
/// to reveal the native hook-derived status.
fn is_github_owned_ci(status: &LiveObservation) -> bool {
    is_github_ci_failure(status) || status.kind == LiveStatusKind::CiPending
}

fn is_local_check_failure(status: &LiveObservation) -> bool {
    status.kind == LiveStatusKind::CiFailed && status.summary == "check failed"
}

fn unix_seconds(at: SystemTime) -> u64 {
    at.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
