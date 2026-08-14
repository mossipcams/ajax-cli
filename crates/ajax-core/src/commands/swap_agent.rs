//! Move an existing task to a different harness.
//!
//! Only provisioned (ACP-backed) tasks may swap: an interactive task has a live
//! agent in its tmux pane, and rewriting `selected_agent` under it would make the
//! registry disagree with the process that is actually running.

use super::{CommandContext, CommandError};
use crate::{
    adapters::acp_launch_for_agent,
    models::{AgentClient, TaskId},
    registry::Registry,
};

/// Point `handle` at `agent`, optionally pinning the model it should run.
///
/// The next session attach spawns the new harness; callers must drop any live
/// ACP slot for this task so it is not served by the previous harness.
pub fn swap_task_agent<R: Registry>(
    context: &mut CommandContext<R>,
    handle: &str,
    agent: AgentClient,
    model: Option<&str>,
) -> Result<(), CommandError> {
    if acp_launch_for_agent(agent).is_none() {
        return Err(CommandError::PlanBlocked(vec![
            "agent has no ACP entry point".to_string(),
        ]));
    }

    let task_id = TaskId::new(handle.to_string());
    let Some(task) = context.registry.get_task_mut(&task_id) else {
        return Err(CommandError::TaskNotFound(handle.to_string()));
    };
    if !task.skip_interactive_agent() {
        return Err(CommandError::PlanBlocked(vec![
            "swapping harness needs a task Ajax started over ACP".to_string(),
        ]));
    }

    task.selected_agent = agent;
    task.set_session_model(model);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, ManagedRepo},
        models::{LifecycleStatus, Task},
        registry::InMemoryRegistry,
    };
    use std::path::PathBuf;

    fn context_with_task(provisioned: bool) -> CommandContext<InMemoryRegistry> {
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            PathBuf::from("/repo/web__worktrees/ajax-fix-login"),
            "ajax-web-fix-login",
            "task",
            AgentClient::Cursor,
        );
        task.lifecycle_status = LifecycleStatus::Active;
        task.set_skip_interactive_agent(provisioned);
        let mut registry = InMemoryRegistry::default();
        registry.create_task(task).expect("create task");
        CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            registry,
        )
    }

    #[test]
    fn swap_points_a_provisioned_task_at_the_new_harness_and_model() {
        let mut context = context_with_task(true);
        swap_task_agent(
            &mut context,
            "web/fix-login",
            AgentClient::Codex,
            Some("gpt-5.6-sol[high]"),
        )
        .unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.selected_agent, AgentClient::Codex);
        assert_eq!(task.session_model(), Some("gpt-5.6-sol[high]"));
    }

    #[test]
    fn swap_refuses_an_interactive_task() {
        let mut context = context_with_task(false);
        let error =
            swap_task_agent(&mut context, "web/fix-login", AgentClient::Codex, None).unwrap_err();
        assert!(matches!(error, CommandError::PlanBlocked(_)));
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .selected_agent,
            AgentClient::Cursor
        );
    }

    #[test]
    fn swap_refuses_an_agent_without_acp() {
        let mut context = context_with_task(true);
        let error =
            swap_task_agent(&mut context, "web/fix-login", AgentClient::Other, None).unwrap_err();
        assert!(matches!(error, CommandError::PlanBlocked(_)));
    }

    #[test]
    fn swap_reports_a_missing_task() {
        let mut context = context_with_task(true);
        let error =
            swap_task_agent(&mut context, "web/missing", AgentClient::Codex, None).unwrap_err();
        assert!(matches!(error, CommandError::TaskNotFound(_)));
    }
}
