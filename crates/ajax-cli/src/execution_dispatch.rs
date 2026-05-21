#[cfg(test)]
use ajax_core::task_operations::start::StartTaskOperationPlan;
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError},
    events::apply_monitor_event_to_registry,
    models::LifecycleStatus,
    registry::{InMemoryRegistry, Registry},
    task_operations::start::{execute_start_task_operation, plan_start_task_operation},
    task_operations::sweep_cleanup::{
        execute_sweep_cleanup_operation, plan_sweep_cleanup_operation,
    },
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
    task_session::{execute_task_entry_plan, TaskSessionRunner},
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
            let operation =
                plan_start_task_operation(context, request.clone()).map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(operation.plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let (outputs, task) = execute_start_task_operation(
                context,
                runner,
                &request,
                &operation,
                subcommand.get_flag("yes"),
                current_open_mode(),
            )
            .map_err(|error| command_error(error).after_state_change())?;
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
            let operation = plan_sweep_cleanup_operation(context);
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(operation.plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let execution = execute_sweep_cleanup_operation(
                context,
                &operation,
                subcommand.get_flag("yes"),
                runner,
            )
            .map_err(|error| {
                let cli_error = command_error(error.error);
                if error.state_changed {
                    cli_error.after_state_change()
                } else {
                    cli_error
                }
            })?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&execution.outputs, None),
                state_changed: execution.state_changed,
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

#[cfg(test)]
pub(crate) fn execute_new_task_plan<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    request: &commands::NewTaskRequest,
    plan: &commands::CommandPlan,
    confirmed: bool,
    open_mode: commands::OpenMode,
) -> Result<(Vec<CommandOutput>, ajax_core::models::Task), CliError> {
    let intent = commands::task_from_new_request(context, request)
        .map_err(command_error)?
        .intent();
    let operation = StartTaskOperationPlan {
        intent,
        plan: plan.clone(),
    };
    execute_start_task_operation(context, runner, request, &operation, confirmed, open_mode)
        .map_err(|error| command_error(error).after_state_change())
}

pub(crate) fn execute_new_task_plan_with_task_session<R: CommandRunner, S: TaskSessionRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    task_session: &mut S,
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
        if let Some(step) = start_provisioning_step_for_command_index(plan, index) {
            commands::mark_new_task_provisioning_step_completed(context, &task.id, step)
                .map_err(|error| command_error(error).after_state_change())?;
        }
        if !commands::is_new_task_husky_hook_command(command) {
            outputs.push(output);
        }
    }
    commands::mark_task_opened(context, &task.qualified_handle())
        .map_err(|error| command_error(error).after_state_change())?;
    let open_plan = commands::open_task_plan(context, &task.qualified_handle(), open_mode)
        .map_err(|error| command_error(error).after_state_change())?;
    outputs.extend(
        execute_task_entry_plan(&open_plan, runner, task_session)
            .map_err(|error| error.after_state_change())?,
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

fn start_provisioning_step_for_command_index(
    plan: &commands::CommandPlan,
    index: usize,
) -> Option<commands::StartProvisioningStep> {
    if index == 0 {
        Some(commands::StartProvisioningStep::WorktreeCreated)
    } else if index + 2 == plan.commands.len() {
        Some(commands::StartProvisioningStep::TaskSessionCreated)
    } else if index + 1 == plan.commands.len() {
        Some(commands::StartProvisioningStep::AgentCommandSent)
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    #[test]
    fn cli_start_dispatch_delegates_task_transaction_to_core_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/execution_dispatch.rs"),
        )
        .unwrap();

        let numeric_step_helper = ["mark_new_task", "_step_completed"].concat();

        assert!(source.contains("execute_start_task_operation"));
        assert!(!source.contains(&numeric_step_helper));
    }

    #[test]
    fn cli_tidy_dispatch_delegates_cleanup_execution_to_core_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/execution_dispatch.rs"),
        )
        .unwrap();
        let plan_operation = ["plan_sweep_cleanup", "_operation"].concat();
        let execute_operation = ["execute_sweep_cleanup", "_operation"].concat();
        let local_helper = ["fn execute_sweep", "_cleanup"].concat();

        assert!(source.contains(&plan_operation));
        assert!(source.contains(&execute_operation));
        assert!(!source.contains(&local_helper));
    }
}
