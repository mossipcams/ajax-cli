use crate::{
    attention::annotate,
    models::{LifecycleStatus, Task},
    output::{TaskSummary, TasksResponse},
    recommended::available_operator_actions,
    registry::Registry,
    use_cases::CommandContext,
};

pub fn review_queue<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    let tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .filter(|task| {
            matches!(
                task.lifecycle_status,
                LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
            )
        })
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

fn task_summary(task: &Task) -> TaskSummary {
    let operator_status = crate::ui_state::derive_operator_status(task);
    TaskSummary {
        id: task.id.as_str().to_string(),
        qualified_handle: task.qualified_handle(),
        title: task.title.clone(),
        lifecycle_status: format!("{:?}", task.lifecycle_status),
        status_label: operator_status.label,
        runtime_observation_error: task.runtime_projection.observation_error.clone(),
        needs_attention: !annotate(task).is_empty(),
        live_status: task.live_status.clone(),
        actions: available_operator_actions(task)
            .into_iter()
            .map(|action| action.as_str().to_string())
            .collect(),
    }
}
