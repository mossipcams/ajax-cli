//! Browser-submitted operator actions.

use ajax_core::{
    adapters::{environment::origin_fetch_age, CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, NewTaskRequest, OpenMode},
    models::{LifecycleStatus, OperatorAction, SideFlag},
    registry::Registry,
    remediation::{self, RemediationError},
    task_operations::{
        drop_task::{
            execute_drop_task_operation, plan_drop_confirmation, plan_drop_task_operation,
            DropTaskCompletion,
        },
        start::plan_start_task_operation_with_observation,
        task_command::{
            execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
        },
    },
};

use crate::adapters::skills::resolve_skill_path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperateRequest {
    pub task_handle: String,
    pub action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct StartTaskRequest {
    pub repo: String,
    pub title: String,
    pub agent: String,
    #[serde(default)]
    pub request_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperateOutcome {
    pub state_changed: bool,
    pub output: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OperateError {
    UnknownAction(String),
    UnsupportedCapability(&'static str),
    Command(CommandError, bool),
}

pub fn operate<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: OperateRequest,
) -> Result<OperateOutcome, OperateError> {
    if remediation::is_remediation_action(&request.action) {
        return run_remediation(context, runner, &request.task_handle, &request.action);
    }

    let Some(action) = OperatorAction::from_label(&request.action) else {
        return Err(OperateError::UnknownAction(request.action));
    };

    match action {
        OperatorAction::Drop => execute_drop(context, runner, &request.task_handle, true),
        OperatorAction::Start => Err(OperateError::UnsupportedCapability(
            "start uses the dedicated Web Cockpit new-task operation",
        )),
        OperatorAction::Review
        | OperatorAction::Ship
        | OperatorAction::Repair
        | OperatorAction::Resume => {
            let kind = task_command_kind(action)?;
            execute_task_command(context, runner, kind, &request.task_handle)
        }
    }
}

/// Test convenience: `start_task_with_checkpoint` with a noop checkpoint.
/// Production callers (ajax-cli) always supply a real checkpoint.
#[cfg(test)]
pub fn start_task<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
) -> Result<OperateOutcome, OperateError> {
    start_task_with_checkpoint(context, runner, request, |_| Ok(()))
}

pub fn start_task_with_checkpoint<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
    checkpoint: impl FnMut(&CommandContext<R>) -> Result<(), ajax_core::commands::CommandError>,
) -> Result<OperateOutcome, OperateError> {
    if request.title.trim().is_empty() {
        return Err(OperateError::UnsupportedCapability(
            "start requires a non-empty task title",
        ));
    }
    if !supported_start_agent(&request.agent) {
        return Err(OperateError::UnsupportedCapability("unsupported agent"));
    }

    let core_request = NewTaskRequest {
        repo: request.repo,
        title: request.title,
        agent: request.agent,
    };
    let observation = start_plan_observation(context, &core_request);
    let (_intent, plan) =
        plan_start_task_operation_with_observation(context, core_request.clone(), observation)
            .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    ajax_core::task_operations::start::execute_start_task_operation_with_checkpoint(
        context,
        runner,
        &core_request,
        &plan,
        confirmed,
        OpenMode::NoAttach,
        checkpoint,
    )
    .map_err(|error| OperateError::Command(error, true))?;

    Ok(OperateOutcome {
        state_changed: true,
        output: format!("started task: {}", core_request.title),
    })
}

fn start_plan_observation<R: Registry>(
    context: &CommandContext<R>,
    request: &NewTaskRequest,
) -> commands::StartPlanObservation {
    let origin_fetch_age = context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == request.repo)
        .and_then(|repo| origin_fetch_age(&repo.path));

    commands::StartPlanObservation { origin_fetch_age }
}

/// Single agent allowlist for web task starts; the route pre-check and the
/// slice validation must never disagree.
pub fn supported_start_agent(agent: &str) -> bool {
    matches!(agent, "codex" | "claude" | "cursor" | "opencode")
}

fn execute_task_command<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    kind: TaskCommandKind,
    task_handle: &str,
) -> Result<OperateOutcome, OperateError> {
    if matches!(
        kind,
        TaskCommandKind::Resume | TaskCommandKind::Review | TaskCommandKind::Repair
    ) {
        let _ = commands::refresh_git_substrate_evidence(context, runner);
    }

    let open_mode = if matches!(kind, TaskCommandKind::Resume | TaskCommandKind::Repair) {
        OpenMode::NoAttach
    } else {
        OpenMode::Attach
    };
    let plan = plan_task_command_operation(context, kind, task_handle, open_mode)
        .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    let (outputs, state_changed) =
        execute_task_command_operation(context, kind, task_handle, &plan, confirmed, runner)
            .map_err(|(error, state_changed)| OperateError::Command(error, state_changed))?;

    Ok(OperateOutcome {
        state_changed,
        output: format_execution_outputs(&outputs),
    })
}

fn execute_drop<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    confirmed: bool,
) -> Result<OperateOutcome, OperateError> {
    let confirmation_plan = plan_drop_confirmation(context, task_handle)
        .map_err(|error| OperateError::Command(error, false))?;
    if !confirmation_plan.blocked_reasons.is_empty() {
        return Err(OperateError::Command(
            CommandError::PlanBlocked(confirmation_plan.blocked_reasons),
            false,
        ));
    }

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| {
            OperateError::Command(CommandError::TaskNotFound(task_handle.to_string()), false)
        })?;

    let resuming_incomplete = task.lifecycle_status == LifecycleStatus::TeardownIncomplete;
    let can_observe_before_confirmation = matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged | LifecycleStatus::Cleanable
    ) && !task.has_side_flag(SideFlag::Dirty)
        && !task.has_side_flag(SideFlag::Conflicted)
        && !task.has_side_flag(SideFlag::Unpushed)
        && task.git_status.as_ref().is_none_or(|status| {
            !status.dirty && !status.conflicted && status.unpushed_commits == 0
        });

    if confirmation_plan.requires_confirmation
        && !confirmed
        && !resuming_incomplete
        && !can_observe_before_confirmation
    {
        return Err(OperateError::Command(
            CommandError::ConfirmationRequired,
            false,
        ));
    }

    let operation = plan_drop_task_operation(context, task_handle, runner)
        .map_err(|error| OperateError::Command(error, false))?;
    let operation_confirmed = confirmed || resuming_incomplete || can_observe_before_confirmation;
    let (outputs, completion) =
        execute_drop_task_operation(context, task_handle, operation, operation_confirmed, runner)
            .map_err(|error| OperateError::Command(error, true))?;

    let output = match completion {
        DropTaskCompletion::Removed => {
            if outputs.is_empty() {
                format!("removed task: {task_handle}")
            } else {
                format_execution_outputs(&outputs)
            }
        }
        DropTaskCompletion::TeardownIncomplete {
            failed_step,
            detail,
        } => {
            return Err(OperateError::Command(
                CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: "drop".to_string(),
                    status_code: 1,
                    stderr: ajax_core::commands::format_drop_teardown_incomplete_message(
                        task_handle,
                        failed_step,
                        &detail,
                    ),
                    cwd: None,
                }),
                true,
            ));
        }
    };

    Ok(OperateOutcome {
        state_changed: true,
        output,
    })
}

fn task_command_kind(action: OperatorAction) -> Result<TaskCommandKind, OperateError> {
    match action {
        OperatorAction::Review => Ok(TaskCommandKind::Review),
        OperatorAction::Ship => Ok(TaskCommandKind::Ship),
        OperatorAction::Repair => Ok(TaskCommandKind::Repair),
        OperatorAction::Resume => Ok(TaskCommandKind::Resume),
        OperatorAction::Start | OperatorAction::Drop => Err(OperateError::UnsupportedCapability(
            "action is handled by a dedicated web operation",
        )),
    }
}

pub fn format_execution_outputs(outputs: &[CommandOutput]) -> String {
    outputs
        .iter()
        .filter_map(|output| {
            let stdout = output.stdout.trim();
            let stderr = output.stderr.trim();
            match (stdout.is_empty(), stderr.is_empty()) {
                (true, true) => None,
                (false, true) => Some(stdout.to_string()),
                (true, false) => Some(stderr.to_string()),
                (false, false) => Some(format!("{stdout}\n{stderr}")),
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn run_remediation<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
    remediation_id: &str,
) -> Result<OperateOutcome, OperateError> {
    let skill_name = match remediation_id {
        remediation::FIX_CI => "gh-fix-ci",
        remediation::RESOLVE_MERGE_CONFLICTS => "resolve-merge-conflicts",
        _ => return Err(OperateError::UnknownAction(remediation_id.to_string())),
    };
    let skill_path = resolve_skill_path(skill_name).ok_or(OperateError::UnsupportedCapability(
        "required agent skill is not installed on the companion host",
    ))?;
    let outcome = remediation::execute_remediation(
        context,
        runner,
        task_handle,
        remediation_id,
        &skill_path.display().to_string(),
    )
    .map_err(remediation_error_to_operate)?;
    Ok(OperateOutcome {
        state_changed: outcome.state_changed,
        output: outcome.output,
    })
}

fn remediation_error_to_operate(error: RemediationError) -> OperateError {
    match error {
        RemediationError::UnknownRemediation(id) => OperateError::UnknownAction(id),
        RemediationError::TaskNotFound(handle) => {
            OperateError::Command(CommandError::TaskNotFound(handle), false)
        }
        RemediationError::UnsupportedCapability(message) => {
            OperateError::UnsupportedCapability(message)
        }
        RemediationError::CommandRun(message) => OperateError::Command(
            CommandError::CommandRun(ajax_core::adapters::CommandRunError::SpawnFailed(message)),
            false,
        ),
    }
}

pub fn format_operate_error(error: &OperateError) -> String {
    match error {
        OperateError::UnknownAction(action) => format!("unknown action: {action}"),
        OperateError::UnsupportedCapability(message) => (*message).to_string(),
        OperateError::Command(error, _) => format_command_error(error),
    }
}

fn format_command_error(error: &CommandError) -> String {
    match error {
        CommandError::ConfirmationRequired => {
            "confirmation required — tap again to confirm".to_string()
        }
        CommandError::PlanBlocked(reasons) => reasons.join("; "),
        CommandError::TaskNotFound(handle) => format!("task not found: {handle}"),
        CommandError::RepoNotFound(repo) => format!("repo not found: {repo}"),
        CommandError::Registry(error) => error.to_string(),
        CommandError::CommandRun(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{format_execution_outputs, operate, OperateError, OperateRequest};
    use ajax_core::remediation;
    use ajax_core::{
        adapters::{CommandOutput, RecordingCommandRunner},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{LifecycleStatus, LiveObservation, LiveStatusKind, TmuxStatus},
        registry::{InMemoryRegistry, Registry as _},
    };

    fn context_with_reviewable_task() -> CommandContext<InMemoryRegistry> {
        let mut task = crate::test_support::fix_login_task();
        task.lifecycle_status = LifecycleStatus::Reviewable;
        crate::test_support::context_with_tasks(&["web"], vec![task])
    }

    #[test]
    fn operate_slice_delegates_resume_to_core_operation_without_attach() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();
        let outcome = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "resume".to_string(),
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        assert!(
            !runner
                .commands()
                .iter()
                .any(|command| command.mode == ajax_core::adapters::CommandMode::InheritStdio),
            "resume must not attach to the task terminal"
        );
    }

    #[test]
    fn operate_slice_delegates_review_to_core_operation_and_returns_output() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();
        let outcome = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "review".to_string(),
            },
        )
        .unwrap();

        assert!(!outcome.state_changed);
        assert!(outcome.output.is_empty());
        assert_eq!(runner.commands().len(), 1);
    }

    #[test]
    fn operate_slice_runs_fix_ci_remediation_via_tmux() {
        let mut task = crate::test_support::fix_login_task();
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });
        let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let mut runner = RecordingCommandRunner::default();

        let home = std::env::temp_dir().join(format!("ajax-skill-{}", std::process::id()));
        let skill_dir = home.join("gh-fix-ci");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
        std::env::set_var("AJAX_SKILL_ROOT", &home);

        let outcome = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: remediation::FIX_CI.to_string(),
            },
        )
        .unwrap();

        std::env::remove_var("AJAX_SKILL_ROOT");
        let _ = std::fs::remove_dir_all(&home);

        assert!(!outcome.state_changed);
        assert!(outcome.output.contains("Fix CI"));
        assert_eq!(runner.commands().len(), 1);
    }

    #[test]
    fn format_execution_outputs_prefers_stdout() {
        let text = format_execution_outputs(&[CommandOutput {
            status_code: 0,
            stdout: " diff stat\n".to_string(),
            stderr: String::new(),
        }]);

        assert_eq!(text, "diff stat");
    }

    fn agent_send_keys_line(commands: &[ajax_core::adapters::CommandSpec]) -> &str {
        commands
            .iter()
            .find(|command| {
                command.program == "tmux" && command.args.first() == Some(&"send-keys".to_string())
            })
            .map(|command| command.args[3].as_str())
            .expect("expected tmux send-keys command")
    }

    #[test]
    fn start_task_cursor_agent_command_uses_agent_subcommand_without_cd() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "cursor".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap();

        let line = agent_send_keys_line(runner.commands());
        assert_eq!(
            line,
            "ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- cursor agent"
        );
        assert!(!line.contains("--cd"));
    }

    #[test]
    fn start_task_opencode_agent_command_runs_opencode_in_task_window() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "opencode".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap();

        // opencode opens in the current directory; the task window's cwd is
        // the worktree, so the launch needs no extra arguments.
        assert_eq!(
            agent_send_keys_line(runner.commands()),
            "ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- opencode"
        );
    }

    #[test]
    fn start_task_claude_agent_command_omits_cd_flag() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "claude".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap();

        assert_eq!(
            agent_send_keys_line(runner.commands()),
            "ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- claude"
        );
    }

    #[test]
    fn start_task_creates_a_new_task_in_the_registry() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        let outcome = super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap();

        assert!(outcome.state_changed);
        let tasks = context.registry.list_tasks();
        assert!(
            tasks
                .iter()
                .any(|task| task.qualified_handle() == "web/fix-login"),
            "expected new task in registry, got {:?}",
            tasks
                .iter()
                .map(|t| t.qualified_handle())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn start_task_rejects_empty_title() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        let error = super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "   ".to_string(),
                agent: "codex".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnsupportedCapability("start requires a non-empty task title")
        );
        assert!(runner.commands().is_empty());
        assert!(context.registry.list_tasks().is_empty());
    }

    #[test]
    fn start_task_rejects_unsupported_agent() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        let error = super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "/bin/sh".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnsupportedCapability("unsupported agent")
        );
        assert!(runner.commands().is_empty());
        assert!(context.registry.list_tasks().is_empty());
    }

    #[test]
    fn start_task_surfaces_unknown_repo_as_command_error() {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        let error = super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "missing".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap_err();

        assert!(
            matches!(error, OperateError::Command(_, false)),
            "{error:?}"
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn start_task_skips_fetch_when_origin_fetch_is_fresh() {
        let root = std::env::temp_dir().join(format!(
            "ajax-web-start-task-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let mut file = std::fs::File::create(root.join(".git/FETCH_HEAD")).unwrap();
        use std::io::Write;
        writeln!(file, "ref: origin/main").unwrap();
        let mut context = context_with_repo_path(&root);
        let mut runner = RecordingCommandRunner::default();

        super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: String::new(),
            },
        )
        .unwrap();

        assert!(
            runner
                .commands()
                .iter()
                .all(|command| !command.args.iter().any(|arg| arg == "fetch")),
            "unexpected fetch command: {:?}",
            runner.commands()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    fn context_with_managed_repo() -> CommandContext<InMemoryRegistry> {
        crate::test_support::context_with_tasks(&["web"], vec![])
    }

    fn context_with_repo_path(repo_path: &std::path::Path) -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new(
                "web",
                repo_path.display().to_string(),
                "main",
            )],
            ..Config::default()
        };
        CommandContext::new(config, InMemoryRegistry::default())
    }
}
