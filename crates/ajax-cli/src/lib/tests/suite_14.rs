#[test]
fn confirmed_agent_stop_records_dead_instead_of_unknown() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);
    task.agent_attempts.push(ajax_core::models::AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "tmux:%1".to_string(),
        started_at: SystemTime::UNIX_EPOCH,
        finished_at: None,
        status: AgentRuntimeStatus::Running,
    });
    ajax_core::commands::mark_drop_agent_stopped(&mut context, "web/fix-login").unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::Dead);
    assert_eq!(task.agent_attempts[0].status, AgentRuntimeStatus::Dead);
    assert!(!task.has_side_flag(SideFlag::AgentRunning));
}

#[test]
fn native_ci_delivery_accepts_fresh_wrapper_and_rejects_unsafe_foreground() {
    use crate::agent_runtime::{AgentRuntimeSnapshot, AgentRuntimeState};
    use crate::ci_agent_delivery::deliver;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandSpec},
        agent_notification::{AgentNotification, AgentNotificationDeliveryStatus, CiFailedCheck},
        models::{AgentClient, Task, TaskId},
    };
    use std::{
        collections::VecDeque,
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Runner {
        outputs: VecDeque<&'static str>,
        commands: Vec<CommandSpec>,
    }

    impl ajax_core::adapters::CommandRunner for Runner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            Ok(CommandOutput {
                status_code: 0,
                stdout: self.outputs.pop_front().unwrap_or("").to_string(),
                stderr: String::new(),
            })
        }
    }

    let dir = std::env::temp_dir().join(format!("ajax-ci-native-{}", std::process::id()));
    fs::create_dir_all(dir.join("agent-runtime")).unwrap();
    let task = Task::new(
        TaskId::new("task-1"),
        "ajax",
        "ci",
        "CI",
        "ajax/ci",
        "main",
        &dir,
        "ajax-ci",
        "task",
        AgentClient::Codex,
    );
    let notification = AgentNotification::CiFailed {
        episode_id: "episode".to_string(),
        task_id: task.id.clone(),
        pr_number: 42,
        head_sha: "abc".to_string(),
        failed_checks: vec![CiFailedCheck {
            name: "CI".to_string(),
            link: None,
            identity: None,
        }],
    };
    let snapshot = AgentRuntimeSnapshot {
        task_id: task.id.as_str().to_string(),
        state: AgentRuntimeState::Running,
        observed_at_unix_millis: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
        pid: Some(123),
        exit_code: None,
        message: None,
    };
    fs::write(
        dir.join("agent-runtime/task-1.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();

    let mut ok_runner = Runner {
        outputs: VecDeque::from(["/usr/bin/codex\n", "codex\n", ""]),
        commands: vec![],
    };
    assert_eq!(
        deliver(&dir, &mut ok_runner, &task, &notification).unwrap(),
        AgentNotificationDeliveryStatus::Accepted
    );
    assert!(ok_runner.commands.last().is_some_and(|command| {
        command.args.first().is_some_and(|arg| arg == "send-keys")
    }));
    let _ = fs::remove_dir_all(dir);
}
