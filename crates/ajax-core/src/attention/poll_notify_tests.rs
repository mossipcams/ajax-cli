use crate::lifecycle::mark_active;
use crate::models::{
    AgentClient, AgentRuntimeStatus, LiveObservation, LiveStatusKind, SideFlag, Task, TaskId,
};

fn task_with_flags(handle: &str, flags: &[SideFlag]) -> Task {
    let mut task = Task::new(
        TaskId::new(format!("task-{handle}")),
        "web",
        handle,
        format!("Task {handle}"),
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/{handle}"),
        format!("ajax-web-{handle}"),
        "task",
        AgentClient::Codex,
    );

    for flag in flags {
        task.add_side_flag(*flag);
    }

    task
}

fn active_task(handle: &str) -> Task {
    let mut task = task_with_flags(handle, &[]);
    mark_active(&mut task).unwrap();
    task
}

fn at(seconds: u64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
}

fn seed_ci_monitor_rerun(task: &mut Task) {
    use crate::runtime_refresh::ci_monitor::{load_state, reduce_report};
    use crate::{
        adapters::{CiChecksReport, CiChecksState},
        diff_review::{PullRequestRef, PullRequestState},
    };
    let pr = PullRequestRef {
        number: 42,
        title: "Fix".to_string(),
        url: "https://github.test/pull/42".to_string(),
        state: PullRequestState::Open,
        head_ref: "ajax/fix".to_string(),
        head_sha: Some("aaa".to_string()),
    };
    reduce_report(
        task,
        &pr,
        CiChecksReport {
            state: CiChecksState::Failed,
            failed_checks: vec![crate::agent_notification::CiFailedCheck {
                name: "lint".to_string(),
                link: Some("https://github.test/runs/1".to_string()),
                identity: Some("run:1".to_string()),
            }],
            check_identities: vec!["run:1".to_string()],
            has_pending: false,
            error: None,
        },
        100,
    );
    reduce_report(
        task,
        &pr,
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run:2".to_string()],
            has_pending: true,
            error: None,
        },
        101,
    );
    reduce_report(
        task,
        &pr,
        CiChecksReport {
            state: CiChecksState::Failed,
            failed_checks: vec![crate::agent_notification::CiFailedCheck {
                name: "lint".to_string(),
                link: Some("https://github.test/runs/1".to_string()),
                identity: Some("run:1".to_string()),
            }],
            check_identities: vec!["run:1".to_string()],
            has_pending: true,
            error: None,
        },
        102,
    );
    assert!(load_state(task).saw_pending_after_failure);
}

fn seed_ci_monitor_first_attempt_pending(task: &mut Task) {
    use crate::runtime_refresh::ci_monitor::{load_state, reduce_report};
    use crate::{
        adapters::{CiChecksReport, CiChecksState},
        diff_review::{PullRequestRef, PullRequestState},
    };
    let pr = PullRequestRef {
        number: 42,
        title: "Fix".to_string(),
        url: "https://github.test/pull/42".to_string(),
        state: PullRequestState::Open,
        head_ref: "ajax/fix".to_string(),
        head_sha: Some("aaa".to_string()),
    };
    reduce_report(
        task,
        &pr,
        CiChecksReport {
            state: CiChecksState::Pending,
            failed_checks: vec![],
            check_identities: vec!["run:1".to_string()],
            has_pending: true,
            error: None,
        },
        100,
    );
    let state = load_state(task);
    assert_eq!(
        state.status,
        crate::runtime_refresh::ci_monitor::CiAttemptStatus::Pending
    );
    assert!(!state.saw_pending_after_failure);
    assert!(crate::runtime_refresh::ci_monitor::checks_in_flight(&state));
}

#[test]
fn ci_rerun_in_progress_suppresses_attention_push() {
    let mut task = active_task("ci-rerun");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed: lint"),
        at(1_000),
    );
    seed_ci_monitor_rerun(&mut task);
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        None,
        "stale CiFailed must not phone-ping while checks are still in flight"
    );
}

#[test]
fn ci_settled_failure_notifies_after_poll_records_terminal_state() {
    use crate::runtime_refresh::ci_monitor::{load_state, pending_notification, reduce_report};

    let mut task = active_task("ci-settled");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed: lint"),
        at(1_000),
    );
    seed_ci_monitor_rerun(&mut task);
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        None
    );
    assert!(pending_notification(&task).is_none());

    let pr = crate::diff_review::PullRequestRef {
        number: 42,
        title: "Fix".to_string(),
        url: "https://github.test/pull/42".to_string(),
        state: crate::diff_review::PullRequestState::Open,
        head_ref: "ajax/fix".to_string(),
        head_sha: Some("aaa".to_string()),
    };
    reduce_report(
        &mut task,
        &pr,
        crate::adapters::CiChecksReport {
            state: crate::adapters::CiChecksState::Failed,
            failed_checks: vec![crate::agent_notification::CiFailedCheck {
                name: "lint".to_string(),
                link: Some("https://github.test/runs/2".to_string()),
                identity: Some("run:2".to_string()),
            }],
            check_identities: vec!["run:2".to_string()],
            has_pending: false,
            error: None,
        },
        103,
    );
    assert!(!crate::runtime_refresh::ci_monitor::checks_in_flight(
        &load_state(&task)
    ));
    assert!(pending_notification(&task).is_some());
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "1030".to_string(),
    );
    assert!(
        super::take_attention_transition_at(&mut task, at(1_045)).is_some(),
        "notify after poll records settled failure"
    );
}

#[test]
fn merge_conflict_agent_turn_does_not_suppress_confirmed_conflict() {
    use crate::models::GitStatus;

    let mut task = active_task("merge-rerun");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        at(1_000),
    );
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: true,
        last_commit: Some("abc".to_string()),
    });
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert!(
        super::take_attention_transition_at(&mut task, at(1_015)).is_some(),
        "confirmed merge conflict must phone-ping even while the agent is running"
    );
}

#[test]
fn merge_conflict_first_attempt_pending_ci_does_not_suppress_confirmed_conflict() {
    use crate::models::GitStatus;

    let mut task = active_task("merge-first-pending");
    seed_ci_monitor_first_attempt_pending(&mut task);
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        at(1_000),
    );
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: true,
        last_commit: Some("abc".to_string()),
    });
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert!(
        super::take_attention_transition_at(&mut task, at(1_015)).is_some(),
        "confirmed merge conflict must phone-ping during first-attempt pending CI"
    );
}

#[test]
fn merge_conflict_post_failure_rerun_suppresses_confirmed_conflict() {
    use crate::models::GitStatus;

    let mut task = active_task("merge-rerun-suppress");
    seed_ci_monitor_rerun(&mut task);
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        at(1_000),
    );
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: true,
        last_commit: Some("abc".to_string()),
    });
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        None,
        "post-failure CI rerun must suppress confirmed merge-conflict phone ping"
    );
}

#[test]
fn merge_conflict_unconfirmed_git_status_suppresses_attention_push() {
    let mut task = active_task("merge-stale");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        at(1_000),
    );
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        None,
        "merge conflict must wait for git status poll before phone-ping"
    );
}
