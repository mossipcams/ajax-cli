use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError},
    events::apply_monitor_event_to_registry,
    models::LifecycleStatus,
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;

use crate::{
    cockpit_backend::{
        refresh_live_context, render_interactive_cockpit_command, render_live_cockpit_command,
    },
    command_error, current_open_mode,
    dispatch::{render_task_command, TaskCommandOperation},
    new_task_request,
    render::{render_execution_outputs, render_plan},
    snapshot_dispatch::{render_matches_with_paths, render_snapshot_matches},
    supervise::supervise_command_output_and_events,
    CliContextPaths, CliError, RenderedCommand,
};

pub(crate) fn render_matches_mut(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<RenderedCommand, CliError> {
    match matches.subcommand() {
        Some((name @ ("repos" | "tasks" | "next" | "inbox" | "ready" | "status"), _)) => {
            render_refreshed_read_command(name, matches, context, runner)
        }
        Some(("start", subcommand)) => {
            let request = new_task_request(subcommand)?;
            let plan = commands::new_task_plan(context, request.clone()).map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let (outputs, task) = execute_new_task_plan(
                context,
                runner,
                &request,
                &plan,
                subcommand.get_flag("yes"),
                current_open_mode(),
            )?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, Some(&task.qualified_handle())),
                state_changed: true,
            })
        }
        Some((name @ ("resume" | "repair" | "review" | "ship" | "drop"), subcommand)) => {
            let operation = TaskCommandOperation::from_cli_subcommand(name).ok_or_else(|| {
                CliError::CommandFailed(format!("unsupported task command: {name}"))
            })?;
            render_task_command(operation, subcommand, context, runner, current_open_mode())
        }
        Some(("tidy", subcommand)) => {
            let plan = commands::sweep_cleanup_plan(context);
            let candidates = commands::sweep_cleanup_candidates(context);
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let outputs =
                execute_sweep_cleanup(context, runner, &candidates, subcommand.get_flag("yes"))?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed: !candidates.is_empty(),
            })
        }
        Some(("supervise", subcommand)) => {
            let supervised_task = validate_supervised_task(context, subcommand)?;
            let (output, events) = supervise_command_output_and_events(subcommand)?;
            let mut state_changed = false;
            if let Some(task_id) = supervised_task {
                for event in &events {
                    state_changed |=
                        apply_monitor_event_to_registry(&mut context.registry, &task_id, event)
                            .map_err(|error| command_error(CommandError::Registry(error)))?;
                }
            }
            Ok(RenderedCommand {
                output,
                state_changed,
            })
        }
        Some(("cockpit", subcommand)) => {
            if subcommand.get_flag("json") {
                return render_refreshed_read_command("cockpit", matches, context, runner);
            }
            if subcommand.get_flag("watch") {
                return render_live_cockpit_command(context, subcommand, runner);
            }
            render_interactive_cockpit_command(context, subcommand, runner)
        }
        _ => Ok(RenderedCommand {
            output: render_snapshot_matches(matches, context)?,
            state_changed: false,
        }),
    }
}

fn render_refreshed_read_command<R: CommandRunner>(
    _name: &str,
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let changed = refresh_live_context(context, runner)?;
    Ok(RenderedCommand {
        output: render_snapshot_matches(matches, context)?,
        state_changed: changed,
    })
}

pub(crate) fn render_matches_mut_with_paths(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<RenderedCommand, CliError> {
    if matches
        .subcommand()
        .is_some_and(|(name, _)| name == "doctor")
    {
        return Ok(RenderedCommand {
            output: render_matches_with_paths(matches, context, paths)?,
            state_changed: false,
        });
    }

    render_matches_mut(matches, context, runner)
}

pub(crate) fn execute_new_task_plan<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    request: &commands::NewTaskRequest,
    plan: &commands::CommandPlan,
    confirmed: bool,
    open_mode: commands::OpenMode,
) -> Result<(Vec<CommandOutput>, ajax_core::models::Task), CliError> {
    let task = commands::record_new_task(context, request).map_err(command_error)?;
    if plan.requires_confirmation && !confirmed {
        return Err(command_error(CommandError::ConfirmationRequired).after_state_change());
    }
    if !plan.blocked_reasons.is_empty() {
        return Err(
            command_error(CommandError::PlanBlocked(plan.blocked_reasons.clone()))
                .after_state_change(),
        );
    }

    let mut outputs = Vec::new();
    for (index, command) in plan.commands.iter().enumerate() {
        let output = runner.run(command).map_err(|error| {
            let _ = commands::mark_new_task_provisioning_failed(context, &task.id);
            command_error(CommandError::CommandRun(error)).after_state_change()
        })?;
        if output.status_code != 0 {
            let _ = commands::mark_new_task_provisioning_failed(context, &task.id);
            return Err(
                command_error(CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: command.program.clone(),
                    status_code: output.status_code,
                    stderr: output.stderr,
                    cwd: command.cwd.clone(),
                }))
                .after_state_change(),
            );
        }
        outputs.push(output);
        commands::mark_new_task_step_completed(context, &task.id, index)
            .map_err(|error| command_error(error).after_state_change())?;
    }
    commands::mark_task_opened(context, &task.qualified_handle())
        .map_err(|error| command_error(error).after_state_change())?;
    let open_plan = commands::open_task_plan(context, &task.qualified_handle(), open_mode)
        .map_err(|error| command_error(error).after_state_change())?;
    outputs.extend(
        commands::execute_plan(&open_plan, true, runner)
            .map_err(|error| command_error(error).after_state_change())?,
    );

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|candidate| candidate.id == task.id)
        .cloned()
        .unwrap_or(task);

    Ok((outputs, task))
}

fn execute_sweep_cleanup<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    candidates: &[String],
    confirmed: bool,
) -> Result<Vec<CommandOutput>, CliError> {
    let mut outputs = Vec::new();
    let mut state_changed = false;

    for candidate in candidates {
        let plan = commands::clean_task_plan(context, candidate).map_err(command_error)?;
        if !plan.blocked_reasons.is_empty() {
            let cli_error = command_error(CommandError::PlanBlocked(plan.blocked_reasons));
            return Err(if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            });
        }
        if plan.requires_confirmation && !confirmed {
            let cli_error = command_error(CommandError::ConfirmationRequired);
            return Err(if state_changed {
                cli_error.after_state_change()
            } else {
                cli_error
            });
        }

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
                let cli_error =
                    command_error(CommandError::CommandRun(CommandRunError::NonZeroExit {
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
            state_changed |= commands::mark_task_cleanup_step_completed(
                context, candidate, command,
            )
            .map_err(|error| {
                let cli_error = command_error(error);
                if state_changed {
                    cli_error.after_state_change()
                } else {
                    cli_error
                }
            })?;
        }
        commands::mark_task_removed(context, candidate).map_err(command_error)?;
        state_changed = true;
    }

    Ok(outputs)
}

fn validate_supervised_task(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<Option<ajax_core::models::TaskId>, CliError> {
    let Some(qualified_handle) = matches.get_one::<String>("task").map(String::as_str) else {
        return Ok(None);
    };

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or_else(|| CliError::CommandFailed(format!("task not found: {qualified_handle}")))?;
    if task.lifecycle_status == LifecycleStatus::Removed {
        return Err(CliError::CommandFailed(format!(
            "task not found: {qualified_handle}"
        )));
    }

    Ok(Some(task.id.clone()))
}
