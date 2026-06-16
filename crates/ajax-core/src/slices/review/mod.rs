mod decision;
mod planning;
mod queue;

use crate::{
    adapters::{CommandOutput, CommandRunner},
    registry::Registry,
    use_cases::{CommandContext, CommandError, CommandPlan},
};

pub use decision::decision;
pub use planning::review_task_plan;
pub use queue::review_queue;

pub fn plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    review_task_plan(context, qualified_handle)
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    super::task_action::execute_review(context, qualified_handle, plan, confirmed, runner)
}
