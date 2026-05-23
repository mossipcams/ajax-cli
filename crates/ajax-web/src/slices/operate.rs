//! Browser-submitted operator actions.

use ajax_core::{
    adapters::CommandRunner,
    commands::{CommandContext, CommandError, OpenMode},
    models::OperatorAction,
    registry::Registry,
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperateRequest {
    pub task_handle: String,
    pub action: String,
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
