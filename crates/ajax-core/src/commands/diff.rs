use super::lookup::find_task;
use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::CommandSpec,
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
