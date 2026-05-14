use crate::{
    models::{
        AgentRuntimeStatus, Annotation, Evidence, LifecycleStatus, LiveStatusKind, OperatorAction,
        SideFlag, Task,
    },
    operation::{task_operation_eligibility, TaskOperation},
    ui_state::{derive_ui_state, UiState},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorActionPlan {
    pub action: OperatorAction,
    pub reason: String,
    pub available_actions: Vec<OperatorAction>,
}

pub fn operator_action(task: &Task) -> OperatorActionPlan {
    let derived_annotations;
    let annotations = if task.annotations.is_empty() {
        derived_annotations = crate::attention::annotate(task);
        derived_annotations.as_slice()
    } else {
        task.annotations.as_slice()
    };
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
            LiveStatusKind::Done => "done",
            LiveStatusKind::ShellIdle
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::CiFailed
            | LiveStatusKind::Unknown => "live_status",
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
    }
}

fn fallback_operator_action(task: &Task) -> OperatorAction {
    match derive_ui_state(task) {
        UiState::SafeMerge => OperatorAction::Ship,
        UiState::Cleanable | UiState::Archived => OperatorAction::Drop,
        UiState::ReviewReady => OperatorAction::Review,
        UiState::Blocked | UiState::Running | UiState::Idle | UiState::Failed => {
            OperatorAction::Resume
        }
    }
}

fn fallback_operator_reason(task: &Task) -> &'static str {
    match derive_ui_state(task) {
        UiState::Blocked => primary_blocker_reason(task).unwrap_or("resolve_blocker"),
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
    if task.has_missing_substrate() {
        return vec![OperatorAction::Repair];
    }
    [
        (TaskOperation::Open, OperatorAction::Resume),
        (TaskOperation::Merge, OperatorAction::Ship),
        (TaskOperation::Clean, OperatorAction::Drop),
        (TaskOperation::Remove, OperatorAction::Drop),
    ]
    .into_iter()
    .filter(|(op, _)| task_operation_eligibility(task, *op).is_allowed())
    .map(|(_, action)| action)
    .collect()
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
    use crate::models::{
        AgentClient, Annotation, AnnotationKind, Evidence, LifecycleStatus, OperatorAction,
        SideFlag, Task, TaskId,
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

    #[test]
    fn operator_action_uses_lowest_severity_annotation() {
        let mut t = task("annotated");
        t.annotations = vec![
            Annotation::new(
                AnnotationKind::Reviewable,
                Evidence::Lifecycle(LifecycleStatus::Reviewable),
            ),
            Annotation::new(
                AnnotationKind::NeedsMe,
                Evidence::SideFlag(SideFlag::NeedsInput),
            ),
        ];

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Resume);
        assert_eq!(plan.reason, "needs_input");
    }
}
