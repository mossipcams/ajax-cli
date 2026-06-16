use crate::{
    adapters::{CommandOutput, CommandRunner},
    capability_policy,
    models::{OperatorAction, Task},
    recommended::{available_built_in_decision, blocked_built_in_decision, TaskActionDecision},
    registry::Registry,
    use_cases::{CommandContext, CommandError, CommandPlan, OpenMode},
};

pub fn decision(task: &Task) -> TaskActionDecision {
    capability_policy::resume_blocked_reasons(task)
        .into_iter()
        .next()
        .map(|reason| blocked_built_in_decision(OperatorAction::Resume, reason, false))
        .unwrap_or_else(|| available_built_in_decision(OperatorAction::Resume, "resume", false))
}

pub fn plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    super::task_action::plan_resume(context, qualified_handle, open_mode)
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    super::task_action::execute_resume(context, qualified_handle, plan, confirmed, runner)
}
