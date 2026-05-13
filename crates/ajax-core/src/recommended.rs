use crate::{
    models::{
        AgentRuntimeStatus, AttentionItem, LifecycleStatus, LiveStatusKind, RecommendedAction,
        SideFlag, Task, TaskId,
    },
    operation::{task_operation_eligibility, TaskOperation},
    ui_state::{derive_ui_state, UiState},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecommendedActionPlan {
    pub action: RecommendedAction,
    pub reason: String,
    pub available_actions: Vec<RecommendedAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NextStep {
    pub task_id: TaskId,
    pub task_handle: String,
    pub ui_state: UiState,
    pub action: RecommendedAction,
    pub reason: String,
}

pub fn recommended_action(task: &Task) -> RecommendedActionPlan {
    let ui_state = derive_ui_state(task);
    let action = primary_action_for(ui_state);
    let reason = match ui_state {
        UiState::Blocked => primary_blocker_reason(task).unwrap_or("resolve blocker"),
        UiState::Running => "monitor",
        UiState::ReviewReady => "review",
        UiState::SafeMerge => "merge",
        UiState::Cleanable => "clean",
        UiState::Idle => "open",
        UiState::Failed => "recover",
        UiState::Archived => "remove",
    };

    RecommendedActionPlan {
        action,
        reason: reason.to_string(),
        available_actions: available_task_actions(task),
    }
}

pub fn next_recommendation(tasks: &[Task]) -> Option<NextStep> {
    let mut best: Option<(u32, NextStep)> = None;
    for task in tasks {
        let ui_state = derive_ui_state(task);
        let Some(rank) = next_rank(ui_state) else {
            continue;
        };
        let plan = recommended_action(task);
        let candidate = NextStep {
            task_id: task.id.clone(),
            task_handle: task.qualified_handle(),
            ui_state,
            action: plan.action,
            reason: plan.reason,
        };
        match &best {
            Some((current_rank, _)) if *current_rank <= rank => {}
            _ => best = Some((rank, candidate)),
        }
    }
    best.map(|(_, step)| step)
}

fn next_rank(state: UiState) -> Option<u32> {
    match state {
        UiState::Blocked => Some(0),
        UiState::SafeMerge => Some(1),
        UiState::ReviewReady => Some(2),
        UiState::Cleanable => Some(3),
        UiState::Failed => Some(4),
        UiState::Running | UiState::Idle | UiState::Archived => None,
    }
}

fn primary_action_for(state: UiState) -> RecommendedAction {
    match state {
        UiState::SafeMerge => RecommendedAction::MergeTask,
        UiState::Cleanable => RecommendedAction::CleanTask,
        UiState::Archived => RecommendedAction::RemoveTask,
        _ => RecommendedAction::OpenTask,
    }
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

pub fn available_task_actions(task: &Task) -> Vec<RecommendedAction> {
    if task.has_side_flag(SideFlag::TmuxMissing) || task.has_side_flag(SideFlag::WorktrunkMissing) {
        return vec![RecommendedAction::OpenTask];
    }
    [
        (TaskOperation::Open, RecommendedAction::OpenTask),
        (TaskOperation::Merge, RecommendedAction::MergeTask),
        (TaskOperation::Clean, RecommendedAction::CleanTask),
        (TaskOperation::Remove, RecommendedAction::RemoveTask),
    ]
    .into_iter()
    .filter(|(op, _)| task_operation_eligibility(task, *op).is_allowed())
    .map(|(_, action)| action)
    .collect()
}

pub fn opportunity_attention_for(task: &Task) -> Option<AttentionItem> {
    let state = derive_ui_state(task);
    let (reason, priority) = match state {
        UiState::SafeMerge => ("safe to merge", 50_u32),
        UiState::ReviewReady => ("ready for review", 55_u32),
        UiState::Cleanable if task.lifecycle_status != LifecycleStatus::Cleanable => {
            ("safe to clean", 60_u32)
        }
        _ => return None,
    };
    let action = primary_action_for(state);
    Some(AttentionItem {
        task_id: task.id.clone(),
        task_handle: task.qualified_handle(),
        reason: reason.to_string(),
        priority,
        recommended_action: action.as_str().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::{next_recommendation, recommended_action, RecommendedActionPlan};
    use crate::{
        lifecycle::{mark_active, mark_cleanable, mark_mergeable, mark_merged, mark_reviewable},
        models::{AgentClient, AgentRuntimeStatus, RecommendedAction, SideFlag, Task, TaskId},
        ui_state::UiState,
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
    fn blocked_task_recommends_open_with_blocker_reason() {
        let mut t = task("blocked");
        mark_active(&mut t).unwrap();
        t.add_side_flag(SideFlag::NeedsInput);

        let plan = recommended_action(&t);

        assert_eq!(plan.action, RecommendedAction::OpenTask);
        assert_eq!(plan.reason, "agent needs input");
    }

    #[test]
    fn mergeable_task_recommends_merge() {
        let mut t = task("mergeable");
        mark_active(&mut t).unwrap();
        mark_reviewable(&mut t).unwrap();
        mark_mergeable(&mut t).unwrap();

        let plan = recommended_action(&t);

        assert_eq!(plan.action, RecommendedAction::MergeTask);
        assert_eq!(plan.reason, "merge");
    }

    #[test]
    fn cleanable_task_recommends_clean() {
        let mut t = task("clean");
        mark_active(&mut t).unwrap();
        mark_reviewable(&mut t).unwrap();
        mark_merged(&mut t).unwrap();
        mark_cleanable(&mut t).unwrap();

        let plan = recommended_action(&t);

        assert_eq!(plan.action, RecommendedAction::CleanTask);
        assert_eq!(plan.reason, "clean");
    }

    #[test]
    fn idle_task_recommends_open() {
        let mut t = task("idle");
        mark_active(&mut t).unwrap();

        let plan: RecommendedActionPlan = recommended_action(&t);

        assert_eq!(plan.action, RecommendedAction::OpenTask);
        assert_eq!(plan.reason, "open");
    }

    #[test]
    fn next_recommendation_prefers_blocked_over_other_states() {
        let mut blocked = task("blocked");
        mark_active(&mut blocked).unwrap();
        blocked.add_side_flag(SideFlag::NeedsInput);

        let mut mergeable = task("mergeable");
        mark_active(&mut mergeable).unwrap();
        mark_reviewable(&mut mergeable).unwrap();
        mark_mergeable(&mut mergeable).unwrap();

        let mut cleanable = task("cleanable");
        mark_active(&mut cleanable).unwrap();
        mark_reviewable(&mut cleanable).unwrap();
        mark_merged(&mut cleanable).unwrap();
        mark_cleanable(&mut cleanable).unwrap();

        let next = next_recommendation(&[cleanable, mergeable, blocked]).unwrap();

        assert_eq!(next.ui_state, UiState::Blocked);
    }

    #[test]
    fn next_recommendation_falls_back_to_safe_merge_then_review_then_clean() {
        let mut mergeable = task("mergeable");
        mark_active(&mut mergeable).unwrap();
        mark_reviewable(&mut mergeable).unwrap();
        mark_mergeable(&mut mergeable).unwrap();

        let mut cleanable = task("cleanable");
        mark_active(&mut cleanable).unwrap();
        mark_reviewable(&mut cleanable).unwrap();
        mark_merged(&mut cleanable).unwrap();
        mark_cleanable(&mut cleanable).unwrap();

        let next = next_recommendation(&[cleanable.clone(), mergeable]).unwrap();
        assert_eq!(next.ui_state, UiState::SafeMerge);

        let next = next_recommendation(&[cleanable]).unwrap();
        assert_eq!(next.ui_state, UiState::Cleanable);
    }

    #[test]
    fn next_recommendation_returns_none_when_nothing_actionable() {
        let mut idle = task("idle");
        mark_active(&mut idle).unwrap();
        idle.agent_status = AgentRuntimeStatus::Running;
        idle.add_side_flag(SideFlag::AgentRunning);

        assert!(next_recommendation(&[idle]).is_none());
    }
}
