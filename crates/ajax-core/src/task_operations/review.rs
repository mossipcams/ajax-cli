use crate::{
    adapters::{CommandOutput, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan},
    registry::Registry,
    task_operations::kernel::execute_external_plan,
};

pub fn plan_review_operation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    commands::diff_task_plan(context, qualified_handle)
}

pub fn execute_review_operation<R: Registry>(
    _context: &mut CommandContext<R>,
    _qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    let outputs = execute_external_plan(plan, confirmed, runner).map_err(|error| (error, false))?;
    Ok((outputs, false))
}
