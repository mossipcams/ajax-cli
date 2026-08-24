//! Task-associated pull-request discovery and CI attempt reduction.

use crate::{
    adapters::{
        CiChecksObservation, CiChecksReport, CiChecksState, CommandRunner, GithubChecksAdapter,
    },
    agent_notification::{
        AgentNotification, AgentNotificationDelivery, CiFailedCheck, CI_MONITOR_STATE_KEY,
    },
    commands::CommandContext,
    diff_review::{stored_pull_requests, PullRequestRef, PullRequestState, AJAX_PULL_REQUESTS_KEY},
    models::Task,
    registry::Registry,
};
use serde::{Deserialize, Serialize};

const ACTIVE_CHECK_INTERVAL_SECS: u64 = 30;
const DISCOVERY_INTERVAL_SECS: u64 = 300;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CiAttemptStatus {
    #[default]
    Unobserved,
    Pending,
    Failed,
    Passed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct CiMonitorState {
    pub pr_number: Option<u64>,
    pub head_sha: Option<String>,
    pub status: CiAttemptStatus,
    pub failed_checks: Vec<CiFailedCheck>,
    pub check_identities: Vec<String>,
    pub has_pending: bool,
    pub last_pr_discovery_at: Option<u64>,
    pub last_check_probe_at: Option<u64>,
    pub episode_id: Option<String>,
    pub last_failure_identities: Vec<String>,
    pub saw_pending_after_failure: bool,
    pub last_notified_failure: Option<String>,
    pub delivery: Option<AgentNotificationDelivery>,
}

pub(crate) fn load_state(task: &Task) -> CiMonitorState {
    let mut state: CiMonitorState = task
        .metadata
        .get(CI_MONITOR_STATE_KEY)
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    if state.last_check_probe_at.is_none() {
        state.last_check_probe_at = task
            .metadata
            .get("ci_checks_probed_at")
            .and_then(|value| value.parse().ok());
    }
    if state.last_pr_discovery_at.is_none() {
        state.last_pr_discovery_at = state.last_check_probe_at;
    }
    if state.status == CiAttemptStatus::Unobserved
        && task
            .live_status
            .as_ref()
            .is_some_and(super::github_checks::is_github_ci_failure)
    {
        state.status = CiAttemptStatus::Failed;
    }
    state
}

pub(crate) fn store_state(task: &mut Task, state: &CiMonitorState) -> bool {
    let Ok(json) = serde_json::to_string(state) else {
        return false;
    };
    if task.metadata.get(CI_MONITOR_STATE_KEY) == Some(&json) {
        return false;
    }
    task.metadata.insert(CI_MONITOR_STATE_KEY.to_string(), json);
    true
}

pub(super) fn refresh_ci_monitor<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    now: u64,
    task_snapshots: &[Task],
    changed: &mut bool,
) {
    let github = GithubChecksAdapter::new("gh");
    for snapshot in task_snapshots {
        let Some(mut task) = context
            .registry
            .get_task_mut(&snapshot.id)
            .map(|task| task.clone())
        else {
            continue;
        };
        let previous = task.clone();
        refresh_task(&mut task, runner, &github, now);
        if task != previous {
            if let Some(stored) = context.registry.get_task_mut(&snapshot.id) {
                *stored = task;
                *changed = true;
            }
        }
    }
}

fn refresh_task(
    task: &mut Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
    now: u64,
) {
    if super::github_checks::github_probe_is_retired(task) {
        retire(task);
        return;
    }
    let mut state = load_state(task);
    let open_pr = discover_open_pr(task, runner, github, now, &mut state);
    let Some(pr) = open_pr else {
        retire_attempt(task, &mut state);
        store_state(task, &state);
        return;
    };
    refresh_open_pr(task, runner, github, now, &pr, state);
}

fn discover_open_pr(
    task: &mut Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
    now: u64,
    state: &mut CiMonitorState,
) -> Option<PullRequestRef> {
    let due = state
        .last_pr_discovery_at
        .is_none_or(|at| now.saturating_sub(at) >= DISCOVERY_INTERVAL_SECS);
    if due {
        let command = github.pr_list(&task.worktree_path.display().to_string(), &task.branch);
        match GithubChecksAdapter::parse_pr_list(&runner.run(&command)) {
            Ok(prs) => {
                task.metadata.insert(
                    AJAX_PULL_REQUESTS_KEY.to_string(),
                    serde_json::to_string(&prs).unwrap_or_else(|_| "[]".to_string()),
                );
                task.metadata
                    .remove(super::github_checks::CI_PROBE_ERROR_KEY);
            }
            Err(error) => {
                task.metadata
                    .insert(super::github_checks::CI_PROBE_ERROR_KEY.to_string(), error);
            }
        }
        state.last_pr_discovery_at = Some(now);
    }
    stored_pull_requests(task)
        .into_iter()
        .filter(|pr| pr.state == PullRequestState::Open)
        .max_by_key(|pr| pr.number)
}

fn refresh_open_pr(
    task: &mut Task,
    runner: &mut impl CommandRunner,
    github: &GithubChecksAdapter,
    now: u64,
    pr: &PullRequestRef,
    state: CiMonitorState,
) {
    let Some(_) = pr.head_sha.as_deref().filter(|sha| !sha.trim().is_empty()) else {
        store_state(task, &state);
        return;
    };
    if !checks_due(&state, now) {
        store_state(task, &state);
        return;
    }
    let command = github.pr_checks_for_pr(&task.worktree_path.display().to_string(), pr.number);
    let report = GithubChecksAdapter::parse_pr_checks_report(&runner.run(&command));
    let _ = reduce_report(task, pr, report, now);
}

fn checks_due(state: &CiMonitorState, now: u64) -> bool {
    if state.status == CiAttemptStatus::Unobserved && state.last_check_probe_at.is_none() {
        return true;
    }
    let interval = match state.status {
        CiAttemptStatus::Pending | CiAttemptStatus::Failed => ACTIVE_CHECK_INTERVAL_SECS,
        CiAttemptStatus::Passed | CiAttemptStatus::Unobserved => DISCOVERY_INTERVAL_SECS,
    };
    state
        .last_check_probe_at
        .is_none_or(|at| now.saturating_sub(at) >= interval)
}

fn retire(task: &mut Task) {
    let mut state = load_state(task);
    retire_attempt(task, &mut state);
    state.last_pr_discovery_at = None;
    store_state(task, &state);
}

fn retire_attempt(task: &mut Task, state: &mut CiMonitorState) {
    let discovery = state.last_pr_discovery_at;
    *state = CiMonitorState {
        last_pr_discovery_at: discovery,
        ..CiMonitorState::default()
    };
    super::github_checks::clear_github_ci_evidence(task);
}

pub(crate) fn reduce_report(
    task: &mut Task,
    pr: &PullRequestRef,
    report: CiChecksReport,
    observed_at: u64,
) -> bool {
    let previous = load_state(task);
    let mut state = previous.clone();
    let head_sha = pr.head_sha.clone().unwrap_or_default();
    let new_attempt =
        state.pr_number != Some(pr.number) || state.head_sha.as_deref() != Some(head_sha.as_str());
    if new_attempt {
        let discovery = state.last_pr_discovery_at;
        state = CiMonitorState {
            pr_number: Some(pr.number),
            head_sha: Some(head_sha),
            last_pr_discovery_at: discovery,
            ..CiMonitorState::default()
        };
    }
    let episode_previous = if new_attempt {
        CiMonitorState::default()
    } else {
        previous.clone()
    };
    state.last_check_probe_at = Some(observed_at);
    task.metadata
        .insert("ci_checks_probed_at".to_string(), observed_at.to_string());
    apply_report(task, &mut state, &episode_previous, report);
    store_state(task, &state) || state != previous
}

fn apply_report(
    task: &mut Task,
    state: &mut CiMonitorState,
    previous: &CiMonitorState,
    report: CiChecksReport,
) {
    if report.state == CiChecksState::Unobservable {
        if let Some(error) = report.error {
            task.metadata
                .insert(super::github_checks::CI_PROBE_ERROR_KEY.to_string(), error);
        }
        return;
    }
    task.metadata
        .remove(super::github_checks::CI_PROBE_ERROR_KEY);
    state.failed_checks = report.failed_checks;
    state.check_identities = report.check_identities;
    state.has_pending = report.has_pending;
    match report.state {
        CiChecksState::Failed => apply_failed(task, state, previous),
        CiChecksState::Pending => apply_pending(task, state),
        CiChecksState::Healthy => apply_healthy(task, state),
        CiChecksState::Unobservable => unreachable!("handled above"),
    }
}

fn apply_failed(task: &mut Task, state: &mut CiMonitorState, previous: &CiMonitorState) {
    let summary = state
        .failed_checks
        .iter()
        .map(|check| check.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    super::github_checks::apply_github_checks_observation(
        task,
        CiChecksObservation::Failed { summary },
        std::time::SystemTime::now(),
    );
    state.status = CiAttemptStatus::Failed;
    let first_episode = previous.episode_id.is_none();
    let distinct_rerun = previous.saw_pending_after_failure
        && !state.check_identities.is_empty()
        && state.check_identities != previous.last_failure_identities;
    if first_episode || distinct_rerun {
        start_episode(task, state);
    }
}

fn start_episode(task: &Task, state: &mut CiMonitorState) {
    let identities = if state.check_identities.is_empty() {
        state
            .failed_checks
            .iter()
            .map(|check| check.name.clone())
            .collect()
    } else {
        state.check_identities.clone()
    };
    state.episode_id = Some(format!(
        "ci-failed:{}:{}:{}:{}",
        task.id.as_str(),
        state.pr_number.unwrap_or_default(),
        state.head_sha.as_deref().unwrap_or_default(),
        identities.join(",")
    ));
    state.last_failure_identities = state.check_identities.clone();
    state.saw_pending_after_failure = false;
    state.delivery = None;
}

fn apply_pending(task: &mut Task, state: &mut CiMonitorState) {
    if state.episode_id.is_some() {
        state.saw_pending_after_failure = true;
    }
    apply_observation(
        task,
        state,
        CiAttemptStatus::Pending,
        CiChecksObservation::Pending,
    );
}

fn apply_healthy(task: &mut Task, state: &mut CiMonitorState) {
    state.has_pending = false;
    apply_observation(
        task,
        state,
        CiAttemptStatus::Passed,
        CiChecksObservation::Healthy,
    );
}

fn apply_observation(
    task: &mut Task,
    state: &mut CiMonitorState,
    status: CiAttemptStatus,
    observation: CiChecksObservation,
) {
    state.status = status;
    super::github_checks::apply_github_checks_observation(
        task,
        observation,
        std::time::SystemTime::now(),
    );
}

pub fn pending_notification(task: &Task) -> Option<AgentNotification> {
    let state = load_state(task);
    let episode_id = state.episode_id.clone()?;
    if state.status != CiAttemptStatus::Failed
        || state.last_notified_failure.as_deref() == Some(&episode_id)
    {
        return None;
    }
    Some(AgentNotification::CiFailed {
        episode_id,
        task_id: task.id.clone(),
        pr_number: state.pr_number?,
        head_sha: state.head_sha?,
        failed_checks: state.failed_checks,
    })
}

#[cfg(test)]
mod tests;
