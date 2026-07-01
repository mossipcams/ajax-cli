//! Browser task terminal attach planning.

use ajax_core::{commands::CommandContext, registry::Registry};

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

#[cfg(test)]
mod tests {
    use super::*;
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
}
