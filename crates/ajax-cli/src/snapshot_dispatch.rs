use ajax_core::{
    commands::{self, CommandContext},
    output::{state_export_json_snapshot, DoctorCheck},
    registry::InMemoryRegistry,
};
use clap::ArgMatches;

use crate::{
    cockpit_backend::render_cockpit_command,
    command_error, current_open_mode, new_task_request,
    render::{
        render_doctor_human, render_inbox_human, render_inspect_human, render_next_human,
        render_plan, render_repos_human, render_response, render_tasks_human,
    },
    task_arg, CliContextPaths, CliError,
};

pub(crate) fn render_snapshot_matches(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, CliError> {
    render_matches_with_paths(matches, context, None)
}

pub(crate) fn render_matches_with_paths(
    matches: &ArgMatches,
    context: &CommandContext<InMemoryRegistry>,
    paths: Option<&CliContextPaths>,
) -> Result<String, CliError> {
    match matches.subcommand() {
        Some(("repos", subcommand)) => render_response(
            commands::list_repos(context),
            subcommand.get_flag("json"),
            render_repos_human,
        ),
        Some(("tasks", subcommand)) => render_response(
            commands::list_tasks(
                context,
                subcommand.get_one::<String>("repo").map(String::as_str),
            ),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("inspect", subcommand)) => {
            let task = subcommand
                .get_one::<String>("task")
                .map(String::as_str)
                .unwrap_or_default();
            let response = commands::inspect_task(context, task).map_err(command_error)?;
            render_response(response, subcommand.get_flag("json"), render_inspect_human)
        }
        Some(("new", subcommand)) => {
            let request = new_task_request(subcommand)?;
            let plan = commands::new_task_plan(context, request).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("open", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::open_task_plan(context, task, current_open_mode())
                .map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("trunk", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::trunk_task_plan_with_open_mode(context, task, current_open_mode())
                .map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("check", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::check_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("diff", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::diff_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("merge", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::merge_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("cleanup", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::clean_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("clean", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::clean_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("remove", subcommand)) => {
            let task = task_arg(subcommand)?;
            let plan = commands::remove_task_plan(context, task).map_err(command_error)?;
            render_readonly_plan(plan, subcommand)
        }
        Some(("sweep", subcommand)) => {
            render_readonly_plan(commands::sweep_cleanup_plan(context), subcommand)
        }
        Some(("next", subcommand)) => render_response(
            commands::next(context),
            subcommand.get_flag("json"),
            render_next_human,
        ),
        Some(("inbox", subcommand)) => render_response(
            commands::inbox(context),
            subcommand.get_flag("json"),
            render_inbox_human,
        ),
        Some(("review", subcommand)) => render_response(
            commands::review_queue(context),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("doctor", subcommand)) => {
            let mut response = commands::doctor(context);
            if let Some(paths) = paths {
                response.checks.extend(context_path_checks(paths));
            }
            render_response(response, subcommand.get_flag("json"), render_doctor_human)
        }
        Some(("status", subcommand)) => render_response(
            commands::status(context),
            subcommand.get_flag("json"),
            render_tasks_human,
        ),
        Some(("state", subcommand)) => render_state_command(context, subcommand),
        Some(("cockpit", subcommand)) => render_cockpit_command(context, subcommand),
        Some(("supervise", _)) => Err(CliError::CommandFailed(
            "supervise requires mutable context and runner support".to_string(),
        )),
        Some((name, _)) => Err(CliError::CommandFailed(format!(
            "unsupported command: {name}"
        ))),
        None => Err(CliError::CommandFailed(
            "command is required; pass --help".to_string(),
        )),
    }
}

fn render_state_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    match matches.subcommand() {
        Some(("export", subcommand)) => {
            let output = subcommand.get_one::<String>("output").ok_or_else(|| {
                CliError::CommandFailed("state export --output is required".to_string())
            })?;
            export_state_snapshot(context, std::path::Path::new(output))
        }
        Some((name, _)) => Err(CliError::CommandFailed(format!(
            "unknown state subcommand: {name}"
        ))),
        None => Err(CliError::CommandFailed(
            "state subcommand is required".to_string(),
        )),
    }
}

fn export_state_snapshot(
    context: &CommandContext<InMemoryRegistry>,
    path: &std::path::Path,
) -> Result<String, CliError> {
    if path.exists() {
        return Err(CliError::CommandFailed(format!(
            "state export target already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| CliError::CommandFailed(error.to_string()))?;
    }
    let json = state_export_json_snapshot(&context.config, &context.registry)
        .map_err(|error| CliError::CommandFailed(format!("state export failed: {error}")))?;
    std::fs::write(path, json)
        .map_err(|error| CliError::CommandFailed(format!("state export failed: {error}")))?;

    Ok(format!("exported state snapshot: {}", path.display()))
}

fn context_path_checks(paths: &CliContextPaths) -> Vec<DoctorCheck> {
    let config_exists = paths.config_file.is_file();
    let state_exists = paths.state_file.is_file();
    let state_parent_creatable = state_exists || parent_directory_available(&paths.state_file);

    vec![
        DoctorCheck {
            name: "config:path".to_string(),
            ok: config_exists,
            message: if config_exists {
                format!("file exists: {}", paths.config_file.display())
            } else {
                format!(
                    "file not found; defaults in use: {}",
                    paths.config_file.display()
                )
            },
        },
        DoctorCheck {
            name: "state:path".to_string(),
            ok: state_parent_creatable,
            message: if state_exists {
                format!("file exists: {}", paths.state_file.display())
            } else if state_parent_creatable {
                "parent directory can be created".to_string()
            } else {
                format!(
                    "parent directory unavailable: {}",
                    paths.state_file.display()
                )
            },
        },
    ]
}

pub(crate) fn parent_directory_available(path: &std::path::Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let parent = if parent.as_os_str().is_empty() {
        std::env::current_dir().ok()
    } else if parent.is_absolute() {
        Some(parent.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .map(|current_dir| current_dir.join(parent))
    };

    parent.is_some_and(|parent| {
        parent.is_dir() || parent.ancestors().skip(1).any(|ancestor| ancestor.is_dir())
    })
}

fn render_readonly_plan(
    plan: commands::CommandPlan,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("execute") {
        return Err(CliError::CommandFailed(
            "execution requires mutable context and runner support".to_string(),
        ));
    }

    render_plan(plan, matches.get_flag("json"))
}
