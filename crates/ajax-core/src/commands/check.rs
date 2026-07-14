use super::lookup::find_task;
use super::task_state;
use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::CommandSpec,
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    registry::Registry,
};

const WORKTREE_MISSING_REASON: &str = "task worktree is missing";

pub fn check_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("check task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Check)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let Some(test_command) = context
        .config
        .test_commands
        .iter()
        .find(|test_command| test_command.repo == task.repo)
    else {
        return Err(CommandError::PlanBlocked(vec![format!(
            "no test command configured for repo {}",
            task.repo
        )]));
    };
    plan.commands.push(
        CommandSpec::new("sh", ["-lc", test_command.command.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

pub(crate) fn check_task_plan_after_worktree_recreate<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("check task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Check)
    {
        plan.blocked_reasons = reasons
            .into_iter()
            .filter(|reason| reason != WORKTREE_MISSING_REASON)
            .collect();
        if !plan.blocked_reasons.is_empty() {
            return Ok(plan);
        }
    }

    let Some(test_command) = context
        .config
        .test_commands
        .iter()
        .find(|test_command| test_command.repo == task.repo)
    else {
        return Err(CommandError::PlanBlocked(vec![format!(
            "no test command configured for repo {}",
            task.repo
        )]));
    };
    plan.commands.push(
        CommandSpec::new("sh", ["-lc", test_command.command.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

pub fn mark_task_check_started<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    task_state::mark_task_check_started(&mut context.registry, &task.id)
        .map_err(CommandError::Registry)?;
    Ok(())
}

pub fn mark_task_check_succeeded<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    if matches!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Active | crate::models::LifecycleStatus::Waiting
    ) {
        task_state::update_check_lifecycle(&mut context.registry, &task.id)
            .map_err(CommandError::Registry)?;
    }
    task_state::mark_task_check_succeeded(&mut context.registry, &task.id)
        .map_err(CommandError::Registry)?;
    Ok(())
}

pub fn mark_task_check_failed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    task_state::mark_task_check_failed(&mut context.registry, &task.id)
        .map_err(CommandError::Registry)?;
    Ok(())
}
