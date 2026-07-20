use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, CommandPlan, OpenMode},
    registry::Registry,
    task_operations::kernel::execute_external_plan,
};

const STALE_BRANCH_ADOPTION_REASON: &str =
    "checkout changed since repair was planned; refresh and retry";

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
    Ok(match kind {
        TaskCommandKind::Resume => commands::open_task_plan(context, qualified_handle, open_mode)?,
        TaskCommandKind::Review => crate::commands::diff_task_plan(context, qualified_handle)?,
        TaskCommandKind::Repair => repair_task_plan(context, qualified_handle, open_mode)?,
        TaskCommandKind::Ship => commands::merge_task_plan(context, qualified_handle)?,
    })
}

pub fn execute_task_command_operation<R: Registry>(
    context: &mut CommandContext<R>,
    kind: TaskCommandKind,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    if kind == TaskCommandKind::Ship {
        let plan =
            refresh_ship_plan_before_execute(context, qualified_handle, plan, confirmed, runner)
                .map_err(|error| (error, false))?;
        return execute_ship_task_command_operation(
            context,
            &plan,
            confirmed,
            runner,
            qualified_handle,
        );
    }
    if kind == TaskCommandKind::Repair {
        if plan.branch_adoption.is_some() {
            return execute_repair_branch_adoption(context, qualified_handle, plan, confirmed);
        }
        commands::mark_task_check_started(context, qualified_handle)
            .map_err(|error| (error, false))?;
    }
    let outputs = match execute_external_plan(plan, confirmed, runner) {
        Ok(execution) => execution,
        Err(error) if kind == TaskCommandKind::Repair => {
            commands::mark_task_check_failed(context, qualified_handle)
                .map_err(|mark_error| (mark_error, true))?;
            return Err((error, true));
        }
        Err(error) => return Err((error, false)),
    };
    let state_changed = match kind {
        TaskCommandKind::Review => false,
        TaskCommandKind::Resume => {
            commands::mark_task_opened(context, qualified_handle)
                .map_err(|error| (error, false))?;
            true
        }
        TaskCommandKind::Ship => {
            commands::mark_task_merged(context, qualified_handle)
                .map_err(|error| (error, false))?;
            true
        }
        TaskCommandKind::Repair => {
            commands::mark_task_window_repaired(context, qualified_handle)
                .map_err(|error| (error, true))?;
            commands::mark_task_check_succeeded(context, qualified_handle)
                .map_err(|error| (error, true))?;
            true
        }
    };

    Ok((outputs, state_changed))
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
    plan_task_command_operation(
        context,
        TaskCommandKind::Ship,
        qualified_handle,
        OpenMode::Attach,
    )
}

fn execute_ship_task_command_operation<R: Registry>(
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

fn repair_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
    open_mode: OpenMode,
) -> Result<CommandPlan, CommandError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;

    if task.has_checkout_mismatch() && !task.has_missing_worktree() {
        let mut plan = CommandPlan::new(format!("repair task: {qualified_handle}"));
        let git_status = task
            .git_status
            .as_ref()
            .expect("checkout mismatch requires git evidence");
        match git_status.current_branch.as_deref() {
            Some(observed_branch) if observed_branch != task.branch => {
                plan.requires_confirmation = true;
                plan.set_branch_adoption(task.branch.clone(), observed_branch);
                return Ok(plan);
            }
            Some(_) => {}
            None => {
                plan.blocked_reasons.push(
                    "cannot adopt a detached worktree; switch to a branch and refresh".to_string(),
                );
                return Ok(plan);
            }
        }
    }

    let mut plan =
        commands::task_window_repair_plan_with_open_mode(context, qualified_handle, open_mode)?;
    plan.title = format!("repair task: {qualified_handle}");
    let recreates_worktree = plan
        .commands
        .iter()
        .any(commands::is_git_worktree_add_command);
    let check_plan = if recreates_worktree {
        commands::check_task_plan_after_worktree_recreate(context, qualified_handle)
    } else {
        commands::check_task_plan(context, qualified_handle)
    };
    if let Ok(check_plan) = check_plan {
        plan.commands.extend(check_plan.commands);
        plan.requires_confirmation |= check_plan.requires_confirmation;
        plan.blocked_reasons.extend(check_plan.blocked_reasons);
    }
    Ok(plan)
}

fn execute_repair_branch_adoption<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    plan: &CommandPlan,
    confirmed: bool,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    if !plan.blocked_reasons.is_empty() {
        return Err((
            CommandError::PlanBlocked(plan.blocked_reasons.clone()),
            false,
        ));
    }

    if !confirmed {
        return Err((CommandError::ConfirmationRequired, false));
    }

    let adoption = plan
        .branch_adoption
        .as_ref()
        .expect("repair branch adoption requires typed payload");

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| {
            (
                CommandError::TaskNotFound(qualified_handle.to_string()),
                false,
            )
        })?;

    let task_id = task.id.clone();

    if task.branch != adoption.expected_branch
        || !task.has_checkout_mismatch()
        || task.has_missing_worktree()
        || task
            .git_status
            .as_ref()
            .and_then(|status| status.current_branch.as_deref())
            != Some(adoption.observed_branch.as_str())
    {
        return Err((
            CommandError::PlanBlocked(vec![STALE_BRANCH_ADOPTION_REASON.to_string()]),
            false,
        ));
    }

    context
        .registry
        .adopt_task_branch(
            &task_id,
            adoption.expected_branch.as_str(),
            adoption.observed_branch.as_str(),
        )
        .map_err(|error| (CommandError::Registry(error), false))?;

    Ok((Vec::new(), true))
}
