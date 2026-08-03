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
