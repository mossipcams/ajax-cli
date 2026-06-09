use crate::{
    models::{
        AgentRuntimeStatus, Annotation, Evidence, LifecycleStatus, LiveStatusKind, OperatorAction,
        SideFlag, Task,
    },
    operation::{task_operation_eligibility, TaskOperation},
    ui_state::{derive_operator_status, derive_ui_state, OperatorStatusKind, UiState},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorActionPlan {
    pub action: OperatorAction,
    pub reason: String,
    pub available_actions: Vec<OperatorAction>,
}

pub fn operator_action(task: &Task) -> OperatorActionPlan {
    if task_is_known_invalid(task) {
        return OperatorActionPlan {
            action: OperatorAction::Drop,
            reason: "invalid_task".to_string(),
            available_actions: available_operator_actions(task),
        };
    }

    let derived_annotations = crate::attention::annotate(task);
    let annotations = derived_annotations.as_slice();
    let primary_annotation = annotations
        .iter()
        .min_by_key(|annotation| annotation.severity);
    let action = primary_annotation
        .map(|annotation| annotation.suggests)
        .unwrap_or_else(|| fallback_operator_action(task));
    let reason = primary_annotation
        .map(annotation_reason)
        .unwrap_or_else(|| fallback_operator_reason(task).to_string());

    OperatorActionPlan {
        action,
        reason,
        available_actions: available_operator_actions(task),
    }
}

fn annotation_reason(annotation: &Annotation) -> String {
    evidence_label(&annotation.evidence).to_string()
}

pub(crate) fn evidence_label(evidence: &Evidence) -> &'static str {
    match evidence {
        Evidence::LiveStatus(status) => match status {
            LiveStatusKind::WaitingForApproval => "waiting_for_approval",
            LiveStatusKind::WaitingForInput => "waiting_for_input",
            LiveStatusKind::AuthRequired => "auth_required",
            LiveStatusKind::RateLimited => "rate_limited",
            LiveStatusKind::ContextLimit => "context_limit",
            LiveStatusKind::CommandFailed => "command_failed",
            LiveStatusKind::Blocked => "blocked",
            LiveStatusKind::WorktreeMissing => "worktree_missing",
            LiveStatusKind::TmuxMissing => "tmux_missing",
            LiveStatusKind::WorktrunkMissing => "worktrunk_missing",
            LiveStatusKind::MergeConflict => "merge_conflict",
            LiveStatusKind::CiFailed => "ci_failed",
            LiveStatusKind::Done => "done",
            LiveStatusKind::ShellIdle
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::Unknown => "live_status",
        },
        Evidence::AgentStatus(status) => match status {
            AgentRuntimeStatus::NotStarted => "agent_not_started",
            AgentRuntimeStatus::Running => "agent_running",
            AgentRuntimeStatus::Waiting => "agent_waiting",
            AgentRuntimeStatus::Blocked => "agent_blocked",
            AgentRuntimeStatus::Done => "agent_done",
            AgentRuntimeStatus::Dead => "agent_dead",
            AgentRuntimeStatus::Unknown => "agent_status_not_observed",
        },
        Evidence::SideFlag(flag) => match flag {
            SideFlag::Dirty => "dirty",
            SideFlag::AgentRunning => "agent_running",
            SideFlag::AgentDead => "agent_dead",
            SideFlag::NeedsInput => "needs_input",
            SideFlag::TestsFailed => "tests_failed",
            SideFlag::TmuxMissing => "tmux_missing",
            SideFlag::WorktreeMissing => "worktree_missing",
            SideFlag::WorktrunkMissing => "worktrunk_missing",
            SideFlag::BranchMissing => "branch_missing",
            SideFlag::Stale => "stale",
            SideFlag::Conflicted => "conflicted",
            SideFlag::Unpushed => "unpushed",
        },
        Evidence::Lifecycle(status) => match status {
            LifecycleStatus::Created => "created",
            LifecycleStatus::Provisioning => "provisioning",
            LifecycleStatus::Active => "active",
            LifecycleStatus::Waiting => "waiting",
            LifecycleStatus::Reviewable => "reviewable",
            LifecycleStatus::Mergeable => "mergeable",
            LifecycleStatus::Merged => "merged",
            LifecycleStatus::Cleanable => "cleanable",
            LifecycleStatus::Removing => "removing",
            LifecycleStatus::TeardownIncomplete => "teardown incomplete",
            LifecycleStatus::Removed => "removed",
            LifecycleStatus::Orphaned => "orphaned",
            LifecycleStatus::Error => "error",
        },
        Evidence::Substrate(gap) => match gap {
            crate::models::SubstrateGap::WorktreeMissing => "worktree_missing",
            crate::models::SubstrateGap::TmuxMissing => "tmux_missing",
            crate::models::SubstrateGap::WorktrunkMissing => "worktrunk_missing",
            crate::models::SubstrateGap::BranchMissing => "branch_missing",
        },
        Evidence::RuntimeObservationFailed => "runtime_observation_failed",
    }
}

fn fallback_operator_action(task: &Task) -> OperatorAction {
    if derive_operator_status(task).kind == OperatorStatusKind::ObservationFailed {
        return OperatorAction::Repair;
    }
    match derive_ui_state(task) {
        UiState::SafeMerge => OperatorAction::Ship,
        UiState::Cleanable | UiState::Archived => OperatorAction::Drop,
        UiState::ReviewReady => OperatorAction::Review,
        UiState::Blocked
        | UiState::NeedsInput
        | UiState::Running
        | UiState::Idle
        | UiState::Failed => OperatorAction::Resume,
    }
}

fn fallback_operator_reason(task: &Task) -> &'static str {
    match derive_ui_state(task) {
        UiState::Blocked => primary_blocker_reason(task).unwrap_or("resolve_blocker"),
        UiState::NeedsInput => "needs_input",
        UiState::Running => "monitor",
        UiState::ReviewReady => "review",
        UiState::SafeMerge => "ship",
        UiState::Cleanable => "drop",
        UiState::Idle => "resume",
        UiState::Failed => "repair",
        UiState::Archived => "drop",
    }
}

pub fn available_operator_actions(task: &Task) -> Vec<OperatorAction> {
    if task_is_known_invalid(task) {
        return vec![OperatorAction::Drop];
    }

    if task.has_missing_substrate() && !has_only_shell_substrate_gap(task) {
        return vec![OperatorAction::Repair];
    }

    let mut actions = if task.has_missing_substrate()
        || derive_operator_status(task).kind == OperatorStatusKind::ObservationFailed
    {
        vec![OperatorAction::Repair]
    } else {
        Vec::new()
    };
    actions.extend(
        [
            (TaskOperation::Open, OperatorAction::Resume),
            (TaskOperation::Merge, OperatorAction::Ship),
            (TaskOperation::Clean, OperatorAction::Drop),
            (TaskOperation::Remove, OperatorAction::Drop),
        ]
        .into_iter()
        .filter(|(op, _)| task_operation_eligibility(task, *op).is_allowed())
        .map(|(_, action)| action),
    );
    actions.dedup();
    actions
}

fn has_only_shell_substrate_gap(task: &Task) -> bool {
    !task.has_side_flag(SideFlag::WorktreeMissing)
        && !task.has_side_flag(SideFlag::BranchMissing)
        && !task.runtime_projection.health.is_git_substrate_gap()
        && !task
            .live_status
            .as_ref()
            .is_some_and(|live| live.kind == LiveStatusKind::WorktreeMissing)
}

fn task_is_known_invalid(task: &Task) -> bool {
    task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::WorktrunkMissing)
        || task.has_side_flag(SideFlag::WorktreeMissing)
        || task.has_side_flag(SideFlag::BranchMissing)
        || task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists)
        || task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists || !status.branch_exists)
        || task
            .worktrunk_status
            .as_ref()
            .is_some_and(|status| !status.exists || !status.points_at_expected_path)
        || task.live_status.as_ref().is_some_and(|live| {
            matches!(
                live.kind,
                LiveStatusKind::WorktreeMissing
                    | LiveStatusKind::TmuxMissing
                    | LiveStatusKind::WorktrunkMissing
            )
        })
}

pub fn primary_blocker_reason(task: &Task) -> Option<&'static str> {
    if let Some(live) = task.live_status.as_ref() {
        if let Some(reason) = blocker_reason_for_live(live.kind) {
            return Some(reason);
        }
    }
    if task.has_side_flag(SideFlag::NeedsInput) {
        return Some("agent needs input");
    }
    if task.has_side_flag(SideFlag::Conflicted) {
        return Some("git conflicts detected");
    }
    if task.has_side_flag(SideFlag::TestsFailed) {
        return Some("tests failed");
    }
    if task.has_side_flag(SideFlag::AgentDead) {
        return Some("agent appears dead");
    }
    if task.has_side_flag(SideFlag::WorktrunkMissing) {
        return Some("worktrunk missing");
    }
    if task.has_side_flag(SideFlag::TmuxMissing) {
        return Some("tmux session missing");
    }
    if task.has_side_flag(SideFlag::WorktreeMissing) {
        return Some("worktree missing");
    }
    if task.has_side_flag(SideFlag::BranchMissing) {
        return Some("branch missing");
    }
    match task.agent_status {
        AgentRuntimeStatus::Waiting => Some("agent is waiting"),
        AgentRuntimeStatus::Blocked => Some("agent is blocked"),
        AgentRuntimeStatus::Dead => Some("agent appears dead"),
        _ => None,
    }
}

fn blocker_reason_for_live(kind: LiveStatusKind) -> Option<&'static str> {
    match kind {
        LiveStatusKind::WaitingForApproval => Some("waiting for approval"),
        LiveStatusKind::WaitingForInput => Some("waiting for input"),
        LiveStatusKind::AuthRequired => Some("authentication required"),
        LiveStatusKind::RateLimited => Some("rate limited"),
        LiveStatusKind::ContextLimit => Some("context limit reached"),
        LiveStatusKind::MergeConflict => Some("merge conflict needs attention"),
        LiveStatusKind::CommandFailed => Some("command failed"),
        LiveStatusKind::CiFailed => Some("ci failed"),
        LiveStatusKind::Blocked => Some("agent is blocked"),
        LiveStatusKind::WorktreeMissing => Some("worktree missing"),
        LiveStatusKind::TmuxMissing => Some("tmux session missing"),
        LiveStatusKind::WorktrunkMissing => Some("worktrunk missing"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::operator_action;
    use crate::{
        lifecycle::{mark_active, mark_reviewable},
        models::{
            AgentClient, GitStatus, LifecycleStatus, OperatorAction, RuntimeHealth,
            RuntimeObservationSource, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
    };

    fn task(handle: &str) -> Task {
        Task::new(
            TaskId::new(format!("task-{handle}")),
            "web",
            handle,
            format!("Task {handle}"),
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/{handle}"),
            format!("ajax-web-{handle}"),
            "worktrunk",
            AgentClient::Codex,
        )
    }

    fn clean_reviewable_task(handle: &str) -> Task {
        let mut t = task(handle);
        mark_active(&mut t).unwrap();
        mark_reviewable(&mut t).unwrap();
        t.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some(format!("ajax/{handle}")),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123 Fix task".to_string()),
        });
        t
    }

    #[test]
    fn operator_actions_prefer_drop_when_shell_substrate_is_missing() {
        for flag in [SideFlag::TmuxMissing, SideFlag::WorktrunkMissing] {
            let mut t = clean_reviewable_task("reviewable");
            t.add_side_flag(flag);

            let plan = operator_action(&t);

            assert_eq!(plan.action, OperatorAction::Drop);
            assert_eq!(plan.available_actions, vec![OperatorAction::Drop]);
        }
    }

    #[test]
    fn operator_actions_hide_ship_when_git_substrate_is_missing() {
        for mark_missing_git_substrate in [
            |task: &mut Task| task.git_status.as_mut().unwrap().worktree_exists = false,
            |task: &mut Task| task.git_status.as_mut().unwrap().branch_exists = false,
        ] {
            let mut t = clean_reviewable_task("reviewable");
            mark_missing_git_substrate(&mut t);

            let plan = operator_action(&t);

            assert!(
                !plan.available_actions.contains(&OperatorAction::Ship),
                "missing git substrate should hide ship: {:?}",
                plan.available_actions
            );
        }
    }

    #[test]
    fn runtime_probe_failure_recommends_repair_instead_of_drop() {
        let mut t = task("probe-failed");
        mark_active(&mut t).unwrap();
        t.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Repair);
        assert_eq!(plan.reason, "runtime_observation_failed");
        assert!(plan.available_actions.contains(&OperatorAction::Repair));
        assert_ne!(plan.action, OperatorAction::Drop);
    }

    #[test]
    fn invalid_tasks_prefer_drop_for_removal() {
        for make_invalid in [
            |task: &mut Task| task.add_side_flag(SideFlag::TmuxMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::WorktrunkMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::WorktreeMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::BranchMissing),
            |task: &mut Task| {
                task.tmux_status = Some(TmuxStatus {
                    exists: false,
                    session_name: task.tmux_session.clone(),
                });
            },
            |task: &mut Task| {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                });
            },
            |task: &mut Task| {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: true,
                    window_name: task.worktrunk_window.clone(),
                    current_path: "/tmp/wrong".into(),
                    points_at_expected_path: false,
                });
            },
            |task: &mut Task| task.git_status.as_mut().unwrap().worktree_exists = false,
            |task: &mut Task| task.git_status.as_mut().unwrap().branch_exists = false,
        ] {
            let mut t = clean_reviewable_task("reviewable");
            make_invalid(&mut t);

            let plan = operator_action(&t);

            assert_eq!(plan.action, OperatorAction::Drop);
            assert!(
                plan.available_actions.contains(&OperatorAction::Drop),
                "invalid task should stay removable: {:?}",
                plan.available_actions
            );
            assert!(
                !plan.available_actions.contains(&OperatorAction::Ship),
                "invalid task should not offer ship: {:?}",
                plan.available_actions
            );
        }
    }

    #[test]
    fn operator_action_prefers_runtime_health_for_shell_repair_without_hiding_ship() {
        let mut t = clean_reviewable_task("reviewable");
        t.runtime_projection.health = RuntimeHealth::MissingSession;

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Repair);
        assert_eq!(plan.reason, "tmux_missing");
        assert!(
            plan.available_actions.contains(&OperatorAction::Repair),
            "runtime health should offer repair: {:?}",
            plan.available_actions
        );
        assert!(
            plan.available_actions.contains(&OperatorAction::Ship),
            "shell runtime health should not hide ship: {:?}",
            plan.available_actions
        );
    }

    #[test]
    fn operator_action_uses_lowest_severity_annotation() {
        let mut t = task("annotated");
        t.lifecycle_status = LifecycleStatus::Reviewable;
        t.add_side_flag(SideFlag::NeedsInput);

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Resume);
        assert_eq!(plan.reason, "needs_input");
    }
}
