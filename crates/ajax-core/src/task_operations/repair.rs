use crate::{
    adapters::{CommandOutput, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan, OpenMode},
    registry::Registry,
    task_operations::kernel::execute_external_plan,
};

pub fn plan_repair_operation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;

    // Refresh follows renamed branches, so a remaining mismatch is the base
    // branch or a detached HEAD: nothing repair can safely pick for the operator.
    if task.has_checkout_mismatch() && !task.has_missing_worktree() {
        let mut plan = CommandPlan::new(format!("repair task: {qualified_handle}"));
        plan.blocked_reasons.push(format!(
            "{}; check out a task branch and refresh",
            task.checkout_mismatch_explanation().unwrap_or_default()
        ));
        return Ok(plan);
    }

    let mut plan =
        commands::task_window_repair_plan_with_open_mode(context, qualified_handle, open_mode)?;
    plan.title = format!("repair task: {qualified_handle}");
    let recreates_worktree = plan
        .commands
        .iter()
        .any(commands::is_git_worktree_add_command);
    let check_plan = if recreates_worktree {
        commands::check_task_plan_after_worktree_recreate(context, qualified_handle)
    } else {
        commands::check_task_plan(context, qualified_handle)
    };
    if let Ok(check_plan) = check_plan {
        plan.commands.extend(check_plan.commands);
        plan.requires_confirmation |= check_plan.requires_confirmation;
        plan.blocked_reasons.extend(check_plan.blocked_reasons);
    }
    Ok(plan)
}

pub fn execute_repair_operation<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    commands::mark_task_check_started(context, qualified_handle).map_err(|error| (error, false))?;
    let outputs = match execute_external_plan(plan, confirmed, runner) {
        Ok(execution) => execution,
        Err(error) => {
            commands::mark_task_check_failed(context, qualified_handle)
                .map_err(|mark_error| (mark_error, true))?;
            return Err((error, true));
        }
    };
    commands::mark_task_window_repaired(context, qualified_handle)
        .map_err(|error| (error, true))?;
    commands::mark_task_check_succeeded(context, qualified_handle)
        .map_err(|error| (error, true))?;
    Ok((outputs, true))
}
