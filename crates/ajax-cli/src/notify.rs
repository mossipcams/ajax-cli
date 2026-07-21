use ajax_core::adapters::{CommandRunner, CommandSpec};
use ajax_core::attention::{take_attention_transition, AttentionTransition};
use ajax_core::commands::CommandContext;
use ajax_core::registry::{InMemoryRegistry, Registry};
use std::time::Duration;

pub(crate) fn webhook_command(webhook_url: &str, transition: &AttentionTransition) -> CommandSpec {
    let mut body = format!(
        "{}/{}: {} ({})",
        transition.repo,
        transition.handle,
        transition.status.as_str(),
        transition.client
    );
    if let Some(explanation) = transition.explanation.as_deref() {
        body.push_str(" — ");
        body.push_str(explanation);
    }
    let mut spec = CommandSpec::new("curl", ["-s", "--max-time", "10", "-d"]);
    spec.args.push(body);
    spec.args.push(webhook_url.to_string());
    spec.with_timeout(Duration::from_secs(10))
}

/// Fire webhook notifications for tasks that just crossed into Waiting or
/// Error. Returns true when any metadata stamp changed so callers persist it.
/// Delivery failures are ignored: notifications must never break a refresh.
pub(crate) fn notify_attention_transitions(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> bool {
    let Some(webhook_url) = context
        .config
        .notify
        .as_ref()
        .map(|notify| notify.webhook_url.clone())
    else {
        return false;
    };

    let task_ids: Vec<_> = context
        .registry
        .list_tasks()
        .iter()
        .map(|task| task.id.clone())
        .collect();
    let mut fired = false;
    for task_id in task_ids {
        let Some(task) = context.registry.get_task_mut(&task_id) else {
            continue;
        };
        if let Some(transition) = take_attention_transition(task) {
            fired = true;
            let _ = runner.run(&webhook_command(&webhook_url, &transition));
        }
    }
    fired
}

#[cfg(test)]
mod tests {
    use super::{notify_attention_transitions, webhook_command};
    use ajax_core::adapters::{
        CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
    };
    use ajax_core::attention::AttentionTransition;
    use ajax_core::commands::CommandContext;
    use ajax_core::config::{Config, NotifyConfig};
    use ajax_core::lifecycle::mark_active;
    use ajax_core::models::{AgentClient, SideFlag, Task, TaskId};
    use ajax_core::registry::{InMemoryRegistry, Registry};
    use ajax_core::ui_state::TaskStatus;
    use std::time::Duration;

    #[test]
    fn webhook_spec_shape() {
        let transition = AttentionTransition {
            repo: "web".to_string(),
            handle: "fix-login".to_string(),
            status: TaskStatus::Waiting,
            explanation: Some("Waiting for input".to_string()),
            client: "codex".to_string(),
        };

        let spec = webhook_command("https://ntfy.sh/topic", &transition);

        assert_eq!(spec.program, "curl");
        assert_eq!(
            spec.args,
            [
                "-s",
                "--max-time",
                "10",
                "-d",
                "web/fix-login: Waiting (codex) — Waiting for input",
                "https://ntfy.sh/topic",
            ]
            .map(String::from)
        );
        assert_eq!(spec.mode, CommandMode::Capture);
        assert_eq!(spec.timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn webhook_spec_without_explanation() {
        let transition = AttentionTransition {
            repo: "web".to_string(),
            handle: "fix-login".to_string(),
            status: TaskStatus::Error,
            explanation: None,
            client: "codex".to_string(),
        };

        let spec = webhook_command("https://ntfy.sh/topic", &transition);

        assert_eq!(spec.args[4], "web/fix-login: Error (codex)");
    }

    struct RecordingRunner {
        specs: Vec<CommandSpec>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.specs.push(command.clone());
            Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn waiting_task() -> Task {
        let mut task = Task::new(
            TaskId::new("task-notify"),
            "web",
            "notify",
            "Notify",
            "ajax/notify",
            "main",
            "/tmp/worktrees/web-notify",
            "ajax-web-notify",
            "task",
            AgentClient::Codex,
        );
        mark_active(&mut task).unwrap();
        task.add_side_flag(SideFlag::NeedsInput);
        task
    }

    fn context_with_waiting_task(config: Config) -> CommandContext<InMemoryRegistry> {
        let mut registry = InMemoryRegistry::default();
        registry.create_task(waiting_task()).unwrap();
        CommandContext::new(config, registry)
    }

    #[test]
    fn notifies_once_and_reports_state_change() {
        let config = Config {
            notify: Some(NotifyConfig {
                webhook_url: "https://ntfy.sh/topic".to_string(),
                poll_seconds: None,
            }),
            ..Config::default()
        };
        let mut context = context_with_waiting_task(config);
        let mut runner = RecordingRunner { specs: Vec::new() };

        assert!(notify_attention_transitions(&mut context, &mut runner));
        assert_eq!(runner.specs.len(), 1);
        assert_eq!(runner.specs[0].program, "curl");

        // Same state again: no new delivery, no state change.
        assert!(!notify_attention_transitions(&mut context, &mut runner));
        assert_eq!(runner.specs.len(), 1);
    }

    #[test]
    fn missing_notify_config_is_silent() {
        let mut context = context_with_waiting_task(Config::default());
        let mut runner = RecordingRunner { specs: Vec::new() };

        assert!(!notify_attention_transitions(&mut context, &mut runner));
        assert!(runner.specs.is_empty());
    }
}
