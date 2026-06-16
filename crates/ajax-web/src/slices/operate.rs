//! Browser-submitted operator actions.

use ajax_core::{
    adapters::{environment::origin_fetch_age, CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, NewTaskRequest, OpenMode},
    models::OperatorAction,
    recommended::{task_action_decisions, RemediationId, TaskActionDecision, TaskActionId},
    registry::Registry,
    slices::drop,
    slices::remediate::{self, RemediationError},
    slices::{repair, resume, review, ship},
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
    UnavailableAction {
        action: TaskActionId,
        reason: String,
    },
    UnsupportedCapability(&'static str),
    Command(CommandError, bool),
}

pub fn operate<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: OperateRequest,
) -> Result<OperateOutcome, OperateError> {
    let Some(action_id) = TaskActionId::from_compatibility_label(&request.action) else {
        return Err(OperateError::UnknownAction(request.action));
    };
    if action_id == TaskActionId::BuiltIn(OperatorAction::Start) {
        return Err(OperateError::UnsupportedCapability(
            "start uses the dedicated Web Cockpit new-task operation",
        ));
    }
    let decision = current_action_decision(context, &request.task_handle, action_id)?;
    if !decision.is_available() {
        return Err(OperateError::UnavailableAction {
            action: action_id,
            reason: decision.reason,
        });
    }

    match action_id {
        TaskActionId::Remediation(remediation_id) => {
            run_remediation(context, runner, &request.task_handle, remediation_id)
        }
        TaskActionId::BuiltIn(action) => match action {
            OperatorAction::Drop => execute_drop(context, runner, &request.task_handle, true),
            OperatorAction::Resume => Err(OperateError::UnsupportedCapability(
                "resume requires native cockpit",
            )),
            OperatorAction::Start => unreachable!("start is handled before task decision lookup"),
            OperatorAction::Review | OperatorAction::Ship | OperatorAction::Repair => {
                execute_task_command(context, runner, action, &request.task_handle)
            }
        },
    }
}

fn current_action_decision<R: Registry>(
    context: &CommandContext<R>,
    task_handle: &str,
    action_id: TaskActionId,
) -> Result<TaskActionDecision, OperateError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| {
            OperateError::Command(CommandError::TaskNotFound(task_handle.to_string()), false)
        })?;
    task_action_decisions(task)
        .into_iter()
        .find(|decision| decision.id == action_id)
        .ok_or_else(|| OperateError::UnavailableAction {
            action: action_id,
            reason: "action is no longer available".to_string(),
        })
}

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

    let core_request = NewTaskRequest {
        repo: request.repo,
        title: request.title,
        agent: request.agent,
    };
    let observation = start_plan_observation(context, &core_request);
    let (_intent, plan) =
        ajax_core::slices::start::plan_with_observation(context, core_request.clone(), observation)
            .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    ajax_core::slices::start::execute_with_checkpoint(
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

fn execute_task_command<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    action: OperatorAction,
    task_handle: &str,
) -> Result<OperateOutcome, OperateError> {
    if matches!(
        action,
        OperatorAction::Resume | OperatorAction::Review | OperatorAction::Repair
    ) {
        let _ = commands::refresh_git_substrate_evidence(context, runner);
    }

    let open_mode = if matches!(action, OperatorAction::Resume | OperatorAction::Repair) {
        OpenMode::NoAttach
    } else {
        OpenMode::Attach
    };
    let plan = plan_task_action(context, action, task_handle, open_mode)
        .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    let (outputs, state_changed) =
        execute_task_action(context, action, task_handle, &plan, confirmed, runner)
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
    let confirmation_plan = drop::plan_confirmation(context, task_handle)
        .map_err(|error| OperateError::Command(error, false))?;

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .ok_or_else(|| {
            OperateError::Command(CommandError::TaskNotFound(task_handle.to_string()), false)
        })?;

    let operation_confirmed = ajax_core::slices::drop::resolve_execution_confirmation(
        task,
        &confirmation_plan,
        confirmed,
    )
    .map_err(|error| OperateError::Command(error, false))?;

    let operation = drop::plan_operation(context, task_handle, runner)
        .map_err(|error| OperateError::Command(error, false))?;
    let (outputs, completion) =
        drop::execute(context, task_handle, operation, operation_confirmed, runner)
            .map_err(|error| OperateError::Command(error, true))?;

    let output = match completion {
        ajax_core::slices::drop::DropTaskCompletion::Removed => {
            if outputs.is_empty() {
                format!("removed task: {task_handle}")
            } else {
                format_execution_outputs(&outputs)
            }
        }
        ajax_core::slices::drop::DropTaskCompletion::TeardownIncomplete {
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

fn plan_task_action<R: Registry>(
    context: &CommandContext<R>,
    action: OperatorAction,
    task_handle: &str,
    open_mode: OpenMode,
) -> Result<ajax_core::use_cases::CommandPlan, CommandError> {
    match action {
        OperatorAction::Resume => resume::plan(context, task_handle, open_mode),
        OperatorAction::Review => review::plan(context, task_handle),
        OperatorAction::Repair => repair::plan(context, task_handle, open_mode),
        OperatorAction::Ship => ship::plan(context, task_handle),
        OperatorAction::Start | OperatorAction::Drop => Err(CommandError::PlanBlocked(vec![
            "action is handled by a dedicated web operation".to_string(),
        ])),
    }
}

fn execute_task_action<R: Registry>(
    context: &mut CommandContext<R>,
    action: OperatorAction,
    task_handle: &str,
    plan: &ajax_core::use_cases::CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    match action {
        OperatorAction::Resume => resume::execute(context, task_handle, plan, confirmed, runner),
        OperatorAction::Review => review::execute(context, task_handle, plan, confirmed, runner),
        OperatorAction::Repair => repair::execute(context, task_handle, plan, confirmed, runner),
        OperatorAction::Ship => ship::execute(context, task_handle, plan, confirmed, runner),
        OperatorAction::Start | OperatorAction::Drop => Err((
            CommandError::PlanBlocked(vec![
                "action is handled by a dedicated web operation".to_string()
            ]),
            false,
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
    remediation_id: RemediationId,
) -> Result<OperateOutcome, OperateError> {
    let skill_name = match remediation_id {
        RemediationId::FixCi => "gh-fix-ci",
        RemediationId::ResolveMergeConflicts => "resolve-merge-conflicts",
    };
    let skill_path = resolve_skill_path(skill_name).ok_or(OperateError::UnsupportedCapability(
        "required agent skill is not installed on the companion host",
    ))?;
    let outcome = remediate::execute_remediation(
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
        OperateError::UnavailableAction { reason, .. } => reason.clone(),
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
        models::{
            AgentClient, LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction, Task,
            TaskId, TmuxStatus,
        },
        recommended::{RemediationId, TaskActionId},
        registry::{InMemoryRegistry, Registry},
    };

    fn context_with_reviewable_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Reviewable;
        registry.create_task(task).unwrap();
        CommandContext::new(config, registry)
    }

    #[test]
    fn operate_slice_rejects_resume_in_favor_of_sync() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();
        let error = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "resume".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnsupportedCapability("resume requires native cockpit")
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn web_operate_rejects_core_unavailable_action_with_same_reason() {
        let mut context = context_with_reviewable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;
        let mut runner = RecordingCommandRunner::default();

        let error = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "ship".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnavailableAction {
                action: TaskActionId::BuiltIn(OperatorAction::Ship),
                reason: "merge requires reviewable or mergeable lifecycle".to_string(),
            }
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn web_operate_reports_needs_terminal_for_core_available_resume() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();

        let error = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "resume".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnsupportedCapability("resume requires native cockpit")
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
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });
        registry.create_task(task).unwrap();
        let mut context = CommandContext::new(config, registry);
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
    fn web_operate_dispatches_available_remediation_through_task_action_id() {
        assert_eq!(
            TaskActionId::from_compatibility_label(remediation::FIX_CI),
            Some(TaskActionId::Remediation(RemediationId::FixCi))
        );
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
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        CommandContext::new(config, InMemoryRegistry::default())
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
