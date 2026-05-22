#[cfg(any(test, feature = "interactive"))]
use ajax_core::adapters::CommandOutput;
use ajax_core::adapters::CommandRunner;
#[cfg(any(test, feature = "interactive", feature = "supervisor"))]
use ajax_core::commands::CommandError;
use ajax_core::commands::{self, CommandContext};
#[cfg(feature = "supervisor")]
use ajax_core::events::apply_monitor_event_to_registry;
#[cfg(feature = "interactive")]
use ajax_core::task_operations::kernel::execute_external_plan_with_success;
#[cfg(any(feature = "interactive", feature = "supervisor"))]
use ajax_core::{models::LifecycleStatus, registry::Registry};
#[cfg(feature = "interactive")]
use ajax_core::{
    models::{RuntimeObservationSource, SideFlag, Task},
    runtime::RUNTIME_PROJECTION_FRESH_FOR,
};
use ajax_core::{
    registry::InMemoryRegistry,
    task_operations::start::{execute_start_task_operation, plan_start_task_operation},
    task_operations::sweep_cleanup::execute_sweep_cleanup_operation,
    task_operations::task_command::TaskCommandKind,
};
use clap::ArgMatches;
#[cfg(feature = "interactive")]
use std::time::SystemTime;

#[cfg(feature = "supervisor")]
use crate::supervise::supervise_command_output_and_events;
#[cfg(feature = "interactive")]
use crate::{
    cockpit_backend::{
        mobile_web_port_for_command, refresh_live_context, render_interactive_cockpit_command,
        render_live_cockpit_command,
    },
    task_session::{execute_task_entry_plan, TaskSessionRunner},
};
use crate::{
    command_error, current_open_mode,
    dispatch::{render_drop_command, render_task_command},
    new_task_request,
    render::{render_execution_outputs, render_plan},
    snapshot_dispatch::{render_matches_with_paths, render_snapshot_matches},
    web_companion_backend::{serve_mobile_web, serve_mobile_web_with_paths},
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
            let (_intent, plan) =
                plan_start_task_operation(context, request.clone()).map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let (outputs, task) = execute_start_task_operation(
                context,
                runner,
                &request,
                &plan,
                subcommand.get_flag("yes"),
                current_open_mode(),
            )
            .map_err(|error| command_error(error).after_state_change())?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, Some(&task.qualified_handle())),
                state_changed: true,
            })
        }
        Some((name @ ("resume" | "repair" | "review" | "ship"), subcommand)) => {
            let kind = match name {
                "resume" => TaskCommandKind::Resume,
                "repair" => TaskCommandKind::Repair,
                "review" => TaskCommandKind::Review,
                "ship" => TaskCommandKind::Ship,
                _ => unreachable!("task command pattern only matches known commands"),
            };
            render_task_command(kind, subcommand, context, runner, current_open_mode())
        }
        Some(("drop", subcommand)) => render_drop_command(subcommand, context, runner),
        Some(("web", subcommand)) => {
            let host = subcommand
                .get_one::<String>("host")
                .map(String::as_str)
                .unwrap_or("0.0.0.0");
            let port = subcommand
                .get_one::<String>("port")
                .map(String::as_str)
                .unwrap_or("8787")
                .parse::<u16>()
                .map_err(|_| {
                    CliError::CommandFailed(format!(
                        "invalid --port value: {}",
                        subcommand
                            .get_one::<String>("port")
                            .map(String::as_str)
                            .unwrap_or("8787")
                    ))
                })?;
            serve_mobile_web(host, port, context, runner)?;
            Ok(RenderedCommand {
                output: String::new(),
                state_changed: false,
            })
        }
        Some(("tidy", subcommand)) => {
            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(
                        commands::sweep_cleanup_plan(context),
                        subcommand.get_flag("json"),
                    )?,
                    state_changed: false,
                });
            }
            let (outputs, state_changed) =
                execute_sweep_cleanup_operation(context, subcommand.get_flag("yes"), runner)
                    .map_err(|(error, error_state_changed)| {
                        let cli_error = command_error(error);
                        if error_state_changed {
                            cli_error.after_state_change()
                        } else {
                            cli_error
                        }
                    })?;
            Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, None),
                state_changed,
            })
        }
        #[cfg(feature = "supervisor")]
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
        #[cfg(not(feature = "supervisor"))]
        Some(("supervise", _)) => Err(CliError::CommandFailed(
            "supervise support is not enabled in this build".to_string(),
        )),
        #[cfg(feature = "interactive")]
        Some((name @ ("cockpit" | "stable" | "dev"), subcommand)) => {
            render_cockpit_entry_command(name, matches, subcommand, context, runner, None)
        }
        #[cfg(not(feature = "interactive"))]
        Some(("cockpit" | "stable" | "dev", _)) => Err(CliError::CommandFailed(
            "cockpit support is not enabled in this build".to_string(),
        )),
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
    let changed = refresh_read_context(context, runner)?;
    Ok(RenderedCommand {
        output: render_snapshot_matches(matches, context)?,
        state_changed: changed,
    })
}

#[cfg(feature = "interactive")]
fn refresh_read_context<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<bool, CliError> {
    if !read_context_needs_live_refresh(context) {
        return Ok(false);
    }
    refresh_live_context(context, runner)
}

#[cfg(not(feature = "interactive"))]
fn refresh_read_context<R: CommandRunner>(
    _context: &mut CommandContext<InMemoryRegistry>,
    _runner: &mut R,
) -> Result<bool, CliError> {
    Ok(false)
}

#[cfg(feature = "interactive")]
fn read_context_needs_live_refresh(context: &CommandContext<InMemoryRegistry>) -> bool {
    let now = SystemTime::now();
    context
        .registry
        .list_tasks()
        .into_iter()
        .any(|task| read_task_needs_live_refresh(task, now))
}

#[cfg(feature = "interactive")]
fn read_task_needs_live_refresh(task: &Task, now: SystemTime) -> bool {
    if task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::WorktrunkMissing)
        || task.has_side_flag(SideFlag::WorktreeMissing)
    {
        return true;
    }

    let live_lifecycle = matches!(
        task.lifecycle_status,
        LifecycleStatus::Provisioning
            | LifecycleStatus::Active
            | LifecycleStatus::Waiting
            | LifecycleStatus::Reviewable
    );
    if !live_lifecycle {
        return false;
    }

    task.runtime_projection.source == RuntimeObservationSource::Unknown
        || task
            .runtime_projection
            .requires_refresh(now, RUNTIME_PROJECTION_FRESH_FOR)
}

pub(crate) fn render_matches_mut_with_paths(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<RenderedCommand, CliError> {
    #[cfg(feature = "interactive")]
    if let Some((name @ ("cockpit" | "stable" | "dev"), subcommand)) = matches.subcommand() {
        return render_cockpit_entry_command(name, matches, subcommand, context, runner, paths);
    }

    if let Some(("web", subcommand)) = matches.subcommand() {
        let host = subcommand
            .get_one::<String>("host")
            .map(String::as_str)
            .unwrap_or("0.0.0.0");
        let port = subcommand
            .get_one::<String>("port")
            .map(String::as_str)
            .unwrap_or("8787")
            .parse::<u16>()
            .map_err(|_| {
                CliError::CommandFailed(format!(
                    "invalid --port value: {}",
                    subcommand
                        .get_one::<String>("port")
                        .map(String::as_str)
                        .unwrap_or("8787")
                ))
            })?;
        serve_mobile_web_with_paths(host, port, context, runner, paths)?;
        return Ok(RenderedCommand {
            output: String::new(),
            state_changed: false,
        });
    }

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

#[cfg(feature = "interactive")]
fn render_cockpit_entry_command<R: CommandRunner>(
    name: &str,
    matches: &ArgMatches,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    paths: Option<&CliContextPaths>,
) -> Result<RenderedCommand, CliError> {
    if subcommand.get_flag("json") {
        if name != "cockpit" {
            return render_live_cockpit_command(context, subcommand, runner);
        }
        return render_refreshed_read_command(name, matches, context, runner);
    }
    if subcommand.get_flag("watch") {
        return render_live_cockpit_command(context, subcommand, runner);
    }
    render_interactive_cockpit_command(
        context,
        subcommand,
        runner,
        mobile_web_port_for_cockpit_entry(name, paths),
        paths,
    )
}

#[cfg(feature = "interactive")]
fn mobile_web_port_for_cockpit_entry(name: &str, paths: Option<&CliContextPaths>) -> u16 {
    paths
        .map(|paths| mobile_web_port_for_command(&paths.runtime_paths.profile))
        .unwrap_or_else(|| mobile_web_port_for_command(name))
}

#[cfg(feature = "interactive")]
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
    let external_outputs =
        match execute_external_plan_with_success(plan, confirmed, runner, |index, _, _| {
            if let Some(step) = start_provisioning_step_for_command_index(plan, index) {
                commands::mark_new_task_provisioning_step_completed(context, &task.id, step)?;
            }
            Ok(())
        }) {
            Ok(outputs) => outputs,
            Err(error @ CommandError::CommandRun(_)) => {
                let _ = commands::mark_new_task_provisioning_failed(context, &task.id);
                return Err(command_error(error).after_state_change());
            }
            Err(error) => return Err(command_error(error).after_state_change()),
        };
    let mut outputs = plan
        .commands
        .iter()
        .zip(external_outputs)
        .filter_map(|(command, output)| {
            (!commands::is_new_task_husky_hook_command(command)).then_some(output)
        })
        .collect::<Vec<_>>();
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

#[cfg(all(test, feature = "interactive"))]
mod cockpit_entry_tests {
    use super::mobile_web_port_for_cockpit_entry;
    use crate::CliContextPaths;
    use ajax_core::config::RuntimePathRequest;

    #[test]
    fn cockpit_entry_mobile_web_port_uses_runtime_profile_over_command_name() {
        let paths = CliContextPaths::from_runtime_paths(
            RuntimePathRequest::new("/Users/matt")
                .with_cli_profile("dev")
                .resolve(),
        );

        assert_eq!(
            mobile_web_port_for_cockpit_entry("cockpit", Some(&paths)),
            8788
        );
    }
}

#[cfg(feature = "interactive")]
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

#[cfg(feature = "supervisor")]
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
        let manual_confirmation = ["plan.requires", "_confirmation"].concat();
        let manual_blocked = ["plan.blocked", "_reasons"].concat();
        let manual_runner = ["runner.run", "(command)"].concat();

        assert!(source.contains("execute_start_task_operation"));
        assert!(source.contains("execute_external_plan_with_success"));
        assert!(!source.contains(&numeric_step_helper));
        assert!(!source.contains(&manual_confirmation));
        assert!(!source.contains(&manual_blocked));
        assert!(!source.contains(&manual_runner));
    }

    #[test]
    fn cli_tidy_dispatch_delegates_cleanup_execution_to_core_operation() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/execution_dispatch.rs"),
        )
        .unwrap();
        let tidy_dispatch = source
            .split("Some((\"tidy\", subcommand))")
            .nth(1)
            .and_then(|source| source.split("Some((\"doctor\", subcommand))").next())
            .unwrap();
        let plan_operation = ["plan_sweep_cleanup", "_operation"].concat();
        let execute_operation = ["execute_sweep_cleanup", "_operation"].concat();
        let direct_plan = ["commands::sweep", "_cleanup_plan"].concat();
        let local_helper = ["fn execute_sweep", "_cleanup"].concat();
        let wrapper_plan_render = ["operation", ".plan"].concat();

        assert!(source.contains(&direct_plan));
        assert!(source.contains(&execute_operation));
        assert!(tidy_dispatch.contains(&execute_operation));
        assert!(!source.contains(&plan_operation));
        assert!(!source.contains(&local_helper));
        assert!(!tidy_dispatch.contains(&wrapper_plan_render));
    }

    #[test]
    fn web_dispatch_delegates_to_mobile_web_server() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/execution_dispatch.rs"),
        )
        .unwrap();

        assert!(source.contains("Some((\"web\", subcommand))"));
        assert!(source.contains("serve_mobile_web"));
    }

    #[test]
    fn web_dispatch_with_paths_can_persist_mobile_actions() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/execution_dispatch.rs"),
        )
        .unwrap();
        let with_paths_dispatch = source
            .split("pub(crate) fn render_matches_mut_with_paths")
            .nth(1)
            .unwrap();

        assert!(with_paths_dispatch.contains("Some((\"web\", subcommand))"));
        assert!(with_paths_dispatch.contains("serve_mobile_web"));
        assert!(with_paths_dispatch.contains("paths"));
    }
}
