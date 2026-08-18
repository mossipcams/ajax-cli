//! Persist the operator's desired session model on a provisioned ACP task.

use super::{CommandContext, CommandError};
use crate::{adapters::acp_launch_for_agent, models::TaskId, registry::Registry};

/// Normalize operator model input before persisting on task metadata.
///
/// Auto and empty mean unspecified ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
pub fn normalize_persisted_session_model(model: Option<&str>) -> Option<&str> {
    match model.map(str::trim).filter(|model| !model.is_empty()) {
        Some("auto") => None,
        other => other,
    }
}

/// Write `session_model` on `handle` before the host replaces its ACP child.
pub fn set_task_session_model<R: Registry>(
    context: &mut CommandContext<R>,
    handle: &str,
    model: Option<&str>,
) -> Result<(), CommandError> {
    let task_id = TaskId::new(handle.to_string());
    let Some(task) = context.registry.get_task_mut(&task_id) else {
        return Err(CommandError::TaskNotFound(handle.to_string()));
    };
    if !task.skip_interactive_agent() {
        return Err(CommandError::PlanBlocked(vec![
            "session model change needs a task Ajax started over ACP".to_string(),
        ]));
    }
    if acp_launch_for_agent(task.selected_agent).is_none() {
        return Err(CommandError::PlanBlocked(vec![
            "agent has no ACP entry point".to_string(),
        ]));
    }

    task.set_session_model(normalize_persisted_session_model(model));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{Config, ManagedRepo},
        models::{AgentClient, LifecycleStatus, Task},
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
            AgentClient::Codex,
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
    fn set_session_model_persists_on_a_provisioned_task() {
        let mut context = context_with_task(true);
        set_task_session_model(&mut context, "web/fix-login", Some("gpt-5.6-sol[high]")).unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.session_model(), Some("gpt-5.6-sol[high]"));
    }

    #[test]
    fn set_session_model_refuses_an_interactive_task() {
        let mut context = context_with_task(false);
        let error =
            set_task_session_model(&mut context, "web/fix-login", Some("gpt-5.6-sol[high]"))
                .unwrap_err();
        assert!(matches!(error, CommandError::PlanBlocked(_)));
    }

    #[test]
    fn set_session_model_clears_auto_to_unspecified() {
        let mut context = context_with_task(true);
        set_task_session_model(&mut context, "web/fix-login", Some("gpt-5.6-sol[high]")).unwrap();
        set_task_session_model(&mut context, "web/fix-login", None).unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.session_model(), None);
    }

    // Regression for #952: never persist the literal `auto` sentinel.
    #[test]
    fn set_session_model_persists_none_for_auto_string() {
        let mut context = context_with_task(true);
        set_task_session_model(&mut context, "web/fix-login", Some("auto")).unwrap();

        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.session_model(), None);
    }
}
