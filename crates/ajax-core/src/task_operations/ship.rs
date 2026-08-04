use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan},
    registry::Registry,
};

pub fn plan_ship_operation<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    commands::merge_task_plan(context, qualified_handle)
}

pub fn execute_ship_operation<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    let plan = refresh_ship_plan_before_execute(context, qualified_handle, plan, confirmed, runner)
        .map_err(|error| (error, false))?;
    execute_ship_plan(context, &plan, confirmed, runner, qualified_handle)
}

fn refresh_ship_plan_before_execute<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<CommandPlan, CommandError> {
    if !plan.blocked_reasons.is_empty() {
        return Ok(plan.clone());
    }
    if plan.requires_confirmation && !confirmed {
        return Ok(plan.clone());
    }
    let has_cached_git = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .is_some_and(|task| task.git_status.is_some());
    if !has_cached_git {
        return Ok(plan.clone());
    }

    commands::refresh_git_evidence(context, qualified_handle, runner, false)?;
    plan_ship_operation(context, qualified_handle)
}

fn execute_ship_plan<R: Registry>(
    context: &mut CommandContext<R>,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    if !plan.blocked_reasons.is_empty() {
        return Err((
            CommandError::PlanBlocked(plan.blocked_reasons.clone()),
            false,
        ));
    }
    if plan.requires_confirmation && !confirmed {
        return Err((CommandError::ConfirmationRequired, false));
    }

    let mut outputs = Vec::new();
    for (index, command) in plan.commands.iter().enumerate() {
        let output = runner
            .run(command)
            .map_err(|error| (CommandError::CommandRun(error), false))?;
        if output.status_code != 0 {
            let error = CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr.clone(),
                cwd: command.cwd.clone(),
            });
            let state_changed = if index > 0 {
                commands::mark_task_merge_failed(
                    context,
                    qualified_handle,
                    merge_error_looks_conflicted(&error),
                )
                .map_err(|mark_error| (mark_error, true))?;
                true
            } else {
                false
            };
            return Err((error, state_changed));
        }
        outputs.push(output);
    }

    commands::mark_task_merged(context, qualified_handle).map_err(|error| (error, false))?;
    Ok((outputs, true))
}

fn merge_error_looks_conflicted(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::CommandRun(error) if command_run_error_looks_conflicted(error)
    )
}

fn command_run_error_looks_conflicted(error: &CommandRunError) -> bool {
    match error {
        CommandRunError::NonZeroExit { stderr, .. } => {
            stderr.to_ascii_lowercase().contains("conflict")
        }
        CommandRunError::SpawnFailed(_)
        | CommandRunError::MissingStatusCode
        | CommandRunError::TimedOut { .. } => false,
    }
}
