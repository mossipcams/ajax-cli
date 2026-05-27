//! Browser-submitted operator actions.

use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner},
    commands::{self, CommandContext, CommandError, NewTaskRequest, OpenMode},
    models::{LifecycleStatus, OperatorAction, SideFlag},
    registry::Registry,
    remediation::{self, RemediationError},
    task_operations::{
        drop_task::{
            execute_drop_task_operation, plan_drop_confirmation, plan_drop_task_operation,
            DropTaskCompletion,
        },
        start::{execute_start_task_operation, plan_start_task_operation},
        task_command::{
            execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
        },
    },
};

use crate::{action_vocabulary::SYNC_ACTION, adapters::skills::resolve_skill_path};

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
    if request.action == SYNC_ACTION {
        return sync_task(context, runner, &request.task_handle);
    }

    if remediation::is_remediation_action(&request.action) {
        return run_remediation(context, runner, &request.task_handle, &request.action);
    }

    let Some(action) = OperatorAction::from_label(&request.action) else {
        return Err(OperateError::UnknownAction(request.action));
    };

    match action {
        OperatorAction::Drop => execute_drop(context, runner, &request.task_handle, true),
        OperatorAction::Resume => Err(OperateError::UnsupportedCapability(
            "resume requires native cockpit; use sync instead",
        )),
        OperatorAction::Start => Err(OperateError::UnsupportedCapability(
            "start uses the dedicated Web Cockpit new-task operation",
        )),
        OperatorAction::Review | OperatorAction::Ship | OperatorAction::Repair => {
            let kind = task_command_kind(action)?;
            execute_task_command(context, runner, kind, &request.task_handle)
        }
    }
}

pub fn sync_task<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    task_handle: &str,
) -> Result<OperateOutcome, OperateError> {
    execute_task_command(context, runner, TaskCommandKind::Resume, task_handle)
}

pub fn start_task<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
    request: StartTaskRequest,
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
    let (_intent, plan) = plan_start_task_operation(context, core_request.clone())
        .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    execute_start_task_operation(
        context,
        runner,
        &core_request,
        &plan,
        confirmed,
        OpenMode::NoAttach,
    )
    .map_err(|error| OperateError::Command(error, true))?;

    Ok(OperateOutcome {
        state_changed: true,
        output: format!("started task: {}", core_request.title),
    })
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
    use super::{format_execution_outputs, operate, sync_task, OperateError, OperateRequest};
    use ajax_core::remediation;
    use ajax_core::{
        adapters::{CommandOutput, RecordingCommandRunner},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, LifecycleStatus, LiveObservation, LiveStatusKind, Task, TaskId, TmuxStatus,
        },
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
            OperateError::UnsupportedCapability("resume requires native cockpit; use sync instead")
        );
        assert!(runner.commands().is_empty());
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
    fn sync_task_runs_resume_without_attach() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();
        let outcome = sync_task(&mut context, &mut runner, "web/fix-login").unwrap();

        assert!(outcome.state_changed);
        assert!(!runner.commands().iter().any(|command| command
            .args
            .first()
            .is_some_and(|arg| arg == "attach-session")));
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
    fn format_execution_outputs_prefers_stdout() {
        let text = format_execution_outputs(&[CommandOutput {
            status_code: 0,
            stdout: " diff stat\n".to_string(),
            stderr: String::new(),
        }]);

        assert_eq!(text, "diff stat");
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

    fn context_with_managed_repo() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        CommandContext::new(config, InMemoryRegistry::default())
    }
}
