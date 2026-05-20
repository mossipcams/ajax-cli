use ajax_core::{
    adapters::{CommandRunner, GitAdapter, TmuxAdapter},
    commands::{self, CommandContext},
    live::{self, LiveObservation, LiveStatusKind},
    models::{
        AgentClient, GitStatus, LifecycleStatus, RuntimeObservationSource, Task, TaskId, TmuxStatus,
    },
    registry::{InMemoryRegistry, Registry},
    runtime::RUNTIME_PROJECTION_FRESH_FOR,
};
use ajax_tui::CockpitSnapshot;
use clap::ArgMatches;
use std::time::{Duration, SystemTime};

use crate::{
    cockpit_actions::{
        execute_pending_cockpit_action_with_task_session, handle_pending_cockpit_result,
        tui_cockpit_action, tui_cockpit_confirmed_action, PendingCockpitOutcome,
    },
    render::render_response,
    task_session::PtyTaskSessionRunner,
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
    let view = commands::cockpit_view(context);
    ajax_tui::render_cockpit(&view.repos, &view.cards, &view.inbox)
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
        let mut task_session = PtyTaskSessionRunner;
        let snapshot = build_cockpit_snapshot(context);
        let pending = ajax_tui::run_interactive_with_flash_and_refresh(
            snapshot.repos,
            snapshot.cards,
            snapshot.inbox,
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
            execute_pending_cockpit_action_with_task_session(
                &pending,
                context,
                runner,
                &mut state_changed,
                &mut task_session,
            ),
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
    let should_refresh_sessions = context
        .registry
        .list_tasks()
        .into_iter()
        .any(|task| should_probe_live_substrate(task) || task.tmux_status.is_some());
    if !should_refresh_sessions {
        return Ok(false);
    }

    let tmux = TmuxAdapter::new("tmux");
    let sessions_command = tmux.list_sessions();
    let sessions_output = match runner.run(&sessions_command) {
        Ok(output) if output.status_code == 0 => output.stdout,
        Ok(_output) => return Ok(false),
        Err(_error) => return Ok(false),
    };

    let mut changed = if has_unregistered_ajax_session(context, &sessions_output) {
        recover_missing_tasks_from_substrate(context, runner, &sessions_output)?
    } else {
        false
    };
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| should_probe_live_substrate(task))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let task_snapshots = task_ids
        .iter()
        .filter_map(|task_id| context.registry.get_task(task_id).cloned())
        .collect::<Vec<_>>();
    let windows_output = if task_snapshots
        .iter()
        .any(|task| TmuxAdapter::parse_session_status(&task.tmux_session, &sessions_output).exists)
    {
        let windows_command = tmux.list_all_windows();
        match runner.run(&windows_command) {
            Ok(output) if output.status_code == 0 => Some(Ok(output.stdout)),
            Ok(_) | Err(_) => Some(Err(())),
        }
    } else {
        None
    };

    for task_snapshot in task_snapshots {
        let task_id = task_snapshot.id.clone();
        let session_status =
            TmuxAdapter::parse_session_status(&task_snapshot.tmux_session, &sessions_output);

        if !session_status.exists {
            let has_fresh_command_result_runtime = task_snapshot.runtime_projection.source
                == RuntimeObservationSource::CommandResult
                && !task_snapshot
                    .runtime_projection
                    .requires_refresh(SystemTime::now(), RUNTIME_PROJECTION_FRESH_FOR);
            if has_fresh_command_result_runtime
                && task_snapshot.tmux_status.is_some()
                && task_snapshot.live_status.is_none()
                && !task_snapshot.has_side_flag(ajax_core::models::SideFlag::TmuxMissing)
            {
                continue;
            }
            changed |= task_snapshot.tmux_status.as_ref() != Some(&session_status);
            context
                .registry
                .update_tmux_status(&task_id, Some(session_status.clone()))
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
            context
                .registry
                .update_worktrunk_status(
                    &task_id,
                    Some(ajax_core::models::WorktrunkStatus {
                        exists: false,
                        window_name: task_snapshot.worktrunk_window.clone(),
                        current_path: task_snapshot.worktree_path.clone(),
                        points_at_expected_path: false,
                    }),
                )
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
            let missing_worktrunk = ajax_core::models::WorktrunkStatus {
                exists: false,
                window_name: task_snapshot.worktrunk_window.clone(),
                current_path: task_snapshot.worktree_path.clone(),
                points_at_expected_path: false,
            };
            changed |= task_snapshot.worktrunk_status.as_ref() != Some(&missing_worktrunk);
            context
                .registry
                .update_worktrunk_status(&task_id, Some(missing_worktrunk))
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
            refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                task.remove_side_flag(ajax_core::models::SideFlag::AgentRunning);
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::TmuxMissing, "tmux session missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        }
        changed |= task_snapshot.tmux_status.as_ref() != Some(&session_status);

        let tmux_status_changed = task_snapshot.tmux_status.as_ref() != Some(&session_status);
        let had_stale_tmux_missing =
            task_snapshot.has_side_flag(ajax_core::models::SideFlag::TmuxMissing);
        changed |= tmux_status_changed || had_stale_tmux_missing;

        if tmux_status_changed || had_stale_tmux_missing {
            context
                .registry
                .update_tmux_status(&task_id, Some(session_status.clone()))
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
        }

        let Some(Ok(windows_output)) = windows_output.as_ref() else {
            context
                .registry
                .update_worktrunk_status(
                    &task_id,
                    Some(ajax_core::models::WorktrunkStatus {
                        exists: false,
                        window_name: task_snapshot.worktrunk_window.clone(),
                        current_path: task_snapshot.worktree_path.clone(),
                        points_at_expected_path: false,
                    }),
                )
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
            refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                );
                refresh_cached_annotations(task);
                changed = true;
            }
            continue;
        };
        let all_windows_output_empty = windows_output.trim().is_empty();
        let mut worktrunk_status = TmuxAdapter::parse_worktrunk_status_for_session(
            &task_snapshot.tmux_session,
            &task_snapshot.worktrunk_window,
            &task_snapshot.worktree_path.display().to_string(),
            windows_output,
        );
        if !worktrunk_status.exists && all_windows_output_empty {
            let windows_command = tmux.list_windows(&task_snapshot.tmux_session);
            if let Ok(output) = runner.run(&windows_command) {
                if output.status_code == 0 {
                    worktrunk_status = TmuxAdapter::parse_worktrunk_status(
                        &task_snapshot.worktrunk_window,
                        &task_snapshot.worktree_path.display().to_string(),
                        &output.stdout,
                    );
                }
            }
        }
        changed |= task_snapshot.worktrunk_status.as_ref() != Some(&worktrunk_status);

        let worktrunk_status_changed =
            task_snapshot.worktrunk_status.as_ref() != Some(&worktrunk_status);
        let had_stale_worktrunk_missing =
            task_snapshot.has_side_flag(ajax_core::models::SideFlag::WorktrunkMissing);
        changed |= worktrunk_status_changed || had_stale_worktrunk_missing;

        if worktrunk_status_changed || had_stale_worktrunk_missing {
            context
                .registry
                .update_worktrunk_status(&task_id, Some(worktrunk_status.clone()))
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
        }
        refresh_runtime_projection_from_tmux_probe(context, &task_id, &mut changed);

        if !worktrunk_status.exists {
            if let Some(task) = context.registry.get_task_mut(&task_id) {
                live::apply_observation(
                    task,
                    LiveObservation::new(LiveStatusKind::WorktrunkMissing, "worktrunk missing"),
                );
                refresh_cached_annotations(task);
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
                    refresh_cached_annotations(task);
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
            refresh_cached_annotations(task);
            changed |= *task != previous;
        }
    }

    Ok(changed)
}

fn should_probe_live_substrate(task: &Task) -> bool {
    matches!(
        task.lifecycle_status,
        LifecycleStatus::Provisioning
            | LifecycleStatus::Active
            | LifecycleStatus::Waiting
            | LifecycleStatus::Reviewable
    ) || task.has_side_flag(ajax_core::models::SideFlag::AgentRunning)
        || task.has_side_flag(ajax_core::models::SideFlag::TmuxMissing)
        || task.has_side_flag(ajax_core::models::SideFlag::WorktrunkMissing)
}

fn refresh_runtime_projection_from_tmux_probe(
    context: &mut CommandContext<InMemoryRegistry>,
    task_id: &TaskId,
    changed: &mut bool,
) {
    if let Some(task) = context.registry.get_task_mut(task_id) {
        let previous = task.runtime_projection.clone();
        task.refresh_runtime_projection_from_source(RuntimeObservationSource::TmuxProbe);
        *changed |= task.runtime_projection != previous;
    }
}

fn has_unregistered_ajax_session(
    context: &CommandContext<InMemoryRegistry>,
    sessions_output: &str,
) -> bool {
    sessions_output.lines().map(str::trim).any(|session| {
        context.config.repos.iter().any(|repo| {
            let prefix = format!("ajax-{}-", repo.name);
            let Some(handle) = session.strip_prefix(&prefix) else {
                return false;
            };
            if handle.is_empty() {
                return false;
            }
            !registered_task_exists(context, &repo.name, handle)
        })
    })
}

fn registered_task_exists(
    context: &CommandContext<InMemoryRegistry>,
    repo_name: &str,
    handle: &str,
) -> bool {
    context.registry.list_tasks().into_iter().any(|task| {
        task.repo == repo_name
            && task.handle == handle
            && task.lifecycle_status != LifecycleStatus::Removed
    })
}

fn recover_missing_tasks_from_substrate<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    sessions_output: &str,
) -> Result<bool, CliError> {
    if context.config.repos.is_empty() {
        return Ok(false);
    }

    let git = GitAdapter::new("git");
    let mut changed = false;

    for repo in context.config.repos.clone() {
        let command = git.list_worktrees(&repo.path.display().to_string());
        let output = match runner.run(&command) {
            Ok(output) if output.status_code == 0 => output.stdout,
            Ok(_) | Err(_) => continue,
        };

        for worktree in GitAdapter::parse_worktrees(&output) {
            let Some(branch) = worktree.branch.as_deref() else {
                continue;
            };
            let Some(handle) = branch.strip_prefix("ajax/") else {
                continue;
            };
            if handle.is_empty() {
                continue;
            }

            if registered_task_exists(context, &repo.name, handle) {
                continue;
            }

            let task_id = TaskId::new(format!("{}/{}", repo.name, handle));
            let tmux_session = format!("ajax-{}-{handle}", repo.name);
            let tmux_status = TmuxAdapter::parse_session_status(&tmux_session, sessions_output);
            if !tmux_status.exists {
                continue;
            }

            let mut task = Task::new(
                task_id.clone(),
                repo.name.clone(),
                handle.to_string(),
                handle.replace('-', " "),
                branch.to_string(),
                repo.default_branch.clone(),
                worktree.path,
                tmux_session,
                "worktrunk",
                AgentClient::Codex,
            );
            task.lifecycle_status = LifecycleStatus::Active;
            task.git_status = Some(GitStatus {
                worktree_exists: true,
                branch_exists: true,
                current_branch: Some(branch.to_string()),
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: None,
            });
            task.tmux_status = Some(TmuxStatus::present(task.tmux_session.clone()));
            context
                .registry
                .create_task(task)
                .map_err(|error| CliError::CommandFailed(error.to_string()))?;
            changed = true;
        }
    }

    Ok(changed)
}

fn refresh_cached_annotations(task: &mut ajax_core::models::Task) {
    task.annotations = ajax_core::attention::annotate(task);
}

pub(crate) fn refresh_cockpit_snapshot<R: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut R,
    state_changed: &mut bool,
) -> Result<CockpitSnapshot, CliError> {
    *state_changed |= refresh_live_context(context, runner)?;
    Ok(build_cockpit_snapshot(context))
}

pub(crate) fn build_cockpit_snapshot(
    context: &CommandContext<InMemoryRegistry>,
) -> CockpitSnapshot {
    let view = commands::cockpit_view(context);
    CockpitSnapshot {
        repos: view.repos,
        cards: view.cards,
        inbox: view.inbox,
    }
}

struct InteractiveCockpitHandler<'a, R: CommandRunner> {
    context: &'a mut CommandContext<InMemoryRegistry>,
    runner: &'a mut R,
    state_changed: &'a mut bool,
}

impl<R: CommandRunner> ajax_tui::CockpitEventHandler for InteractiveCockpitHandler<'_, R> {
    fn on_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_action(item, self.context, self.runner, self.state_changed)
    }

    fn on_confirmed_action(
        &mut self,
        item: &ajax_core::models::CockpitActionItem,
    ) -> std::io::Result<ajax_tui::ActionOutcome> {
        tui_cockpit_confirmed_action(item, self.context, self.runner, self.state_changed)
    }

    fn on_refresh(&mut self) -> std::io::Result<Option<CockpitSnapshot>> {
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

#[cfg(test)]
mod tests {
    use super::refresh_cockpit_snapshot;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
            LiveStatusKind, OperatorAction, RuntimeHealth, RuntimeObservationSource, SideFlag,
            Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry},
    };

    #[derive(Default)]
    struct LiveRefreshRunner;

    impl CommandRunner for LiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "Do you want to proceed? y/n\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn context_with_active_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        registry.create_task(task).unwrap();

        CommandContext::new(config, registry)
    }

    #[derive(Default)]
    struct EmptyTmuxRunner;

    impl CommandRunner for EmptyTmuxRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn context_with_cached_running_task() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "working on task",
        ));
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        context
    }

    #[test]
    fn live_refresh_updates_cached_annotations_for_cockpit_inbox() {
        let mut context = context_with_active_task();
        let mut runner = LiveRefreshRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        assert_eq!(
            snapshot.cards[0].live_summary.as_deref(),
            Some("waiting for approval")
        );
        assert!(snapshot.inbox.items.iter().any(|item| {
            item.reason == "waiting_for_approval" && item.task_handle == "web/fix-login"
        }));
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .annotations
            .iter()
            .any(|annotation| annotation.evidence.label() == "waiting for approval"));
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.runtime_projection.health, RuntimeHealth::Healthy);
        assert_eq!(
            task.runtime_projection.source,
            RuntimeObservationSource::TmuxProbe
        );
    }

    #[test]
    fn live_refresh_marks_cached_running_task_invalid_when_tmux_sessions_are_empty() {
        let mut context = context_with_cached_running_task();
        let mut runner = EmptyTmuxRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
        assert!(!snapshot
            .cards
            .iter()
            .any(|card| card.qualified_handle == "web/fix-login"));
        assert!(snapshot.inbox.items.is_empty());
        assert!(ajax_core::commands::inbox(&context)
            .items
            .iter()
            .any(|item| {
                item.task_handle == "web/fix-login" && item.action == OperatorAction::Drop
            }));
    }

    #[derive(Default)]
    struct SubstrateRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for SubstrateRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-code\nHEAD 2222222\nbranch refs/heads/ajax/code\n\n"
                }
                [command, ..] if command == "list-sessions" => {
                    "ajax-web-existing\najax-web-code\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-code\tworktrunk\t/Users/matt/projects/web__worktrees/ajax-code\n"
                }
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn refresh_recovers_missing_registry_task_from_existing_ajax_worktree_and_tmux() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut existing = Task::new(
            TaskId::new("task-1"),
            "web",
            "existing",
            "existing",
            "ajax/existing",
            "main",
            "/Users/matt/projects/web__worktrees/ajax-existing",
            "ajax-web-existing",
            "worktrunk",
            AgentClient::Codex,
        );
        existing.lifecycle_status = LifecycleStatus::Active;
        registry.create_task(existing).unwrap();
        let mut context = CommandContext::new(config, registry);
        let mut runner = SubstrateRecoveryRunner::default();
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        assert!(snapshot
            .cards
            .iter()
            .any(|card| card.qualified_handle == "web/code"));
        let task = context
            .registry
            .get_task(&TaskId::new("web/code"))
            .expect("missing Ajax worktree should be recovered into the registry");
        assert_eq!(task.branch, "ajax/code");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-code"
        );
        assert_eq!(task.tmux_session, "ajax-web-code");
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    }

    #[derive(Default)]
    struct CountingLiveRefreshRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for CountingLiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [command, ..] if command == "list-windows" => {
                    "worktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn live_refresh_skips_window_and_pane_probes_for_non_live_tasks() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Cleanable;
        task.tmux_status = Some(ajax_core::models::TmuxStatus::present(
            task.tmux_session.clone(),
        ));
        task.worktrunk_status = Some(ajax_core::models::WorktrunkStatus {
            exists: true,
            window_name: task.worktrunk_window.clone(),
            current_path: task.worktree_path.clone(),
            points_at_expected_path: true,
        });

        let mut runner = CountingLiveRefreshRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert!(runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-sessions")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "list-windows")
        ));
        assert!(!runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
    }
}
