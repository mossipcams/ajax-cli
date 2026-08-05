use super::*;

#[test]
fn operation_kernel_refuses_blocked_plan_without_running_commands() {
    let mut blocked_plan = CommandPlan::new("blocked");
    blocked_plan.blocked_reasons = vec!["not ready".to_string()];
    let mut runner = RecordingQueuedRunner::default();

    assert_eq!(
        execute_external_plan(&blocked_plan, true, &mut runner),
        Err(CommandError::PlanBlocked(vec!["not ready".to_string()]))
    );
    assert!(runner.commands.is_empty());
}

#[test]
fn operation_kernel_requires_confirmation_before_running_risky_plan() {
    let mut confirmation_plan = CommandPlan::new("confirm");
    confirmation_plan.requires_confirmation = true;
    let mut runner = RecordingQueuedRunner::default();

    assert_eq!(
        execute_external_plan(&confirmation_plan, false, &mut runner),
        Err(CommandError::ConfirmationRequired)
    );
    assert!(runner.commands.is_empty());
}

#[test]
fn operation_kernel_surfaces_nonzero_exit_after_running_the_failing_command() {
    let mut failing_plan = CommandPlan::new("failing");
    failing_plan
        .commands
        .push(CommandSpec::new("git", ["status"]));
    let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
        status_code: 128,
        stdout: String::new(),
        stderr: "fatal".to_string(),
    }]);

    assert_eq!(
        execute_external_plan(&failing_plan, true, &mut runner),
        Err(CommandError::CommandRun(
            crate::adapters::CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 128,
                stderr: "fatal".to_string(),
                cwd: None,
            }
        ))
    );
    assert_eq!(runner.commands.len(), 1);
}

#[test]
fn operation_kernel_returns_outputs_for_successful_plan() {
    let mut success_plan = CommandPlan::new("success");
    success_plan
        .commands
        .push(CommandSpec::new("git", ["status"]));
    success_plan.commands.push(CommandSpec::new("tmux", ["ls"]));
    let mut runner = RecordingQueuedRunner::new(vec![
        CommandOutput {
            status_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
        },
        CommandOutput {
            status_code: 0,
            stdout: "session".to_string(),
            stderr: String::new(),
        },
    ]);

    assert_eq!(
        execute_external_plan(&success_plan, true, &mut runner).unwrap(),
        vec![
            CommandOutput {
                status_code: 0,
                stdout: "ok".to_string(),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: "session".to_string(),
                stderr: String::new(),
            },
        ]
    );
    assert_eq!(runner.commands.len(), 2);
}

#[test]
fn start_operation_plan_returns_task_intent_and_commands_without_mutating_registry() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
    };

    let (intent, plan) = plan_start_task_operation(&context, request).unwrap();

    assert_eq!(context.registry.list_tasks().len(), 0);
    assert_eq!(context.registry.list_events().len(), 0);
    assert_eq!(intent.id, TaskId::new("web/fix-login"));
    assert_eq!(intent.repo, "web");
    assert_eq!(intent.handle, "fix-login");
    assert_eq!(intent.title, "Fix login");
    assert_eq!(intent.branch, "ajax/fix-login");
    assert_eq!(intent.base_branch, "main");
    assert_eq!(
        intent.worktree_path,
        std::path::Path::new("/repo/web__worktrees/ajax-fix-login")
    );
    assert_eq!(intent.tmux_session, "ajax-web-fix-login");
    assert_eq!(intent.task_window, "task");
    assert_eq!(intent.selected_agent, AgentClient::Codex);
    assert_eq!(plan.title, "create task: Fix login");
    assert_eq!(plan.commands.len(), 4);
    assert!(crate::commands::is_git_worktree_add_command(
        &plan.commands[1]
    ));
    assert!(crate::commands::is_task_window_new_session_command(
        &plan.commands[2]
    ));
    assert!(crate::commands::is_agent_send_keys_command(
        &plan.commands[3]
    ));
}

#[test]
fn start_operation_execution_failure_preserves_intent_and_marks_provisioning_failed() {
    let mut context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
    };
    let (intent, plan) = plan_start_task_operation(&context, request.clone()).unwrap();
    let mut runner = FirstCommandFailsRunner::default();

    let error = execute_start_task_operation(
        &mut context,
        &mut runner,
        &request,
        &plan,
        true,
        OpenMode::Attach,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
            status_code: 1,
            ..
        })
    ));
    let task = context.registry.get_task(&intent.id).unwrap();
    assert_eq!(task.intent(), intent);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Error);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.metadata.get("start_failed_step").map(String::as_str),
        Some("worktree_created")
    );
    assert_eq!(
        task.metadata
            .get("operator_recommendation")
            .map(String::as_str),
        Some("retry ajax start after checking the failed provisioning step")
    );
    assert_eq!(runner.commands.len(), 1);
}

#[test]
fn start_operation_records_receipts_for_successful_provisioning_steps() {
    let mut context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
    };
    let (intent, plan) = plan_start_task_operation(&context, request.clone()).unwrap();
    let mut runner = RecordingQueuedRunner::default();

    execute_start_task_operation(
        &mut context,
        &mut runner,
        &request,
        &plan,
        true,
        OpenMode::Attach,
    )
    .unwrap();

    let receipts = context.registry.step_receipts_for_task(&intent.id);
    let keys = receipts
        .iter()
        .map(|receipt| {
            (
                receipt.operation,
                receipt.step_key.as_str(),
                receipt.target.as_str(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        keys,
        vec![
            (
                TaskOperationKind::Start,
                "worktree_created",
                "/repo/web__worktrees/ajax-fix-login",
            ),
            (
                TaskOperationKind::Start,
                "task_session_created",
                "ajax-web-fix-login",
            ),
            (
                TaskOperationKind::Start,
                "agent_command_sent",
                "ajax-web-fix-login:task",
            ),
        ]
    );
}

#[test]
fn task_command_operation_plans_use_operator_titles() {
    let context = context_with_reviewable_task();

    let cases = [
        (TaskCommandKind::Resume, "open task: web/fix-login"),
        (TaskCommandKind::Review, "diff task: web/fix-login"),
        (TaskCommandKind::Repair, "repair task: web/fix-login"),
        (TaskCommandKind::Ship, "merge task: web/fix-login"),
    ];

    for (kind, title) in cases {
        let plan =
            plan_task_command_operation(&context, kind, "web/fix-login", OpenMode::Attach).unwrap();

        assert_eq!(plan.title, title);
        assert!(
            !plan.commands.is_empty(),
            "{kind:?} should carry executable commands"
        );
    }
}

#[test]
fn resume_operation_executes_plan_and_reports_state_change() {
    let mut context = context_with_reviewable_task();
    let resume_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Resume,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut resume_runner = RecordingQueuedRunner::new(vec![
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
    ]);

    let (resume_outputs, resume_state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Resume,
        "web/fix-login",
        &resume_plan,
        true,
        &mut resume_runner,
    )
    .unwrap();

    assert_eq!(resume_runner.commands.len(), 2);
    assert_eq!(resume_outputs.len(), 2);
    assert!(resume_state_changed);
}

#[test]
fn review_operation_returns_diff_output_without_state_change() {
    let mut context = context_with_reviewable_task();
    let review_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Review,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut review_runner = RecordingQueuedRunner::new(vec![CommandOutput {
        status_code: 0,
        stdout: "diff stat".to_string(),
        stderr: String::new(),
    }]);

    let (review_outputs, review_state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Review,
        "web/fix-login",
        &review_plan,
        true,
        &mut review_runner,
    )
    .unwrap();

    assert_eq!(review_runner.commands.len(), 1);
    assert_eq!(review_outputs[0].stdout, "diff stat");
    assert!(!review_state_changed);
}

fn claude_waiting_context() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    task.lifecycle_status = LifecycleStatus::Active;
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.add_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    context
}

#[test]
fn successful_resume_records_attention_acknowledgment() {
    let mut context = claude_waiting_context();
    let plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Resume,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(
        plan.commands
            .iter()
            .map(|_| CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
            .collect(),
    );

    execute_task_command_operation(
        &mut context,
        TaskCommandKind::Resume,
        "web/fix-login",
        &plan,
        true,
        &mut runner,
    )
    .unwrap();

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(task.attention_acknowledged_at.is_some());
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
}

#[test]
fn failed_resume_does_not_acknowledge_attention() {
    let mut context = claude_waiting_context();
    let plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Resume,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
        status_code: 1,
        stdout: String::new(),
        stderr: "resume failed".to_string(),
    }]);

    let (_error, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Resume,
        "web/fix-login",
        &plan,
        true,
        &mut runner,
    )
    .unwrap_err();

    assert!(!state_changed);
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task.attention_acknowledged_at, None);
    assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}

#[test]
fn review_operation_does_not_acknowledge_attention() {
    let mut context = claude_waiting_context();
    let plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Review,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(
        plan.commands
            .iter()
            .map(|_| CommandOutput {
                status_code: 0,
                stdout: "diff stat".to_string(),
                stderr: String::new(),
            })
            .collect(),
    );

    execute_task_command_operation(
        &mut context,
        TaskCommandKind::Review,
        "web/fix-login",
        &plan,
        true,
        &mut runner,
    )
    .unwrap();

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task.attention_acknowledged_at, None);
    assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
}

#[test]
fn ship_task_operation_refreshes_git_evidence_before_merge_commands() {
    let mut context = context_with_reviewable_task();
    let ship_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Ship,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
        status_code: 0,
        stdout: "## ajax/fix-login\n M src/lib.rs\n".to_string(),
        stderr: String::new(),
    }]);

    let (error, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Ship,
        "web/fix-login",
        &ship_plan,
        true,
        &mut runner,
    )
    .unwrap_err();

    assert!(!state_changed);
    assert!(matches!(error, CommandError::PlanBlocked(_)));
    assert_eq!(runner.commands.len(), 1);
    assert_eq!(runner.commands[0].program, "git");
    assert!(runner.commands[0].args.contains(&"status".to_string()));
}

#[test]
fn ship_operation_marks_task_merged_on_success() {
    let mut context = context_with_reviewable_task();
    let ship_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Ship,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(vec![
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
    ]);

    let (outputs, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Ship,
        "web/fix-login",
        &ship_plan,
        true,
        &mut runner,
    )
    .unwrap();

    assert_eq!(outputs.len(), 2);
    assert!(state_changed);
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );
}

#[test]
fn ship_operation_records_conflict_attention_on_merge_failure() {
    let mut context = context_with_reviewable_task();
    let ship_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Ship,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(vec![
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: "Automatic merge failed; fix conflicts and then commit.".to_string(),
        },
    ]);

    let (error, _state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Ship,
        "web/fix-login",
        &ship_plan,
        true,
        &mut runner,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
            status_code: 1,
            ..
        })
    ));
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(task.has_side_flag(SideFlag::Conflicted));
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::MergeConflict, "merge failed"))
    );
}

fn context_with_detached_checkout_mismatch() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: None,
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.refresh_runtime_projection();
    context
}

#[test]
fn checkout_mismatch_repair_plans_confirmed_branch_adoption_or_blocks_detached() {
    let named_context = context_with_named_checkout_mismatch();
    let named_plan = plan_task_command_operation(
        &named_context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();

    assert_eq!(named_plan.title, "repair task: web/fix-login");
    assert!(named_plan.commands.is_empty());
    assert!(named_plan.blocked_reasons.is_empty());
    assert!(named_plan.requires_confirmation);
    let adoption = named_plan.branch_adoption.as_ref().unwrap();
    assert_eq!(adoption.expected_branch, "ajax/fix-login");
    assert_eq!(adoption.observed_branch, "fix/pane-stuck");

    let detached_context = context_with_detached_checkout_mismatch();
    let detached_plan = plan_task_command_operation(
        &detached_context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();

    assert!(detached_plan.commands.is_empty());
    assert!(detached_plan.branch_adoption.is_none());
    assert_eq!(
        detached_plan.blocked_reasons,
        vec!["cannot adopt a detached worktree; switch to a branch and refresh"]
    );
}
