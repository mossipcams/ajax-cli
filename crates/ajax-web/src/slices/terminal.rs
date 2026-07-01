//! Browser task terminal attach planning.

use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, registry::Registry, slices::pane::PaneError,
};

use crate::adapters::tmux_input::TmuxInputAdapter;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalAttachPlan {
    pub qualified_handle: String,
    pub tmux_session: String,
    pub worktrunk_window: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalRouteError {
    TaskNotFound,
    SessionMissing,
}

pub fn prepare_task_terminal<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<TerminalAttachPlan, TerminalRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(TerminalRouteError::TaskNotFound)?;

    if task.tmux_session.trim().is_empty() {
        return Err(TerminalRouteError::SessionMissing);
    }

    Ok(TerminalAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        tmux_session: task.tmux_session.clone(),
        worktrunk_window: task.worktrunk_window.clone(),
    })
}

/// Why a browser send-keys request could not be applied. Maps directly onto
/// HTTP status in the route handler.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SendKeysRouteError {
    TaskNotFound,
    SessionMissing,
    InvalidKeys(String),
    Command(String),
}

/// Send free text (the mobile composer) or an allowed control token to a task's
/// tmux pane without opening the raw terminal socket. Reuses the same session
/// resolution as the attach path and the validated `send_keys` slice.
pub fn send_task_keys<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    qualified_handle: &str,
    keys: &str,
    submit: bool,
) -> Result<(), SendKeysRouteError> {
    let plan = prepare_task_terminal(context, qualified_handle).map_err(|error| match error {
        TerminalRouteError::TaskNotFound => SendKeysRouteError::TaskNotFound,
        TerminalRouteError::SessionMissing => SendKeysRouteError::SessionMissing,
    })?;

    TmuxInputAdapter
        .send_keys(runner, &plan.tmux_session, keys, submit)
        .map_err(|error| match error {
            PaneError::InvalidKeys(message) => SendKeysRouteError::InvalidKeys(message),
            PaneError::SessionMissing => SendKeysRouteError::SessionMissing,
            PaneError::CommandRun(run_error) => SendKeysRouteError::Command(run_error.to_string()),
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::adapters::RecordingCommandRunner;
    use ajax_core::{
        config::{Config, ManagedRepo},
        models::{AgentClient, Task, TaskId},
        registry::InMemoryRegistry,
    };

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
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
            ))
            .unwrap();
        CommandContext::new(config, registry)
    }

    fn context_with_empty_session_task() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("web/fix-login"),
                "web",
                "fix-login",
                "Fix login",
                "ajax/fix-login",
                "main",
                "/repo/web__worktrees/ajax-fix-login",
                "",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        CommandContext::new(config, registry)
    }

    #[test]
    fn prepare_task_terminal_returns_registered_session_and_worktrunk_target() {
        let context = context_with_task();

        let plan = prepare_task_terminal(&context, "web/fix-login").expect("plan");

        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert_eq!(plan.tmux_session, "ajax-web-fix-login");
        assert_eq!(plan.worktrunk_window, "worktrunk");
    }

    #[test]
    fn prepare_task_terminal_returns_task_not_found_for_unknown_handle() {
        let context = context_with_task();

        let error = prepare_task_terminal(&context, "web/missing").unwrap_err();

        assert_eq!(error, TerminalRouteError::TaskNotFound);
    }

    #[test]
    fn prepare_task_terminal_returns_session_missing_for_empty_tmux_session() {
        let context = context_with_empty_session_task();

        let error = prepare_task_terminal(&context, "web/fix-login").unwrap_err();

        assert_eq!(error, TerminalRouteError::SessionMissing);
    }

    #[test]
    fn send_task_keys_sends_literal_text_and_enter() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        send_task_keys(&context, &mut runner, "web/fix-login", "approve it", true).unwrap();

        assert_eq!(
            runner.commands()[0].args,
            vec![
                "send-keys",
                "-t",
                "ajax-web-fix-login:worktrunk",
                "approve it",
                "Enter",
            ]
        );
    }

    #[test]
    fn send_task_keys_omits_enter_when_not_submitting() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        send_task_keys(&context, &mut runner, "web/fix-login", "C-c", false).unwrap();

        assert_eq!(
            runner.commands()[0].args,
            vec!["send-keys", "-t", "ajax-web-fix-login:worktrunk", "C-c"]
        );
    }

    #[test]
    fn send_task_keys_rejects_unknown_task() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        let error = send_task_keys(&context, &mut runner, "web/missing", "hi", true).unwrap_err();

        assert_eq!(error, SendKeysRouteError::TaskNotFound);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn send_task_keys_rejects_missing_session() {
        let context = context_with_empty_session_task();
        let mut runner = RecordingCommandRunner::default();

        let error = send_task_keys(&context, &mut runner, "web/fix-login", "hi", true).unwrap_err();

        assert_eq!(error, SendKeysRouteError::SessionMissing);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn send_task_keys_rejects_disallowed_key_token() {
        let context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        let error =
            send_task_keys(&context, &mut runner, "web/fix-login", "C-x", false).unwrap_err();

        assert!(matches!(error, SendKeysRouteError::InvalidKeys(_)));
    }
}
