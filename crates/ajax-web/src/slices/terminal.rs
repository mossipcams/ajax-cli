//! Browser task terminal attach planning.

use ajax_core::{commands::CommandContext, registry::Registry};

pub use crate::adapters::terminal_pty::TerminalAttachPlan;

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
        task_window: task.task_window.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use ajax_core::registry::InMemoryRegistry;

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        test_support::context_with_fix_login_task()
    }

    fn context_with_empty_session_task() -> CommandContext<InMemoryRegistry> {
        let mut task = test_support::fix_login_task();
        task.tmux_session = String::new();
        test_support::context_with_tasks(&["web"], vec![task])
    }

    #[test]
    fn prepare_task_terminal_returns_registered_session_and_task_target() {
        let context = context_with_task();

        let plan = prepare_task_terminal(&context, "web/fix-login").expect("plan");

        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert_eq!(plan.tmux_session, "ajax-web-fix-login");
        assert_eq!(plan.task_window, "task");
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
