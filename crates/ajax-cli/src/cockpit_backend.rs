use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    registry::InMemoryRegistry,
    runtime_refresh::refresh_runtime_context,
};
use ajax_tui::CockpitSnapshot;
use clap::ArgMatches;
use std::time::Duration;

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
            #[cfg(test)]
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
    refresh_runtime_context(context, runner).map_err(crate::command_error)
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
    let view = commands::rebuild_cockpit_view(context);
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

    #[test]
    fn cockpit_backend_live_refresh_delegates_runtime_refresh_to_core() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let refresh_live_context = source
            .split("pub(crate) fn refresh_live_context")
            .nth(1)
            .and_then(|source| {
                source
                    .split("pub(crate) fn refresh_cockpit_snapshot")
                    .next()
            })
            .unwrap();
        let core_refresh = ["refresh_runtime", "_context"].concat();

        assert!(refresh_live_context.contains(&core_refresh));
        assert!(!refresh_live_context.contains("TmuxAdapter::new"));
        assert!(!refresh_live_context.contains("GitAdapter::new"));
    }

    #[test]
    fn cockpit_snapshot_build_explicitly_rebuilds_core_projection() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/cockpit_backend.rs"),
        )
        .unwrap();
        let build_cockpit_snapshot = source
            .split("pub(crate) fn build_cockpit_snapshot")
            .nth(1)
            .and_then(|source| source.split("struct InteractiveCockpitHandler").next())
            .unwrap();
        let implicit_view_read = ["commands::", "cockpit_view"].concat();

        assert!(build_cockpit_snapshot.contains("rebuild_cockpit_view"));
        assert!(!build_cockpit_snapshot.contains(&implicit_view_read));
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
    fn live_refresh_clears_stale_input_when_codex_prompt_is_working() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Waiting;
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
        task.annotations = ajax_core::attention::annotate(task);
        let mut runner = WorkingPromptRunner;
        let mut state_changed = false;

        let snapshot =
            refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed).unwrap();

        assert!(state_changed);
        let card = snapshot
            .cards
            .iter()
            .find(|card| card.qualified_handle == "web/fix-login")
            .expect("task should stay visible in cockpit");
        assert_eq!(card.status_label, "agent running");
        assert_eq!(card.ui_state, ajax_core::ui_state::UiState::Running);
        assert!(card.annotations.is_empty(), "{:?}", card.annotations);
        assert!(!snapshot
            .inbox
            .items
            .iter()
            .any(|item| item.task_handle == "web/fix-login"));

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
        assert!(!task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::AgentRunning)
        );
    }

    struct WorkingPromptRunner;

    impl CommandRunner for WorkingPromptRunner {
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
                [command, ..] if command == "capture-pane" => {
                    "• Working (3m 00s • esc to interrupt) · 1 background terminal running · /ps to…\n\n› Improve documentation in @filename\n\n  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-ci\n"
                }
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

    #[test]
    fn live_refresh_marks_cached_present_tmux_missing_even_after_fresh_command_result() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.runtime_projection = ajax_core::models::RuntimeProjection::new(
            RuntimeHealth::Healthy,
            std::time::SystemTime::now(),
            RuntimeObservationSource::CommandResult,
        );
        let mut runner = EmptyTmuxRunner;

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .expect("fixture task should remain registered");

        assert!(changed);
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
    }

    #[test]
    fn live_refresh_reprobes_error_task_with_recoverable_conflict_prompt() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Error;
        task.agent_status = AgentRuntimeStatus::Blocked;
        task.add_side_flag(SideFlag::Conflicted);
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::MergeConflict,
            "merge conflict needs attention",
        ));
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        let mut runner = SpaghettiRecoveryRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();
        let task = context
            .registry
            .get_task(&TaskId::new("task-1"))
            .expect("fixture task should remain registered");

        assert!(changed);
        assert!(runner.commands.iter().any(
            |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
        ));
        assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::NeedsInput));
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::WaitingForInput)
        );
    }

    #[derive(Default)]
    struct SpaghettiRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for SpaghettiRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\tworktrunk\t/tmp/worktrees/web-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => {
                    "› Improve documentation in @filename\n\n  gpt-5.5 high · ~/Desktop/Projects/ajax-cli__worktrees/ajax-spaghetti\n"
                }
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
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
    struct OrphanWorktreeRecoveryRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for OrphanWorktreeRecoveryRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, action, flag]
                    if repo == "/Users/matt/projects/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /Users/matt/projects/web__worktrees/ajax-orphan\nHEAD 2222222\nbranch refs/heads/ajax/orphan\n\n"
                }
                [command, ..] if command == "list-sessions" => "ajax-web-existing\n",
                [command, ..] if command == "list-windows" => "",
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
    fn refresh_recovers_missing_registry_task_from_orphaned_ajax_worktree_without_tmux() {
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
        let mut runner = OrphanWorktreeRecoveryRunner::default();

        let changed = super::refresh_live_context(&mut context, &mut runner).unwrap();

        assert!(changed);
        let task = context
            .registry
            .get_task(&TaskId::new("web/orphan"))
            .expect("orphaned Ajax worktree should be recovered into the registry");
        assert_eq!(task.branch, "ajax/orphan");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-orphan"
        );
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists && status.branch_exists));
        assert_eq!(
            task.tmux_status.as_ref().map(|status| status.exists),
            Some(false)
        );
        assert_eq!(
            task.live_status.as_ref().map(|status| status.kind),
            Some(LiveStatusKind::TmuxMissing)
        );
        assert!(task.has_side_flag(SideFlag::TmuxMissing));
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

    #[test]
    fn live_refresh_does_not_probe_generic_error_task_without_live_attention() {
        let mut context = context_with_active_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .expect("fixture task should exist");
        task.lifecycle_status = LifecycleStatus::Error;
        task.agent_status = AgentRuntimeStatus::Blocked;
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
