use super::{CommandContext, CommandError, CommandPlan};
use crate::registry::Registry;

pub fn diff_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    crate::slices::review::review_task_plan(context, qualified_handle)
}
