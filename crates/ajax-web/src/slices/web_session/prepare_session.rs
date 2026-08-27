//! Resolve whether a task may attach an orchestration session and build the plan.

use super::task_pane_agent::tmux_task_pane_runs_live_agent;
use super::{
    harness_default_model, is_unspecified_model, normalize_session_model, SessionAttachPlan,
    SessionRouteError,
};
use ajax_core::{
    adapters::{acp_launch_for_agent, CommandRunner},
    commands::CommandContext,
    models::TaskId,
    registry::Registry,
};

pub fn prepare_task_session<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
    model: &str,
) -> Result<SessionAttachPlan, SessionRouteError> {
    let task_id = TaskId::new(qualified_handle.to_string());
    let Some(task) = context.registry.get_task(&task_id) else {
        return Err(SessionRouteError::TaskNotFound);
    };

    if acp_launch_for_agent(task.selected_agent).is_none() {
        return Err(SessionRouteError::NotOrchestrationChat);
    }

    if !task.skip_interactive_agent() {
        if tmux_task_pane_runs_live_agent(runner, &task) {
            return Err(SessionRouteError::NotOrchestrationChat);
        }
        let Some(task_mut) = context.registry.get_task_mut(&task_id) else {
            return Err(SessionRouteError::TaskNotFound);
        };
        task_mut.set_skip_interactive_agent(true);
    }

    let task = context
        .registry
        .get_task(&task_id)
        .ok_or(SessionRouteError::TaskNotFound)?;

    if !task.worktree_path.exists() {
        return Err(SessionRouteError::WorktreeMissing);
    }

    let url_model = normalize_session_model(model).ok();
    let model = task
        .session_model()
        .filter(|stored| !is_unspecified_model(Some(stored)))
        .map(str::to_string)
        .or_else(|| {
            url_model
                .filter(|model| model != &super::default_session_model())
                .map(|model| model.to_string())
        })
        .or_else(|| harness_default_model(task.selected_agent).map(str::to_string))
        .unwrap_or_default();

    Ok(SessionAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree_path: task.worktree_path.clone(),
        model,
        agent: task.selected_agent,
    })
}

#[cfg(test)]
#[path = "prepare_session_1092_tests.rs"]
mod prepare_session_1092_tests;
