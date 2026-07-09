use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::CommandSpec,
    models::Task,
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    registry::Registry,
};

pub fn diff_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("diff task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Diff)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let range = format!("{}...{}", task.base_branch, task.branch);
    plan.commands.push(
        CommandSpec::new("git", ["diff", "--stat", range.as_str()])
            .with_cwd(task.worktree_path.display().to_string()),
    );

    Ok(plan)
}

fn find_task<'a, R: Registry>(
    context: &'a CommandContext<R>,
    qualified_handle: &str,
) -> Result<&'a Task, CommandError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))
}
