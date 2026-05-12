use ajax_core::{
    adapters::{CommandRunner, TmuxAdapter},
    commands::{self, CommandContext},
    live::{self, LiveObservation, LiveStatusKind},
    output::CockpitResponse,
    registry::{InMemoryRegistry, Registry},
};
use clap::ArgMatches;
use std::time::Duration;

use crate::{
    cockpit_actions::{
        execute_pending_cockpit_action, handle_pending_cockpit_result, tui_cockpit_action,
        tui_cockpit_confirmed_action, PendingCockpitOutcome,
    },
    render::render_response,
    CliError, RenderedCommand,
};

pub(crate) fn render_cockpit_command(
    context: &CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
) -> Result<String, CliError> {
    if matches.get_flag("json") {
        return render_response(commands::cockpit(context), true, |_| String::new());
    }

    let iterations = parse_u32_arg(matches, "iterations", 1)?;
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;

    if matches.get_flag("watch") {
        return Ok(render_cockpit_frames(
            context,
            iterations.max(1),
            Duration::from_millis(interval),
        ));
    }

    Err(CliError::CommandFailed(
        "interactive cockpit requires command execution support".to_string(),
    ))
}

fn render_cockpit_frames(
    context: &CommandContext<InMemoryRegistry>,
    iterations: u32,
    interval: Duration,
) -> String {
    let frames = (0..iterations)
        .map(|index| {
            if index > 0 && !interval.is_zero() {
                std::thread::sleep(interval);
            }
            render_cockpit_frame(context)
        })
        .collect::<Vec<_>>();

    frames.join("\n\n")
}

pub(crate) fn render_cockpit_frame(context: &CommandContext<InMemoryRegistry>) -> String {
    ajax_tui::render_cockpit(
        &commands::list_repos(context),
        &commands::list_tasks(context, None),
        &commands::inbox(context),
    )
}

pub(crate) fn render_interactive_cockpit_command<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    subcommand: &ArgMatches,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let mut state_changed = false;
    let mut cockpit_flash = None;
    state_changed |= refresh_live_context(context, runner)?;
    let refresh_interval = Duration::from_millis(parse_u64_arg(subcommand, "interval-ms", 1000)?);
    loop {
        let pending = ajax_tui::run_interactive_with_flash_and_refresh(
            commands::list_repos(context),
            commands::list_tasks(context, None),
            commands::inbox(context),
            cockpit_flash.take(),
            refresh_interval,
            InteractiveCockpitHandler {
                context,
                runner,
                state_changed: &mut state_changed,
            },
        )
        .map_err(|e| CliError::CommandFailed(e.to_string()))?;
        let Some(pending) = pending else {
            return Ok(RenderedCommand {
                output: String::new(),
                state_changed,
            });
        };

        let Some(outcome) = handle_pending_cockpit_result(
            execute_pending_cockpit_action(&pending, context, runner, &mut state_changed),
            &mut cockpit_flash,
        ) else {
            continue;
        };

        match outcome {
            PendingCockpitOutcome::Exit(output) => {
                return Ok(RenderedCommand {
                    output,
                    state_changed,
                });
            }
            PendingCockpitOutcome::ReturnToCockpit => {}
        }
    }
}

pub(crate) fn render_live_cockpit_command<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    matches: &ArgMatches,
    runner: &mut R,
) -> Result<RenderedCommand, CliError> {
    let iterations = parse_u32_arg(matches, "iterations", 1)?.max(1);
    let interval = parse_u64_arg(matches, "interval-ms", 1000)?;

    if matches.get_flag("json") {
        let changed = refresh_live_context(context, runner)?;
        return Ok(RenderedCommand {
            output: render_response(commands::cockpit(context), true, |_| String::new())?,
            state_changed: changed,
        });
    }

    let mut state_changed = false;
    let frames = (0..iterations)
        .map(|index| {
            if index > 0 && interval > 0 {
                std::thread::sleep(Duration::from_millis(interval));
            }
            let changed = refresh_live_context(context, runner)?;
            state_changed |= changed;
            Ok(render_cockpit_frame(context))
        })
        .collect::<Result<Vec<_>, CliError>>()?;

    Ok(RenderedCommand {
        output: frames.join("\n\n"),
        state_changed,
    })
}

pub(crate) fn refresh_live_context<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
) -> Result<bool, CliError> {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| task.lifecycle_status != ajax_core::models::LifecycleStatus::Removed)
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    if task_ids.is_empty() {
        return Ok(false);
    }

    let tmux = TmuxAdapter::new("tmux");
    let sessions_command = tmux.list_sessions();
    let sessions_output = match runner.run(&sessions_command) {
        Ok(output) if output.status_code == 0 => output.stdout,
        Ok(_output) => return Ok(false),
        Err(_error) => return Ok(false),
    };
    if sessions_output.trim().is_empty() {
        return Ok(false);
    }
    let mut changed = false;

    for task_id in task_ids {
        let Some(task_snapshot) = context.registry.get_task(&task_id).cloned() else {
            continue;
        };
        let session_status =
            TmuxAdapter::parse_session_status(&task_snapshot.tmux_session, &sessions_output);

        if !session_status.exists {
            if task_snapshot
                .tmux_status
                .as_ref()
                .is_some_and(|status| status.exists)
            {
                if let Some(task) = context.registry.get_task_mut(&task_id) {
                    task.remove_side_flag(ajax_core::models::SideFlag::AgentRunning);
                    changed = true;
                }
                continue;
            }
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                task.add_side_flag(ajax_core::models::SideFlag::TmuxMissing);
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux session missing"),
                );
                changed = true;
            }
            continue;
        }
        changed |= task_snapshot.tmux_status.as_ref() != Some(&session_status);

        if let Some(task) = context.registry.get_task_mut(&task_id) {
            task.tmux_status = Some(session_status.clone());
        }

        let windows_command = tmux.list_windows(&task_snapshot.tmux_session);
        let windows_output = match runner.run(&windows_command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(_) | Err(_) => {
                if let Some(task) = context.registry.get_task_mut(&task_id) {
                    task.add_side_flag(ajax_core::models::SideFlag::WorktrunkMissing);
                    live::apply_observation(
                        task,
                        LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                    );
                    changed = true;
                }
                continue;
            }
        };
        let worktrunk_status = TmuxAdapter::parse_worktrunk_status(
            &task_snapshot.worktrunk_window,
            &task_snapshot.worktree_path.display().to_string(),
            &windows_output,
        );
        changed |= task_snapshot.worktrunk_status.as_ref() != Some(&worktrunk_status);

        if let Some(task) = context.registry.get_task_mut(&task_id) {
            task.tmux_status = Some(session_status);
            task.worktrunk_status = Some(worktrunk_status.clone());
            if task.has_side_flag(ajax_core::models::SideFlag::TmuxMissing) {
                task.remove_side_flag(ajax_core::models::SideFlag::TmuxMissing);
                changed = true;
            }
        }

        if !worktrunk_status.exists {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                task.add_side_flag(ajax_core::models::SideFlag::WorktrunkMissing);
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                );
                changed = true;
            }
            continue;
        }

        let pane_command =
            tmux.capture_pane(&task_snapshot.tmux_session, &task_snapshot.worktrunk_window);
        let pane_output = match runner.run(&pane_command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(_) | Err(_) => {
                if let Some(task) = context.registry.get_task_mut(&task_id) {
                    live::apply_observation(
                        task,
                        LiveObservation::new(LiveStatusKind::CommandFailed, "live refresh failed"),
                    );
                    changed = true;
                }
                continue;
            }
        };
        let observation = live::classify_pane(&pane_output);
        if let Some(task) = context.registry.get_task_mut(&task_id) {
            let previous = task.clone();
            task.remove_side_flag(ajax_core::models::SideFlag::TmuxMissing);
            task.remove_side_flag(ajax_core::models::SideFlag::WorktrunkMissing);
            live::apply_observation(task, observation);
            changed |= *task != previous;
        }
    }

    Ok(changed)
}

pub(crate) fn refresh_cockpit_snapshot<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> Result<CockpitResponse, CliError> {
    *state_changed |= refresh_live_context(context, runner)?;
    Ok(commands::cockpit(context))
}

struct InteractiveCockpitHandler<'a, R: CommandRunner> {
    context: &'a mut CommandContext<InMemoryRegistry>,
    runner: &'a mut R,
    state_changed: &'a mut bool,
}

impl<R: CommandRunner> ajax_tui::CockpitEventHandler for InteractiveCockpitHandler<'_, R> {
    fn on_action(
        &mut self,
        item: &ajax_core::models::AttentionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_action(item, self.context, self.runner, self.state_changed)
    }

    fn on_confirmed_action(
        &mut self,
        item: &ajax_core::models::AttentionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_confirmed_action(item, self.context, self.runner, self.state_changed)
    }

    fn on_refresh(&mut self) -> std::io::Result<Option<CockpitResponse>> {
        refresh_cockpit_snapshot(self.context, self.runner, self.state_changed)
            .map(Some)
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

fn parse_u32_arg(matches: &ArgMatches, name: &str, default: u32) -> Result<u32, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(default);
    };

    value
        .parse::<u32>()
        .map_err(|_| CliError::CommandFailed(format!("invalid --{name} value: {value}")))
}

fn parse_u64_arg(matches: &ArgMatches, name: &str, default: u64) -> Result<u64, CliError> {
    let Some(value) = matches.get_one::<String>(name) else {
        return Ok(default);
    };

    value
        .parse::<u64>()
        .map_err(|_| CliError::CommandFailed(format!("invalid --{name} value: {value}")))
}
