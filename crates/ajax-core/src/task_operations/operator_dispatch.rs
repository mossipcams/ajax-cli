use crate::{
    adapters::{CommandOutput, CommandRunner},
    commands::{CommandContext, CommandError, CommandPlan, OpenMode},
    registry::Registry,
    task_operations::{repair, resume, review, ship},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskCommandKind {
    Resume,
    Review,
    Repair,
    Ship,
}

pub fn plan_task_command_operation<R: Registry>(
    context: &CommandContext<R>,
    kind: TaskCommandKind,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    match kind {
        TaskCommandKind::Resume => {
            resume::plan_resume_operation(context, qualified_handle, open_mode)
        }
        TaskCommandKind::Review => review::plan_review_operation(context, qualified_handle),
        TaskCommandKind::Repair => {
            repair::plan_repair_operation(context, qualified_handle, open_mode)
        }
        TaskCommandKind::Ship => ship::plan_ship_operation(context, qualified_handle),
    }
}

pub fn execute_task_command_operation<R: Registry>(
    context: &mut CommandContext<R>,
    kind: TaskCommandKind,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    match kind {
        TaskCommandKind::Resume => {
            resume::execute_resume_operation(context, qualified_handle, plan, confirmed, runner)
        }
        TaskCommandKind::Review => {
            review::execute_review_operation(context, qualified_handle, plan, confirmed, runner)
        }
        TaskCommandKind::Repair => {
            repair::execute_repair_operation(context, qualified_handle, plan, confirmed, runner)
        }
        TaskCommandKind::Ship => {
            ship::execute_ship_operation(context, qualified_handle, plan, confirmed, runner)
        }
    }
}
