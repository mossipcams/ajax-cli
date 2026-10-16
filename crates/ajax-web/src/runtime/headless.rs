//! Startup dispatch of queued prompts left by a previous host without any browser
//! attached: after the execution lease is claimed, resumable chats are dispatched
//! strictly one at a time.

use std::sync::Arc;
use std::time::Duration;

use ajax_core::{
    adapters::acp_launch_for_agent,
    commands::CommandContext,
    registry::{InMemoryRegistry, Registry as _},
};

use super::state::WebAppState;
use crate::slices::web_session::{run_headless_dispatch, HeadlessTarget, HEARTBEAT_EVERY};

const CLAIM_RETRIES: usize = 6; // a live foreign host may still hold the lease right after a restart
const MAX_WAIT_PER_CHAT: Duration = Duration::from_secs(30 * 60);

/// Pure: the chats among `handles` that can be resumed headlessly, in `handles` order.
pub(crate) fn targets_for(
    context: &CommandContext<InMemoryRegistry>,
    handles: &[String],
) -> Vec<HeadlessTarget> {
    let mut targets = Vec::new();
    for handle in handles {
        match context.registry.list_tasks().into_iter().find(|task| {
            task.qualified_handle() == *handle
                && task.skip_interactive_agent()
                && acp_launch_for_agent(task.selected_agent).is_some()
        }) {
            Some(task) => targets.push(HeadlessTarget {
                handle: handle.clone(),
                worktree_path: task.worktree_path.clone(),
                model: task.session_model().unwrap_or("auto").to_string(),
                agent: task.selected_agent,
            }),
            None => tracing::debug!(
                target: "ajax_web",
                handle = %handle,
                "headless dispatch: no resumable task for handle"
            ),
        }
    }
    targets
}

pub(crate) fn spawn_startup_dispatch<C, B>(state: &WebAppState<C, B>)
where
    C: Send + 'static,
    B: Send + 'static,
{
    let directory = Arc::clone(&state.task_session_directory);
    let shared = Arc::clone(&state.shared);
    tokio::spawn(async move {
        // Claim the startup dispatch list; a live foreign host may still hold the
        // lease right after a restart, so retry a few times.
        let mut claimed: Option<Vec<String>> = None;
        for attempt in 1..=CLAIM_RETRIES {
            match directory.startup_dispatch_handles() {
                Ok(handles) => {
                    claimed = Some(handles);
                    break;
                }
                Err(error) => {
                    if attempt == CLAIM_RETRIES {
                        tracing::warn!(
                            target: "ajax_web",
                            attempt,
                            %error,
                            "headless dispatch: giving up on startup claim"
                        );
                        return;
                    }
                    tracing::debug!(
                        target: "ajax_web",
                        attempt,
                        %error,
                        "headless dispatch: startup claim failed, retrying"
                    );
                    tokio::time::sleep(HEARTBEAT_EVERY).await;
                }
            }
        }

        let handles = match claimed {
            Some(handles) if !handles.is_empty() => handles,
            _ => return,
        };

        // Build targets inside a short std-mutex scope; the guard must not live
        // across an await.
        let targets = {
            let guard = shared.lock().expect("web shared state lock poisoned");
            targets_for(&guard.context, &handles)
        };

        let results = run_headless_dispatch(&directory, targets, MAX_WAIT_PER_CHAT).await;
        let failed = results.iter().filter(|(_, result)| result.is_err()).count();
        tracing::info!(
            target: "ajax_web",
            ok = results.len() - failed,
            failed,
            "headless dispatch finished"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::models::AgentClient;

    fn acp_task_context() -> (String, CommandContext<InMemoryRegistry>) {
        let mut task = crate::test_support::fix_login_task();
        task.set_skip_interactive_agent(true);
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        (handle, context)
    }

    #[test]
    fn targets_for_returns_target_for_acp_task() {
        let (handle, context) = acp_task_context();
        let targets = targets_for(&context, std::slice::from_ref(&handle));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].handle, handle);
        assert_eq!(
            targets[0].worktree_path.to_string_lossy().into_owned(),
            "/repo/web__worktrees/ajax-fix-login"
        );
        assert_eq!(targets[0].model, "auto");
        assert_eq!(targets[0].agent, AgentClient::Codex);
    }

    #[test]
    fn targets_for_skips_unknown_handle() {
        let (_handle, context) = acp_task_context();
        let targets = targets_for(&context, &["web/nope".to_string()]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_skips_non_acp_task() {
        // Rebuild with a non-ACP-capable agent (AgentClient::Other).
        let mut task = ajax_core::models::Task::new(
            ajax_core::models::TaskId::new("web/other-agent"),
            "web",
            "other-agent",
            "Other agent",
            "ajax/other-agent".to_string(),
            "main",
            "/repo/web__worktrees/ajax-other-agent".to_string(),
            "ajax-web-other-agent".to_string(),
            "task",
            AgentClient::Other,
        );
        task.set_skip_interactive_agent(true);
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let targets = targets_for(&context, &[handle]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_skips_interactive_task() {
        // A task that is not flagged skip_interactive_agent is not resumable.
        let task = crate::test_support::fix_login_task();
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let targets = targets_for(&context, &[handle]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_preserves_input_order() {
        let (a_handle, mut context) = acp_task_context();
        let mut b_task = crate::test_support::task_in("web", "fix-payment", "Fix payment");
        b_task.set_skip_interactive_agent(true);
        let b_handle = b_task.qualified_handle();
        context.registry.create_task(b_task).unwrap();

        let targets = targets_for(&context, &[b_handle.clone(), a_handle.clone()]);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].handle, b_handle);
        assert_eq!(targets[1].handle, a_handle);
    }
}
