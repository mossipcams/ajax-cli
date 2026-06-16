use crate::{
    adapters::{CommandOutput, CommandRunner},
    capability_policy,
    models::{OperatorAction, Task},
    recommended::{available_built_in_decision, blocked_built_in_decision, TaskActionDecision},
    registry::Registry,
    use_cases::{CommandContext, CommandError, CommandPlan},
};

pub fn decision(task: &Task) -> TaskActionDecision {
    if super::drop::invalid_task_requires_drop(task) {
        return blocked_built_in_decision(OperatorAction::Ship, "task has missing substrate", true);
    }
    capability_policy::ship_blocked_reasons(task)
        .into_iter()
        .next()
        .map(|reason| blocked_built_in_decision(OperatorAction::Ship, reason, true))
        .unwrap_or_else(|| available_built_in_decision(OperatorAction::Ship, "ship", true))
}

pub fn plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    super::task_action::plan_ship(context, qualified_handle)
}

pub fn execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    super::task_action::execute_ship(context, qualified_handle, plan, confirmed, runner)
}
