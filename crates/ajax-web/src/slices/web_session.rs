//! Browser task orchestration-chat attach planning.

use ajax_core::{commands::CommandContext, models::AgentClient, registry::Registry};

use crate::ports::web_session::SessionAttachPlan;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionRouteError {
    TaskNotFound,
    NotCursor,
    WorktreeMissing,
}

pub fn prepare_task_session<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<SessionAttachPlan, SessionRouteError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
        .ok_or(SessionRouteError::TaskNotFound)?;

    if task.selected_agent != AgentClient::Cursor {
        return Err(SessionRouteError::NotCursor);
    }

    if !task.worktree_path.is_dir() {
        return Err(SessionRouteError::WorktreeMissing);
    }

    Ok(SessionAttachPlan {
        qualified_handle: qualified_handle.to_string(),
        worktree: task.worktree_path.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use ajax_core::{models::AgentClient, registry::InMemoryRegistry};
    use std::{fs, path::PathBuf};

    fn context_with_cursor_task(worktree: PathBuf) -> CommandContext<InMemoryRegistry> {
        let mut task = test_support::fix_login_task();
        task.selected_agent = AgentClient::Cursor;
        task.worktree_path = worktree;
        test_support::context_with_tasks(&["web"], vec![task])
    }

    #[test]
    fn prepare_task_session_returns_plan_for_cursor_task_with_worktree() {
        let worktree = std::env::temp_dir().join(format!(
            "ajax-web-session-plan-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&worktree).expect("worktree dir");
        let context = context_with_cursor_task(worktree.clone());

        let plan = prepare_task_session(&context, "web/fix-login").expect("plan");

        assert_eq!(plan.qualified_handle, "web/fix-login");
        assert_eq!(plan.worktree, worktree);
    }

    #[test]
    fn prepare_task_session_returns_task_not_found_for_unknown_handle() {
        let worktree =
            std::env::temp_dir().join(format!("ajax-web-session-missing-{}", std::process::id()));
        fs::create_dir_all(&worktree).expect("worktree dir");
        let context = context_with_cursor_task(worktree);

        let error = prepare_task_session(&context, "web/missing").unwrap_err();

        assert_eq!(error, SessionRouteError::TaskNotFound);
    }

    #[test]
    fn prepare_task_session_returns_not_cursor_for_codex_task() {
        let context = test_support::context_with_fix_login_task();

        let error = prepare_task_session(&context, "web/fix-login").unwrap_err();

        assert_eq!(error, SessionRouteError::NotCursor);
    }

    #[test]
    fn prepare_task_session_returns_worktree_missing_when_path_absent() {
        let worktree =
            std::env::temp_dir().join(format!("ajax-web-session-no-dir-{}", std::process::id()));
        let context = context_with_cursor_task(worktree);

        let error = prepare_task_session(&context, "web/fix-login").unwrap_err();

        assert_eq!(error, SessionRouteError::WorktreeMissing);
    }
}
