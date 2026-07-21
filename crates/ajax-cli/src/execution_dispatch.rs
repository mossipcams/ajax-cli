use ajax_core::adapters::{CommandRunError, CommandRunner};
use ajax_core::commands::CommandError;
use ajax_core::commands::OpenMode;
use ajax_core::commands::{self, CommandContext};
use ajax_core::events::apply_monitor_event_to_registry;
use ajax_core::task_operations::kernel::execute_external_plan_with_success;
use ajax_core::{
    adapters::environment::{local_branch_exists, origin_fetch_age},
    registry::InMemoryRegistry,
    task_operations::start::{
        execute_start_task_operation, execute_start_task_operation_with_checkpoint,
        plan_start_task_operation_with_observation,
    },
    task_operations::sweep_cleanup::execute_sweep_cleanup_operation,
    task_operations::task_command::TaskCommandKind,
};
use ajax_core::{models::LifecycleStatus, registry::Registry};
use ajax_core::{
    models::{RuntimeObservationSource, SideFlag, Task},
    runtime::RUNTIME_PROJECTION_FRESH_FOR,
};
use clap::ArgMatches;
use std::time::SystemTime;

use crate::supervise::supervise_command_output_and_events;
use crate::{
    cockpit_backend::{
        mobile_web_port_for_command, refresh_live_context, render_interactive_cockpit_command,
        render_live_cockpit_command,
    },
    task_session::{
        execute_task_entry_plan, TaskEntryPlanOutcome, TaskSessionContext, TaskSessionRunner,
    },
};
use crate::{
    command_error, current_open_mode,
    dispatch::{render_drop_command, render_task_command},
    new_task_request,
    render::{render_execution_outputs, render_plan},
    snapshot_dispatch::{render_matches_with_paths, render_snapshot_matches},
    web_backend::{serve_mobile_web, serve_mobile_web_with_paths},
    CliContextPaths, CliError, RenderedCommand,
};

pub(crate) fn render_matches_mut(
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    open_mode: OpenMode,
) -> Result<RenderedCommand, CliError> {
    match matches.subcommand() {
        Some((name @ ("repos" | "tasks" | "next" | "inbox" | "ready" | "status"), subcommand)) => {
            render_refreshed_read_command(
                name,
                subcommand.get_flag("json"),
                matches,
                context,
                runner,
            )
        }
        Some(("start", subcommand)) => {
            let request = new_task_request(subcommand)?;
            let observation = start_plan_observation(context, &request);
            let (_intent, plan) =
                plan_start_task_operation_with_observation(context, request.clone(), observation)
                    .map_err(command_error)?;

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
                open_mode,
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
            render_task_command(kind, subcommand, context, runner, open_mode)
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
            let orphan_mode = match subcommand.get_one::<String>("orphans").map(String::as_str) {
                Some("all") => Some(commands::OrphanGcMode::All),
                Some("ajax") => Some(commands::OrphanGcMode::AjaxShaped),
                Some(_) => None,
                None => None,
            };
            if !subcommand.get_flag("execute") {
                let mut plan = commands::sweep_cleanup_plan(context);
                if let Some(mode) = orphan_mode {
                    commands::append_orphan_gc_to_plan(context, &mut plan, runner, mode)
                        .map_err(command_error)?;
                }
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }
            let (outputs, state_changed) = execute_sweep_cleanup_operation(
                context,
                subcommand.get_flag("yes"),
                runner,
                orphan_mode,
            )
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
        Some(("supervise", subcommand)) => {
            let supervised_task = validate_supervised_task(context, subcommand)?;
            let task_agent = supervised_task
                .as_ref()
                .and_then(|task_id| context.registry.get_task(task_id))
                .map(supervisor_agent_for_task);
            let (output, events) = supervise_command_output_and_events(subcommand, task_agent)?;
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
        Some((name @ ("cockpit" | "stable" | "dev"), subcommand)) => {
            render_cockpit_entry_command(name, matches, subcommand, context, runner, None, None)
        }
        _ => Ok(RenderedCommand {
            output: render_snapshot_matches(matches, context)?,
            state_changed: false,
        }),
    }
}

pub(crate) fn start_plan_observation(
    context: &CommandContext<InMemoryRegistry>,
    request: &commands::NewTaskRequest,
) -> commands::StartPlanObservation {
    let repo = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo);
    let origin_fetch_age = repo.and_then(|repo| origin_fetch_age(&repo.path));
    // Derive the same handle the start planner would use, then form the
    // `ajax/<handle>` branch without re-implementing slugify. The repo/handle
    // identity is already public via `start_task_identity`.
    let branch = format!(
        "ajax/{}",
        commands::start_task_identity(&request.repo, &request.title)
            .as_str()
            .split_once('/')
            .map(|(_, handle)| handle)
            .unwrap_or_default()
    );
    let target_branch_exists = repo.is_some_and(|repo| local_branch_exists(&repo.path, &branch));

    commands::StartPlanObservation {
        origin_fetch_age,
        target_branch_exists,
    }
}

fn render_refreshed_read_command<R: CommandRunner>(
    name: &str,
    json: bool,
    matches: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let changed = refresh_read_context(name, json, context, runner)?;
    Ok(RenderedCommand {
        output: render_snapshot_matches(matches, context)?,
        state_changed: changed,
    })
}

fn refresh_read_context<R: CommandRunner>(
    name: &str,
    json: bool,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<bool, CliError> {
    if json || name == "cockpit" {
        return refresh_live_context(context, runner);
    }
    if !read_context_needs_live_refresh(context) {
        return Ok(false);
    }
    refresh_live_context(context, runner)
}

fn read_context_needs_live_refresh(context: &CommandContext<InMemoryRegistry>) -> bool {
    let now = SystemTime::now();
    context
        .registry
        .list_tasks()
        .into_iter()
        .any(|task| read_task_needs_live_refresh(task, now))
}

fn read_task_needs_live_refresh(task: &Task, now: SystemTime) -> bool {
    if task.has_side_flag(SideFlag::TmuxMissing)
        || task.has_side_flag(SideFlag::TaskWindowMissing)
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
    mut save_state: Option<&mut crate::context::ContextSaveState>,
) -> Result<RenderedCommand, CliError> {
    if let Some(("start", subcommand)) = matches.subcommand() {
        if let (Some(paths), Some(save_state)) = (paths, save_state.as_deref_mut()) {
            let request = new_task_request(subcommand)?;
            let observation = start_plan_observation(context, &request);
            let (_intent, plan) =
                plan_start_task_operation_with_observation(context, request.clone(), observation)
                    .map_err(command_error)?;

            if !subcommand.get_flag("execute") {
                return Ok(RenderedCommand {
                    output: render_plan(plan, subcommand.get_flag("json"))?,
                    state_changed: false,
                });
            }

            let (outputs, task) = execute_start_task_operation_with_checkpoint(
                context,
                runner,
                &request,
                &plan,
                subcommand.get_flag("yes"),
                current_open_mode(),
                |checkpoint_context| {
                    crate::context::save_context_with_state(paths, checkpoint_context, save_state)
                        .map_err(|error| {
                            CommandError::CommandRun(CommandRunError::SpawnFailed(format!(
                                "persist start checkpoint: {error}"
                            )))
                        })
                },
            )
            .map_err(|error| command_error(error).after_state_change())?;
            return Ok(RenderedCommand {
                output: render_execution_outputs(&outputs, Some(&task.qualified_handle())),
                state_changed: true,
            });
        }
    }

    if let Some((name @ ("cockpit" | "stable" | "dev"), subcommand)) = matches.subcommand() {
        return render_cockpit_entry_command(
            name, matches, subcommand, context, runner, paths, save_state,
        );
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

    render_matches_mut(matches, context, runner, current_open_mode())
}

fn render_cockpit_entry_command<R: CommandRunner>(
    name: &str,
    matches: &ArgMatches,
    subcommand: &ArgMatches,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    paths: Option<&CliContextPaths>,
    save_state: Option<&mut crate::context::ContextSaveState>,
) -> Result<RenderedCommand, CliError> {
    if subcommand.get_flag("watch") {
        return render_live_cockpit_command(context, subcommand, runner);
    }
    if subcommand.get_flag("json") {
        if name != "cockpit" {
            return render_live_cockpit_command(context, subcommand, runner);
        }
        return render_refreshed_read_command(name, true, matches, context, runner);
    }
    render_interactive_cockpit_command(
        context,
        subcommand,
        runner,
        mobile_web_port_for_cockpit_entry(name, paths),
        paths,
        save_state,
    )
}

fn mobile_web_port_for_cockpit_entry(name: &str, paths: Option<&CliContextPaths>) -> u16 {
    paths
        .map(|paths| mobile_web_port_for_command(&paths.runtime_paths.profile))
        .unwrap_or_else(|| mobile_web_port_for_command(name))
}

pub(crate) struct ExecuteNewTaskWithSession<'a> {
    pub request: &'a commands::NewTaskRequest,
    pub plan: &'a commands::CommandPlan,
    pub session_context: &'a TaskSessionContext,
    pub confirmed: bool,
    pub open_mode: commands::OpenMode,
}

pub(crate) fn execute_new_task_plan_with_task_session_and_checkpoint<
    R: CommandRunner,
    S: TaskSessionRunner,
    C,
>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    task_session: &mut S,
    execution: &ExecuteNewTaskWithSession<'_>,
    mut checkpoint: C,
) -> Result<TaskEntryPlanOutcome, CliError>
where
    C: FnMut(&CommandContext<InMemoryRegistry>) -> Result<(), CommandError>,
{
    let request = execution.request;
    let plan = execution.plan;
    let session_context = execution.session_context;
    let confirmed = execution.confirmed;
    let open_mode = execution.open_mode;
    let task = commands::record_new_task(context, request).map_err(command_error)?;
    checkpoint(context).map_err(|error| command_error(error).after_state_change())?;
    let external_outputs =
        match execute_external_plan_with_success(plan, confirmed, runner, |index, _, _| {
            if let Some(step) = plan
                .commands
                .get(index)
                .and_then(commands::start_provisioning_step_for_command)
            {
                commands::mark_new_task_provisioning_step_completed(context, &task.id, step)?;
                checkpoint(context)?;
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
    let mut outputs = external_outputs;
    commands::mark_task_opened(context, &task.qualified_handle())
        .map_err(|error| command_error(error).after_state_change())?;
    let open_plan = commands::open_task_plan(context, &task.qualified_handle(), open_mode)
        .map_err(|error| command_error(error).after_state_change())?;
    match execute_task_entry_plan(&open_plan, runner, task_session, session_context)
        .map_err(|error| error.after_state_change())?
    {
        TaskEntryPlanOutcome::Completed(session_outputs) => {
            outputs.extend(session_outputs);
            Ok(TaskEntryPlanOutcome::Completed(outputs))
        }
        TaskEntryPlanOutcome::OpenNewTask => Ok(TaskEntryPlanOutcome::OpenNewTask),
    }
}

#[cfg(test)]
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

fn supervisor_agent_for_task(task: &ajax_core::models::Task) -> ajax_supervisor::SupervisorAgent {
    use ajax_core::models::AgentClient;
    use ajax_supervisor::SupervisorAgent;

    match task.selected_agent {
        AgentClient::Other => SupervisorAgent::Cursor,
        AgentClient::Codex | AgentClient::Claude => SupervisorAgent::Codex,
    }
}

#[cfg(test)]
mod tests {
    use super::start_plan_observation;
    use ajax_core::{
        commands::{CommandContext, NewTaskRequest},
        config::{Config, ManagedRepo},
        registry::InMemoryRegistry,
    };
    use std::{
        fs::{self, File},
        io::Write,
        time::{Duration, SystemTime},
    };

    #[test]
    fn start_plan_observation_reads_fetch_head_age() {
        let root = std::env::temp_dir().join(format!(
            "ajax-cli-start-plan-observation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".git")).unwrap();
        let mut file = File::create(root.join(".git/FETCH_HEAD")).unwrap();
        writeln!(file, "ref: origin/main").unwrap();
        let context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", root.display().to_string(), "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };

        let observation = start_plan_observation(&context, &request);

        assert!(matches!(
            observation.origin_fetch_age,
            Some(age) if age < Duration::from_secs(5)
        ));
        let _ = fs::remove_dir_all(root);
    }
}
