use ajax_core::{
    adapters::{
        CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter, TmuxAdapter,
    },
    commands::{self, CommandContext, CommandError},
    models::{LifecycleStatus, Task},
    registry::RegistryEventKind,
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;

use crate::{
    classifiers::command_error_looks_conflicted,
    command_error,
    render::{render_execution_outputs, render_plan},
    task_arg, CliError, RenderedCommand,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TaskCommandOperation {
    Open,
    Diff,
    Merge,
    Repair,
    Drop,
}

impl TaskCommandOperation {
    pub(crate) fn from_cli_subcommand(name: &str) -> Option<Self> {
        match name {
            "resume" => Some(Self::Open),
            "repair" => Some(Self::Repair),
            "review" => Some(Self::Diff),
            "ship" => Some(Self::Merge),
            "drop" => Some(Self::Drop),
            _ => None,
        }
    }

    pub(crate) fn plan<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
    ) -> Result<commands::CommandPlan, CommandError> {
        self.plan_with_open_mode(context, task, commands::OpenMode::Attach)
    }

    pub(crate) fn plan_with_open_mode<R: Registry>(
        self,
        context: &CommandContext<R>,
        task: &str,
        open_mode: commands::OpenMode,
    ) -> Result<commands::CommandPlan, CommandError> {
        match self {
            Self::Open => commands::open_task_plan(context, task, open_mode),
            Self::Diff => commands::diff_task_plan(context, task),
            Self::Merge => commands::merge_task_plan(context, task),
            Self::Repair => repair_task_plan(context, task, open_mode),
            Self::Drop => drop_task_plan(context, task),
        }
    }

    pub(crate) fn apply_after_execute<R: Registry>(
        self,
        context: &mut CommandContext<R>,
        task: &str,
    ) -> Result<bool, CommandError> {
        match self {
            Self::Open => {
                commands::mark_task_opened(context, task)?;
                Ok(true)
            }
            Self::Merge => {
                commands::mark_task_merged(context, task)?;
                Ok(true)
            }
            Self::Drop => {
                commands::mark_task_removed(context, task)?;
                Ok(true)
            }
            Self::Repair => {
                commands::mark_task_trunk_repaired(context, task)?;
                Ok(true)
            }
            Self::Diff => Ok(false),
        }
    }

    pub(crate) fn returns_to_cockpit_after_execute(self) -> bool {
        matches!(self, Self::Diff | Self::Merge | Self::Repair | Self::Drop)
    }
}

fn repair_task_plan<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
    open_mode: commands::OpenMode,
) -> Result<commands::CommandPlan, CommandError> {
    let mut plan = commands::trunk_task_plan_with_open_mode(context, task, open_mode)?;
    plan.title = format!("repair task: {task}");
    if let Ok(check_plan) = commands::check_task_plan(context, task) {
        plan.commands.extend(check_plan.commands);
        plan.requires_confirmation |= check_plan.requires_confirmation;
        plan.blocked_reasons.extend(check_plan.blocked_reasons);
    }
    Ok(plan)
}

fn drop_task_plan<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> Result<commands::CommandPlan, CommandError> {
    let clean_plan = commands::clean_task_plan(context, task)?;
    if clean_plan.blocked_reasons.is_empty() {
        Ok(clean_plan)
    } else {
        commands::remove_task_plan(context, task)
    }
}

pub(crate) fn render_task_command<R: CommandRunner>(
    operation: TaskCommandOperation,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    open_mode: commands::OpenMode,
) -> Result<RenderedCommand, CliError> {
    let task = task_arg(subcommand)?;
    let execute = subcommand.get_flag("execute");
    let confirmed = subcommand.get_flag("yes");
    if operation == TaskCommandOperation::Drop && execute {
        return execute_observed_drop(context, task, confirmed, runner);
    }
    let mut pre_plan_state_changed = false;
    if !matches!(
        operation,
        TaskCommandOperation::Drop | TaskCommandOperation::Merge
    ) {
        if let Ok(changed) = commands::refresh_git_substrate_evidence(context, runner) {
            pre_plan_state_changed |= changed;
        }
    }
    let drop_cleanup_refresh = operation == TaskCommandOperation::Drop
        && drop_should_refresh_cleanup_evidence(context, task);
    let mut plan = if drop_cleanup_refresh && execute {
        match commands::ensure_cleanup_git_status(context, task, runner) {
            Ok(()) => operation.plan_with_open_mode(context, task, open_mode),
            Err(error) if drop_git_status_error_is_missing_worktree(&error) => {
                if confirmed {
                    commands::remove_task_plan(context, task)
                } else {
                    operation.plan_with_open_mode(context, task, open_mode)
                }
            }
            Err(error) => Err(error),
        }
    } else {
        if operation == TaskCommandOperation::Drop
            && !drop_cleanup_refresh
            && (!execute || confirmed)
        {
            match commands::refresh_git_substrate_evidence(context, runner) {
                Ok(changed) => pre_plan_state_changed |= changed,
                Err(_) => {
                    pre_plan_state_changed |=
                        commands::mark_task_git_substrate_missing(context, task)
                            .map_err(command_error)?;
                }
            }
        }
        operation.plan_with_open_mode(context, task, open_mode)
    }
    .map_err(command_error)?;
    if !execute {
        return Ok(RenderedCommand {
            output: render_plan(plan, subcommand.get_flag("json"))?,
            state_changed: pre_plan_state_changed,
        });
    }
    if operation == TaskCommandOperation::Merge
        && plan.blocked_reasons.is_empty()
        && (!plan.requires_confirmation || confirmed)
        && merge_task_has_cached_git_evidence(context, task)
    {
        refresh_merge_evidence_if_available(context, task, runner);
        plan = operation
            .plan_with_open_mode(context, task, open_mode)
            .map_err(command_error)?;
    }

    if matches!(operation, TaskCommandOperation::Repair) {
        commands::mark_task_check_started(context, task).map_err(command_error)?;
        match commands::execute_plan(&plan, confirmed, runner) {
            Ok(outputs) => {
                if operation == TaskCommandOperation::Repair {
                    commands::mark_task_trunk_repaired(context, task).map_err(command_error)?;
                }
                commands::mark_task_check_succeeded(context, task).map_err(command_error)?;
                return Ok(RenderedCommand {
                    output: render_execution_outputs(&outputs, None),
                    state_changed: true,
                });
            }
            Err(error) => {
                commands::mark_task_check_failed(context, task)
                    .map_err(|mark_error| command_error(mark_error).after_state_change())?;
                return Err(command_error(error).after_state_change());
            }
        }
    }

    if operation == TaskCommandOperation::Merge {
        match commands::execute_plan(&plan, confirmed, runner) {
            Ok(outputs) => {
                commands::mark_task_merged(context, task).map_err(command_error)?;
                return Ok(RenderedCommand {
                    output: render_execution_outputs(&outputs, None),
                    state_changed: true,
                });
            }
            Err(error) => {
                if matches!(error, CommandError::CommandRun(_)) {
                    let conflicted = merge_error_looks_conflicted(&error);
                    commands::mark_task_merge_failed(context, task, conflicted)
                        .map_err(|mark_error| command_error(mark_error).after_state_change())?;
                    return Err(command_error(error).after_state_change());
                }
                return Err(command_error(error));
            }
        }
    }

    if matches!(operation, TaskCommandOperation::Drop) {
        return execute_teardown_plan(context, task, operation, &plan, confirmed, runner);
    }

    let outputs = commands::execute_plan(&plan, confirmed, runner).map_err(command_error)?;
    let state_changed = operation
        .apply_after_execute(context, task)
        .map_err(command_error)?;
    Ok(RenderedCommand {
        output: render_execution_outputs(&outputs, None),
        state_changed,
    })
}

fn merge_task_has_cached_git_evidence<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> bool {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.qualified_handle() == task)
        .is_some_and(|candidate| candidate.git_status.is_some())
}

fn drop_should_refresh_cleanup_evidence<R: Registry>(
    context: &CommandContext<R>,
    task: &str,
) -> bool {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.qualified_handle() == task)
        .is_some_and(|candidate| {
            matches!(
                candidate.lifecycle_status,
                ajax_core::models::LifecycleStatus::Merged
                    | ajax_core::models::LifecycleStatus::Cleanable
            )
        })
}

fn refresh_merge_evidence_if_available<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task: &str,
    runner: &mut R,
) {
    // Merge still runs the fresh-evidence probe first when available; if the
    // probe itself cannot run, the existing plan remains the operator-facing
    // source of confirmation and execution errors.
    let _refresh_attempted = commands::refresh_git_evidence(context, task, runner, false).is_ok();
}

fn merge_error_looks_conflicted(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::CommandRun(error) if command_error_looks_conflicted(error)
    )
}

fn execute_observed_drop<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task_handle: &str,
    confirmed: bool,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let confirmation_plan = drop_task_plan(context, task_handle).map_err(command_error)?;
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(command_error(CommandError::PlanBlocked(
            confirmation_plan.blocked_reasons,
        )));
    }
    let task_before_drop = cli_task(context, task_handle)?;
    let resuming_incomplete =
        task_before_drop.lifecycle_status == LifecycleStatus::TeardownIncomplete;
    let cleanup_lifecycle = matches!(
        task_before_drop.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable
    );
    let can_observe_before_confirmation = matches!(
        task_before_drop.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) && !task_before_drop
        .has_side_flag(ajax_core::models::SideFlag::Dirty)
        && !task_before_drop.has_side_flag(ajax_core::models::SideFlag::Conflicted)
        && !task_before_drop.has_side_flag(ajax_core::models::SideFlag::Unpushed)
        && task_before_drop.git_status.as_ref().is_none_or(|status| {
            !status.dirty && !status.conflicted && status.unpushed_commits == 0
        });
    if confirmation_plan.requires_confirmation
        && !confirmed
        && !resuming_incomplete
        && !can_observe_before_confirmation
    {
        return Err(command_error(CommandError::ConfirmationRequired));
    }

    commands::mark_task_removing(context, task_handle).map_err(command_error)?;
    let initial_task = cli_task(context, task_handle)?.clone();
    let observation = commands::observe_drop_resources(context, &initial_task, runner)
        .map_err(|error| command_error(error).after_state_change())?;
    let ops = commands::plan_drop_from_observation(&observation);
    let force = drop_needs_force(context, task_handle, &confirmation_plan, cleanup_lifecycle)?;
    let mut outputs = Vec::new();

    for op in ops {
        match op {
            commands::DropOp::EnsureAgentStopped => {
                commands::mark_drop_agent_stopped(context, task_handle)
                    .map_err(|error| command_error(error).after_state_change())?;
                record_drop_step_event(context, task_handle, op)
                    .map_err(|error| command_error(error).after_state_change())?;
            }
            commands::DropOp::EnsureTmuxSessionAbsent
            | commands::DropOp::EnsureWorktreeAbsent
            | commands::DropOp::EnsureBranchAbsent => {
                let command = drop_op_command(context, task_handle, op, force)?;
                let output = runner.run(&command).map_err(|error| {
                    command_error(CommandError::CommandRun(error)).after_state_change()
                })?;
                if output.status_code != 0
                    && !drop_cleanup_resource_is_already_missing(&command, &output)
                {
                    let drop_error = CommandError::CommandRun(CommandRunError::NonZeroExit {
                        program: command.program.clone(),
                        status_code: output.status_code,
                        stderr: output.stderr.clone(),
                        cwd: command.cwd.clone(),
                    });
                    mark_observed_drop_failure(context, task_handle, op, runner)
                        .map_err(|error| command_error(error).after_state_change())?;
                    return Err(command_error(drop_error).after_state_change());
                }
                outputs.push(output);
                commands::mark_task_cleanup_step_completed(context, task_handle, &command)
                    .map_err(|error| command_error(error).after_state_change())?;
                record_drop_step_event(context, task_handle, op)
                    .map_err(|error| command_error(error).after_state_change())?;
            }
            commands::DropOp::MarkRegistryRemoved => {}
        }
    }

    let final_task = cli_task(context, task_handle)?.clone();
    let final_observation = commands::observe_drop_resources(context, &final_task, runner)
        .map_err(|error| command_error(error).after_state_change())?;
    if drop_observation_all_absent(&final_observation) {
        commands::mark_task_removed(context, task_handle).map_err(command_error)?;
        let output = if outputs.is_empty() {
            format!("removed task: {task_handle}")
        } else {
            render_execution_outputs(&outputs, None)
        };
        return Ok(RenderedCommand {
            output,
            state_changed: true,
        });
    }

    commands::mark_task_teardown_incomplete(
        context,
        task_handle,
        commands::DropOp::MarkRegistryRemoved,
        &final_observation,
    )
    .map_err(|error| command_error(error).after_state_change())?;
    Err(CliError::CommandFailedAfterStateChange(
        "drop teardown incomplete".to_string(),
    ))
}

fn cli_task<'a>(
    context: &'a CommandContext<InMemoryRegistry>,
    task_handle: &str,
) -> Result<&'a Task, CliError> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| command_error(CommandError::TaskNotFound(task_handle.to_string())))
}

fn drop_op_command(
    context: &CommandContext<InMemoryRegistry>,
    task_handle: &str,
    op: commands::DropOp,
    force: bool,
) -> Result<CommandSpec, CliError> {
    let task = cli_task(context, task_handle)?;
    let repo_path = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == task.repo)
        .map(|repo| repo.path.display().to_string())
        .ok_or_else(|| command_error(CommandError::RepoNotFound(task.repo.clone())))?;
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let command = match op {
        commands::DropOp::EnsureTmuxSessionAbsent => tmux.kill_session(&task.tmux_session),
        commands::DropOp::EnsureWorktreeAbsent if force => {
            git.force_remove_worktree(&repo_path, &task.worktree_path.display().to_string())
        }
        commands::DropOp::EnsureWorktreeAbsent => {
            git.remove_worktree(&repo_path, &task.worktree_path.display().to_string())
        }
        commands::DropOp::EnsureBranchAbsent if force => {
            git.force_delete_branch(&repo_path, &task.branch)
        }
        commands::DropOp::EnsureBranchAbsent => git.delete_branch(&repo_path, &task.branch),
        commands::DropOp::EnsureAgentStopped | commands::DropOp::MarkRegistryRemoved => {
            return Err(CliError::CommandFailed(format!(
                "drop op {op:?} does not have an external command"
            )));
        }
    };
    Ok(command)
}

fn drop_needs_force(
    context: &CommandContext<InMemoryRegistry>,
    task_handle: &str,
    confirmation_plan: &commands::CommandPlan,
    cleanup_lifecycle: bool,
) -> Result<bool, CliError> {
    if confirmation_plan.title.starts_with("remove task:") {
        return Ok(true);
    }
    let task = cli_task(context, task_handle)?;
    if cleanup_lifecycle {
        return Ok(task.has_side_flag(ajax_core::models::SideFlag::Dirty)
            || task.has_side_flag(ajax_core::models::SideFlag::Conflicted)
            || task.git_status.as_ref().is_some_and(|status| {
                status.dirty || status.untracked_files > 0 || status.conflicted
            }));
    }
    Ok(task.has_side_flag(ajax_core::models::SideFlag::Dirty)
        || task.has_side_flag(ajax_core::models::SideFlag::Conflicted)
        || task.has_side_flag(ajax_core::models::SideFlag::Unpushed)
        || task.git_status.as_ref().is_some_and(|status| {
            status.dirty
                || status.untracked_files > 0
                || status.conflicted
                || status.unpushed_commits > 0
                || !status.merged
        }))
}

fn record_drop_step_event(
    context: &mut CommandContext<InMemoryRegistry>,
    task_handle: &str,
    op: commands::DropOp,
) -> Result<(), CommandError> {
    let task_id = cli_task(context, task_handle)
        .map_err(|error| CommandError::CommandRun(CommandRunError::SpawnFailed(error.to_string())))?
        .id
        .clone();
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::SubstrateChanged,
            format!("drop step completed: {op:?}"),
        )
        .map_err(CommandError::Registry)
}

fn mark_observed_drop_failure<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task_handle: &str,
    failed_step: commands::DropOp,
    runner: &mut R,
) -> Result<(), CommandError> {
    let task = cli_task(context, task_handle)
        .map_err(|error| CommandError::CommandRun(CommandRunError::SpawnFailed(error.to_string())))?
        .clone();
    let observation = commands::observe_drop_resources(context, &task, runner).unwrap_or(
        commands::DropObservation {
            agent: commands::ResourceState::Unknown,
            tmux_session: commands::ResourceState::Unknown,
            worktree: commands::ResourceState::Unknown,
            branch: commands::ResourceState::Unknown,
        },
    );
    commands::mark_task_teardown_incomplete(context, task_handle, failed_step, &observation)
}

fn drop_observation_all_absent(observation: &commands::DropObservation) -> bool {
    observation.agent == commands::ResourceState::Absent
        && observation.tmux_session == commands::ResourceState::Absent
        && observation.worktree == commands::ResourceState::Absent
        && observation.branch == commands::ResourceState::Absent
}

fn execute_teardown_plan<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    task: &str,
    operation: TaskCommandOperation,
    plan: &commands::CommandPlan,
    confirmed: bool,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    if !plan.blocked_reasons.is_empty() {
        return Err(command_error(CommandError::PlanBlocked(
            plan.blocked_reasons.clone(),
        )));
    }
    if plan.requires_confirmation && !confirmed {
        return Err(command_error(CommandError::ConfirmationRequired));
    }

    let mut outputs = Vec::new();
    let mut state_changed = false;
    for command in &plan.commands {
        let output = runner.run(command).map_err(|error| {
            let cli_error = command_error(CommandError::CommandRun(error));
            if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            }
        })?;
        if output.status_code != 0 {
            if operation == TaskCommandOperation::Drop
                && drop_cleanup_resource_is_already_missing(command, &output)
            {
                outputs.push(output);
                state_changed |= commands::mark_task_cleanup_step_completed(context, task, command)
                    .map_err(|error| {
                        let cli_error = command_error(error);
                        if state_changed {
                            cli_error.after_state_change()
                        } else {
                            cli_error
                        }
                    })?;
                continue;
            }
            let cli_error = command_error(CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr.clone(),
                cwd: command.cwd.clone(),
            }));
            return Err(if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            });
        }
        outputs.push(output);
        state_changed |= commands::mark_task_cleanup_step_completed(context, task, command)
            .map_err(|error| {
                let cli_error = command_error(error);
                if state_changed {
                    cli_error.after_state_change()
                } else {
                    cli_error
                }
            })?;
    }

    if operation == TaskCommandOperation::Drop && plan.title.starts_with("remove task:") {
        commands::mark_task_force_removed(context, task).map_err(command_error)?;
    } else {
        commands::mark_task_removed(context, task).map_err(command_error)?;
    }
    let output = if outputs.is_empty() {
        format!("removed task: {task}")
    } else {
        render_execution_outputs(&outputs, None)
    };
    Ok(RenderedCommand {
        output,
        state_changed: true,
    })
}

fn drop_cleanup_resource_is_already_missing(command: &CommandSpec, output: &CommandOutput) -> bool {
    if output.status_code == 0 {
        return false;
    }

    let stderr = output.stderr.to_ascii_lowercase();
    if command.program == "tmux"
        && command
            .args
            .first()
            .is_some_and(|arg| arg == "kill-session")
    {
        return stderr.contains("can't find session")
            || stderr.contains("no server running")
            || stderr.contains("session not found");
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "worktree")
        && command.args.iter().any(|arg| arg == "remove")
    {
        return git_error_says_worktree_missing(&stderr);
    }

    command.program == "git"
        && command.args.iter().any(|arg| arg == "branch")
        && (command.args.iter().any(|arg| arg == "-d")
            || command.args.iter().any(|arg| arg == "-D"))
        && git_error_says_branch_missing(&stderr)
}

fn drop_git_status_error_is_missing_worktree(error: &CommandError) -> bool {
    matches!(
        error,
        CommandError::CommandRun(CommandRunError::NonZeroExit {
            program,
            stderr,
            ..
        }) if program == "git" && git_error_says_worktree_missing(&stderr.to_ascii_lowercase())
    )
}

fn git_error_says_worktree_missing(stderr: &str) -> bool {
    stderr.contains("no such file or directory")
        || stderr.contains("is not a working tree")
        || stderr.contains("is not a worktree")
        || stderr.contains("does not exist")
}

fn git_error_says_branch_missing(stderr: &str) -> bool {
    stderr.contains("not found") || stderr.contains("not a branch")
}
