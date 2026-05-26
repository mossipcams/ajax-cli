//! Browser-submitted operator actions.

use ajax_core::{
    adapters::CommandRunner,
    commands::{CommandContext, CommandError, NewTaskRequest, OpenMode},
    models::OperatorAction,
    registry::Registry,
    task_operations::{
        start::{execute_start_task_operation, plan_start_task_operation},
        task_command::{
            execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
        },
    },
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebActionState {
    pub action: String,
    pub status: String,
    pub reason: Option<&'static str>,
    pub destructive: bool,
    pub confirmation_required: bool,
}

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
    let Some(action) = OperatorAction::from_label(&request.action) else {
        return Err(OperateError::UnknownAction(request.action));
    };

    let kind = task_command_kind(action)?;
    let plan = plan_task_command_operation(context, kind, &request.task_handle, OpenMode::Attach)
        .map_err(|error| OperateError::Command(error, false))?;
    let confirmed = !plan.requires_confirmation;
    let (_outputs, state_changed) = execute_task_command_operation(
        context,
        kind,
        &request.task_handle,
        &plan,
        confirmed,
        runner,
    )
    .map_err(|(error, state_changed)| OperateError::Command(error, state_changed))?;

    Ok(OperateOutcome { state_changed })
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
    })
}

fn task_command_kind(action: OperatorAction) -> Result<TaskCommandKind, OperateError> {
    match action {
        OperatorAction::Review => Ok(TaskCommandKind::Review),
        OperatorAction::Ship => Ok(TaskCommandKind::Ship),
        OperatorAction::Repair => Err(OperateError::UnsupportedCapability(
            "repair requires native cockpit terminal attach",
        )),
        OperatorAction::Resume => Err(OperateError::UnsupportedCapability(
            "terminal attach requires native cockpit",
        )),
        OperatorAction::Start => Err(OperateError::UnsupportedCapability(
            "task title input requires native cockpit",
        )),
        OperatorAction::Drop => Err(OperateError::UnsupportedCapability(
            "drop confirmation is not enabled for mobile web",
        )),
    }
}

pub fn web_action_state(action: OperatorAction) -> WebActionState {
    let (status, reason) = match action {
        OperatorAction::Review
        | OperatorAction::Ship
        | OperatorAction::Repair
        | OperatorAction::Drop => ("supported", None),
        OperatorAction::Resume => (
            "needs_terminal",
            Some("terminal attach requires native cockpit"),
        ),
        OperatorAction::Start => (
            "unsupported",
            Some("start uses the dedicated Web Cockpit new-task operation"),
        ),
    };

    WebActionState {
        action: action.as_str().to_string(),
        status: status.to_string(),
        reason,
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
    }
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    web_action_state(action).status == "supported"
}

#[cfg(test)]
mod tests {
    use super::{operate, OperateError, OperateRequest};
    use ajax_core::{
        adapters::RecordingCommandRunner,
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{AgentClient, LifecycleStatus, Task, TaskId},
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
    fn operate_slice_rejects_terminal_attach() {
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
            OperateError::UnsupportedCapability("terminal attach requires native cockpit")
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn operate_slice_rejects_repair_terminal_attach() {
        let mut context = context_with_reviewable_task();
        let mut runner = RecordingCommandRunner::default();
        let error = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "repair".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            OperateError::UnsupportedCapability("repair requires native cockpit terminal attach")
        );
        assert!(runner.commands().is_empty());
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

    #[test]
    fn operate_slice_delegates_review_to_core_operation() {
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
        assert_eq!(runner.commands().len(), 1);
        assert_eq!(runner.commands()[0].program, "git");
        assert_eq!(
            runner.commands()[0].args,
            ["diff", "--stat", "main...ajax/fix-login"]
        );
    }
}
