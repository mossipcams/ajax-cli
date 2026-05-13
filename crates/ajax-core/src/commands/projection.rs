use crate::{
    attention::derive_attention_items,
    models::{LifecycleStatus, RecommendedAction, SideFlag, Task},
    operation::{task_operation_eligibility, TaskOperation},
    output::{
        CockpitNextStep, CockpitProjection, CockpitSummary, InboxResponse, ReposResponse, TaskCard,
        TaskSummary, TasksResponse,
    },
    recommended::{next_recommendation, recommended_action},
    ui_state::derive_ui_state,
};

pub(super) fn cockpit_summary(
    repos: &ReposResponse,
    tasks: &TasksResponse,
    review: &TasksResponse,
    inbox: &InboxResponse,
) -> CockpitSummary {
    CockpitSummary {
        repos: repos.repos.len() as u32,
        tasks: tasks.tasks.len() as u32,
        active_tasks: repos.repos.iter().map(|repo| repo.active_tasks).sum(),
        attention_items: inbox.items.len() as u32,
        reviewable_tasks: review.tasks.len() as u32,
        cleanable_tasks: repos.repos.iter().map(|repo| repo.cleanable_tasks).sum(),
    }
}

pub(super) fn count_lifecycle(tasks: &[&Task], status: LifecycleStatus) -> u32 {
    tasks
        .iter()
        .filter(|task| task.lifecycle_status == status)
        .count() as u32
}

pub(super) fn count_active_tasks(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .filter(|task| {
            task.lifecycle_status == LifecycleStatus::Active && !task.has_missing_substrate()
        })
        .count() as u32
}

pub(super) fn count_attention_items(tasks: &[&Task]) -> u32 {
    tasks
        .iter()
        .map(|task| derive_attention_items(std::slice::from_ref(*task)).len() as u32)
        .sum()
}

pub(super) fn is_visible_task(task: &Task) -> bool {
    task.lifecycle_status != LifecycleStatus::Removed
}

pub(super) fn task_summary(task: &Task) -> TaskSummary {
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        needs_attention: !derive_attention_items(std::slice::from_ref(task)).is_empty(),
        live_status: task.live_status.clone(),
        actions: task_actions(task),
    }
}

fn task_actions(task: &Task) -> Vec<String> {
    if task.has_side_flag(SideFlag::TmuxMissing) || task.has_side_flag(SideFlag::WorktrunkMissing) {
        return vec![RecommendedAction::OpenTask.as_str().to_string()];
    }

    [
        (TaskOperation::Open, RecommendedAction::OpenTask),
        (TaskOperation::Merge, RecommendedAction::MergeTask),
        (TaskOperation::Clean, RecommendedAction::CleanTask),
        (TaskOperation::Remove, RecommendedAction::RemoveTask),
    ]
    .into_iter()
    .filter(|(operation, _)| task_operation_eligibility(task, *operation).is_allowed())
    .map(|(_, action)| action.as_str().to_string())
    .collect()
}

pub(super) fn task_card(task: &Task) -> TaskCard {
    let ui_state = derive_ui_state(task);
    let plan = recommended_action(task);
    let blocker_reason = if matches!(ui_state, crate::ui_state::UiState::Blocked) {
        Some(plan.reason.clone())
    } else {
        None
    };
    TaskCard {
        id: task.id.clone(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        ui_state,
        lifecycle: task.lifecycle_status,
        recommended_action: plan.action,
        action_reason: plan.reason,
        available_actions: plan.available_actions,
        live_summary: task.live_status.as_ref().map(|live| live.summary.clone()),
        blocker_reason,
    }
}

pub(super) fn cockpit_projection(
    tasks: &[&Task],
    summary: CockpitSummary,
    attention: Vec<crate::models::AttentionItem>,
) -> CockpitProjection {
    let visible: Vec<&Task> = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect();
    let cards = visible.iter().copied().map(task_card).collect();
    let owned: Vec<Task> = visible.iter().copied().cloned().collect();
    let next = next_recommendation(&owned).map(|step| CockpitNextStep {
        task_id: step.task_id,
        task_handle: step.task_handle,
        ui_state: step.ui_state,
        action: step.action,
        reason: step.reason,
    });
    CockpitProjection {
        counts: summary,
        cards,
        attention,
        next,
    }
}
