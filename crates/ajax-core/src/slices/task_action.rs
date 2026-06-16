use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan, OpenMode},
    external_plan,
    registry::Registry,
};

pub(crate) fn plan_resume<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    commands::open_task_plan(context, qualified_handle, open_mode)
}

pub(crate) fn execute_resume<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    revalidate(context, qualified_handle, crate::slices::resume::decision)
        .map_err(|error| (error, false))?;
    let outputs =
        external_plan::execute(plan, confirmed, runner).map_err(|error| (error, false))?;
    commands::mark_task_opened(context, qualified_handle).map_err(|error| (error, false))?;
    Ok((outputs, true))
}

pub(crate) fn execute_review<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    revalidate(context, qualified_handle, crate::slices::review::decision)
        .map_err(|error| (error, false))?;
    let outputs =
        external_plan::execute(plan, confirmed, runner).map_err(|error| (error, false))?;
    Ok((outputs, false))
}

pub(crate) fn plan_repair<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let mut plan = commands::trunk_task_plan_with_open_mode(context, qualified_handle, open_mode)?;
    plan.title = format!("repair task: {qualified_handle}");
    if let Ok(check_plan) = commands::check_task_plan(context, qualified_handle) {
        plan.commands.extend(check_plan.commands);
        plan.requires_confirmation |= check_plan.requires_confirmation;
        plan.blocked_reasons.extend(check_plan.blocked_reasons);
    }
    Ok(plan)
}

pub(crate) fn execute_repair<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    revalidate(context, qualified_handle, crate::slices::repair::decision)
        .map_err(|error| (error, false))?;
    commands::mark_task_check_started(context, qualified_handle).map_err(|error| (error, false))?;
    let outputs = match external_plan::execute(plan, confirmed, runner) {
        Ok(outputs) => outputs,
        Err(error) => {
            commands::mark_task_check_failed(context, qualified_handle)
                .map_err(|mark_error| (mark_error, true))?;
            return Err((error, true));
        }
    };
    commands::mark_task_trunk_repaired(context, qualified_handle).map_err(|error| (error, true))?;
    commands::mark_task_check_succeeded(context, qualified_handle)
        .map_err(|error| (error, true))?;
    Ok((outputs, true))
}

pub(crate) fn plan_ship<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    commands::merge_task_plan(context, qualified_handle)
}

pub(crate) fn execute_ship<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    revalidate(context, qualified_handle, crate::slices::ship::decision)
        .map_err(|error| (error, false))?;
    let plan = refresh_ship_plan_before_execute(context, qualified_handle, plan, confirmed, runner)
        .map_err(|error| (error, false))?;
    execute_ship_plan(context, qualified_handle, &plan, confirmed, runner)
}

fn revalidate<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    decision: impl Fn(&crate::models::Task) -> crate::recommended::TaskActionDecision,
) -> Result<(), CommandError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    let decision = decision(task);
    if decision.is_available() {
        Ok(())
    } else {
        Err(CommandError::PlanBlocked(vec![decision.reason]))
    }
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
    plan_ship(context, qualified_handle)
}

fn execute_ship_plan<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
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
