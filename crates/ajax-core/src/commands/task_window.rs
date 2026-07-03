use super::lookup::find_task;
use super::{CommandContext, CommandError, CommandPlan, OpenMode};
use crate::{
    adapters::TmuxAdapter,
    live::LiveStatusKind,
    models::{LifecycleStatus, TaskWindowStatus, TmuxStatus},
    registry::Registry,
};

pub fn task_window_repair_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    task_window_repair_plan_with_open_mode(context, qualified_handle, OpenMode::Attach)
}

pub fn task_window_repair_plan_with_open_mode<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let tmux = TmuxAdapter::new("tmux");
    let mut plan = CommandPlan::new(format!("open task: {qualified_handle}"));
    if task.lifecycle_status == LifecycleStatus::Removed {
        plan.blocked_reasons.push("task is removed".to_string());
        return Ok(plan);
    }
    if task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists)
    {
        plan.blocked_reasons.push(format!(
            "task worktree is missing: {}",
            task.worktree_path.display()
        ));
        return Ok(plan);
    }

    let tmux_session_exists = task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists);
    if !tmux_session_exists {
        plan.commands.push(tmux.new_detached_task_session(
            &task.tmux_session,
            &task.task_window,
            &task.worktree_path.display().to_string(),
        ));
    } else if task
        .task_window_status
        .as_ref()
        .is_some_and(|status| status.exists && !status.points_at_expected_path)
    {
        plan.commands
            .push(tmux.kill_window(&task.tmux_session, &task.task_window));
        plan.commands.push(tmux.ensure_task_window(
            &task.tmux_session,
            &task.task_window,
            &task.worktree_path.display().to_string(),
        ));
    } else if task
        .task_window_status
        .as_ref()
        .is_none_or(|status| !status.exists)
    {
        plan.commands.push(tmux.ensure_task_window(
            &task.tmux_session,
            &task.task_window,
            &task.worktree_path.display().to_string(),
        ));
    }
    if matches!(mode, OpenMode::NoAttach) {
        return Ok(plan);
    }

    plan.commands
        .push(tmux.select_window(&task.tmux_session, &task.task_window));
    match mode {
        OpenMode::Attach => plan
            .commands
            .push(tmux.attach_window(&task.tmux_session, &task.task_window)),
        OpenMode::SwitchClient => plan
            .commands
            .push(tmux.switch_client_to_window(&task.tmux_session, &task.task_window)),
        OpenMode::NoAttach => unreachable!("handled above"),
    };

    Ok(plan)
}

pub fn mark_task_window_repaired<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    context
        .registry
        .update_tmux_status(&task.id, Some(TmuxStatus::present(task.tmux_session)))
        .map_err(CommandError::Registry)?;
    context
        .registry
        .update_task_window_status(
            &task.id,
            Some(TaskWindowStatus::present(
                task.task_window,
                task.worktree_path,
            )),
        )
        .map_err(CommandError::Registry)?;
    if let Some(task) = context.registry.get_task_mut(&task.id) {
        if task.live_status.as_ref().is_some_and(|status| {
            matches!(
                status.kind,
                LiveStatusKind::TmuxMissing | LiveStatusKind::TaskWindowMissing
            )
        }) {
            task.live_status = None;
        }
    }
    Ok(())
}
