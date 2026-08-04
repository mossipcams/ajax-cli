use crate::{
    adapters::{CommandOutput, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan, OpenMode},
    registry::Registry,
    task_operations::kernel::execute_external_plan,
};

pub fn plan_resume_operation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    commands::open_task_plan(context, qualified_handle, open_mode)
}

pub fn execute_resume_operation<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    let outputs = execute_external_plan(plan, confirmed, runner).map_err(|error| (error, false))?;
    commands::mark_task_opened(context, qualified_handle).map_err(|error| (error, false))?;
    Ok((outputs, true))
}
