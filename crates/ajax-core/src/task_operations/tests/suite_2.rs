use super::*;

#[test]
fn checkout_mismatch_repair_adopts_without_external_commands() {
    let mut context = context_with_named_checkout_mismatch();
    let task_before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();
    let events_before: Vec<_> = context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .iter()
        .map(|event| (event.kind, event.message.clone()))
        .collect();

    let repair_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::default();

    let (outputs, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Repair,
        "web/fix-login",
        &repair_plan,
        true,
        &mut runner,
    )
    .unwrap();

    assert!(outputs.is_empty());
    assert!(state_changed);
    assert!(runner.commands.is_empty());

    let task_after = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task_after.branch, "fix/pane-stuck");
    assert_eq!(task_after.id, task_before.id);
    assert_eq!(task_after.handle, task_before.handle);
    assert_eq!(task_after.title, task_before.title);
    assert_eq!(task_after.worktree_path, task_before.worktree_path);
    assert_eq!(task_after.tmux_session, task_before.tmux_session);
    assert_eq!(task_after.task_window, task_before.task_window);
    assert_eq!(task_after.lifecycle_status, task_before.lifecycle_status);
    assert_eq!(task_after.agent_attempts, task_before.agent_attempts);
    assert!(!task_after.has_checkout_mismatch());
    assert!(!task_after.has_side_flag(SideFlag::BranchMissing));

    let git_before = task_before.git_status.as_ref().unwrap();
    let git_after = task_after.git_status.as_ref().unwrap();
    assert!(!git_before.branch_exists);
    assert!(git_after.branch_exists);
    assert_eq!(git_after.current_branch.as_deref(), Some("fix/pane-stuck"));
    assert_ne!(task_before.branch, task_after.branch);
    assert_ne!(
        task_before.runtime_projection.health,
        task_after.runtime_projection.health
    );

    let events_after: Vec<_> = context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .iter()
        .map(|event| (event.kind, event.message.clone()))
        .collect();
    assert_eq!(
        events_after.len(),
        events_before.len() + 1,
        "expected one adoption event"
    );
    for (index, event) in events_before.iter().enumerate() {
        assert_eq!(&events_after[index], event);
    }
    assert_eq!(
        events_after.last().unwrap().0,
        crate::registry::RegistryEventKind::SubstrateChanged
    );
    assert_eq!(
        events_after.last().unwrap().1,
        "task branch adopted from ajax/fix-login to fix/pane-stuck"
    );
}

#[test]
fn checkout_mismatch_repair_rejects_stale_or_declined_adoption() {
    const STALE_REASON: &str = "checkout changed since repair was planned; refresh and retry";

    let mut context = context_with_named_checkout_mismatch();
    let repair_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let branch_before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .branch
        .clone();
    let event_count_before = context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .len();

    let (error, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Repair,
        "web/fix-login",
        &repair_plan,
        false,
        &mut RecordingQueuedRunner::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::ConfirmationRequired));
    assert!(!state_changed);
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .branch,
        branch_before
    );
    assert_eq!(
        context
            .registry
            .events_for_task(&TaskId::new("web/fix-login"))
            .len(),
        event_count_before
    );

    let mut tampered_plan = repair_plan.clone();
    tampered_plan.requires_confirmation = false;
    let (error, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Repair,
        "web/fix-login",
        &tampered_plan,
        false,
        &mut RecordingQueuedRunner::default(),
    )
    .unwrap_err();
    assert!(matches!(error, CommandError::ConfirmationRequired));
    assert!(!state_changed);

    type StaleCase = (
        &'static str,
        Box<dyn Fn(&mut CommandContext<InMemoryRegistry>)>,
    );
    let stale_cases: Vec<StaleCase> = vec![
        (
            "observed branch changed",
            Box::new(|context| {
                context
                    .registry
                    .get_task_mut(&TaskId::new("web/fix-login"))
                    .unwrap()
                    .git_status
                    .as_mut()
                    .unwrap()
                    .current_branch = Some("other/branch".to_string());
            }),
        ),
        (
            "detached checkout",
            Box::new(|context| {
                context
                    .registry
                    .get_task_mut(&TaskId::new("web/fix-login"))
                    .unwrap()
                    .git_status
                    .as_mut()
                    .unwrap()
                    .current_branch = None;
            }),
        ),
        (
            "missing worktree",
            Box::new(|context| {
                let task = context
                    .registry
                    .get_task_mut(&TaskId::new("web/fix-login"))
                    .unwrap();
                task.git_status.as_mut().unwrap().worktree_exists = false;
                task.add_side_flag(SideFlag::WorktreeMissing);
            }),
        ),
        (
            "task intent changed",
            Box::new(|context| {
                context
                    .registry
                    .get_task_mut(&TaskId::new("web/fix-login"))
                    .unwrap()
                    .branch = "ajax/other-branch".to_string();
            }),
        ),
    ];

    for (label, mutate) in stale_cases {
        let mut stale_context = context_with_named_checkout_mismatch();
        let stale_plan = plan_task_command_operation(
            &stale_context,
            TaskCommandKind::Repair,
            "web/fix-login",
            OpenMode::Attach,
        )
        .unwrap();
        mutate(&mut stale_context);
        let branch_before = stale_context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .branch
            .clone();
        let events_before = stale_context
            .registry
            .events_for_task(&TaskId::new("web/fix-login"))
            .len();

        let (error, state_changed) = execute_task_command_operation(
            &mut stale_context,
            TaskCommandKind::Repair,
            "web/fix-login",
            &stale_plan,
            true,
            &mut RecordingQueuedRunner::default(),
        )
        .unwrap_err();

        assert!(
            matches!(
                &error,
                CommandError::PlanBlocked(reasons) if reasons == &[STALE_REASON.to_string()]
            ),
            "case {label}: got {error:?}"
        );
        assert!(!state_changed, "case {label}");
        assert_eq!(
            stale_context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .branch,
            branch_before,
            "case {label}"
        );
        assert_eq!(
            stale_context
                .registry
                .events_for_task(&TaskId::new("web/fix-login"))
                .len(),
            events_before,
            "case {label}"
        );
    }
}

#[test]
fn repair_operation_promotes_task_to_reviewable_on_check_success() {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TestsFailed);
    let repair_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(
        repair_plan
            .commands
            .iter()
            .map(|_| CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
            .collect(),
    );

    let (outputs, state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Repair,
        "web/fix-login",
        &repair_plan,
        true,
        &mut runner,
    )
    .unwrap();

    assert_eq!(outputs.len(), repair_plan.commands.len());
    assert!(state_changed);
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    assert!(!task.has_side_flag(SideFlag::TestsFailed));
    assert!(task.live_status.is_none());
}

#[test]
fn repair_operation_records_tests_failed_on_check_failure() {
    let mut context = context_with_reviewable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Active;
    let repair_plan = plan_task_command_operation(
        &context,
        TaskCommandKind::Repair,
        "web/fix-login",
        OpenMode::Attach,
    )
    .unwrap();
    let mut runner = RecordingQueuedRunner::new(vec![CommandOutput {
        status_code: 42,
        stdout: String::new(),
        stderr: "tests failed".to_string(),
    }]);

    let (error, _state_changed) = execute_task_command_operation(
        &mut context,
        TaskCommandKind::Repair,
        "web/fix-login",
        &repair_plan,
        true,
        &mut runner,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CommandError::CommandRun(crate::adapters::CommandRunError::NonZeroExit {
            status_code: 42,
            ..
        })
    ));
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "check failed"))
    );
}

#[test]
fn drop_operation_plan_uses_fresh_observation_instead_of_cached_substrate() {
    let mut context = context_with_cleanable_task();
    let mut runner = QueuedRunner::new(vec![
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
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
    ]);

    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    assert_eq!(operation.observation.tmux_session, ResourceState::Absent);
    assert_eq!(operation.observation.worktree, ResourceState::Absent);
    assert_eq!(operation.observation.branch, ResourceState::Absent);
}

#[test]
fn branch_sensitive_checkout_mismatch_drop_uses_remove_plan() {
    let mut context = context_with_cleanable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap()
        .git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("dependabot/pip/minor".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    let mut runner = RecordingQueuedRunner::new(present_drop_observation_outputs());

    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    assert!(
        operation.confirmation_plan.blocked_reasons.is_empty(),
        "Drop/Remove must stay available on checkout mismatch"
    );
    assert!(
        operation
            .confirmation_plan
            .title
            .starts_with("remove task:"),
        "clean is blocked on mismatch; drop should fall through to remove"
    );
    assert_eq!(operation.observation.worktree, ResourceState::Present);
    assert_eq!(operation.observation.branch, ResourceState::Present);
    assert!(!runner.commands.is_empty());
}

#[test]
fn drop_execution_keeps_resource_specific_command_and_missing_rules() {
    let context = context_with_cleanable_task();

    let agent_decision =
        drop_op_execution_decision(&context, "web/fix-login", DropOp::EnsureAgentStopped, false)
            .unwrap();
    assert!(matches!(agent_decision, DropExecutionDecision::InProcess));

    let worktree_unforced = drop_op_execution_decision(
        &context,
        "web/fix-login",
        DropOp::EnsureWorktreeAbsent,
        false,
    )
    .unwrap();
    assert!(matches!(
        worktree_unforced,
        DropExecutionDecision::Command(ref command)
            if command
                == &crate::adapters::GitAdapter::new("git")
                    .remove_worktree("/repo/web", "/repo/web__worktrees/ajax-fix-login")
    ));

    let worktree_forced = drop_op_execution_decision(
        &context,
        "web/fix-login",
        DropOp::EnsureWorktreeAbsent,
        true,
    )
    .unwrap();
    assert!(matches!(
        worktree_forced,
        DropExecutionDecision::Command(ref command)
            if command.program == "sh"
                && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
    ));

    let branch_unforced =
        drop_op_execution_decision(&context, "web/fix-login", DropOp::EnsureBranchAbsent, false)
            .unwrap();
    assert!(matches!(
        branch_unforced,
        DropExecutionDecision::Command(ref command)
            if command.program == "sh"
                && command.args.get(2).map(String::as_str) == Some("ajax-delete-branch")
                && command.args[1].contains("branch -d")
    ));

    let branch_forced =
        drop_op_execution_decision(&context, "web/fix-login", DropOp::EnsureBranchAbsent, true)
            .unwrap();
    assert!(matches!(
        branch_forced,
        DropExecutionDecision::Command(ref command)
            if command.program == "sh"
                && command.args.get(2).map(String::as_str) == Some("ajax-delete-branch")
                && command.args[1].contains("branch -D")
    ));

    let tmux_decision = drop_op_execution_decision(
        &context,
        "web/fix-login",
        DropOp::EnsureTmuxSessionAbsent,
        false,
    )
    .unwrap();
    assert!(matches!(
        tmux_decision,
        DropExecutionDecision::Command(ref command)
            if command.program == "tmux"
                && command.args.iter().any(|arg| arg == "kill-session")
    ));
}

#[test]
fn drop_resource_catalog_preserves_receipt_policy_and_targets() {
    let task = context_with_cleanable_task()
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();

    let receipts = [
        DropOp::EnsureAgentStopped,
        DropOp::EnsureWorktreeAbsent,
        DropOp::EnsureBranchAbsent,
        DropOp::EnsureTmuxSessionAbsent,
    ]
    .into_iter()
    .map(|op| {
        (
            op,
            op.step_key(),
            op.receipt_target(&task),
            op.records_observed_absent_receipt(),
        )
    })
    .collect::<Vec<_>>();

    assert_eq!(
        receipts,
        vec![
            (
                DropOp::EnsureAgentStopped,
                "agent_stopped",
                "ajax-web-fix-login".to_string(),
                false,
            ),
            (
                DropOp::EnsureWorktreeAbsent,
                "worktree_absent",
                "/repo/web__worktrees/ajax-fix-login".to_string(),
                true,
            ),
            (
                DropOp::EnsureBranchAbsent,
                "branch_absent",
                "ajax/fix-login".to_string(),
                true,
            ),
            (
                DropOp::EnsureTmuxSessionAbsent,
                "tmux_session_absent",
                "ajax-web-fix-login".to_string(),
                true,
            ),
        ]
    );
}

#[test]
fn drop_operation_removes_failed_or_orphaned_tasks_when_resources_are_absent() {
    for lifecycle_status in [LifecycleStatus::Error, LifecycleStatus::Orphaned] {
        let mut context = context();
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        );
        task.lifecycle_status = lifecycle_status;
        context.registry.create_task(task).unwrap();
        let mut outputs = absent_drop_observation_outputs();
        outputs.extend(absent_drop_observation_outputs());
        let mut runner = RecordingQueuedRunner::new(outputs);
        let operation =
            plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

        let (_outputs, completion) = execute_drop_task_operation(
            &mut context,
            "web/fix-login",
            operation,
            true,
            &mut runner,
        )
        .unwrap();

        assert_eq!(completion, DropTaskCompletion::Removed);
        assert!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .is_none(),
            "{lifecycle_status:?}"
        );
    }
}

#[test]
fn drop_operation_force_deletes_unmerged_branch_on_confirmed_cleanup() {
    let mut context = context_with_cleanable_task();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("web/fix-login"))
            .unwrap();
        if let Some(git_status) = task.git_status.as_mut() {
            git_status.merged = false;
        }
    }
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(runner.commands.iter().any(|command| {
        command.program == "sh"
            && command.args.get(2) == Some(&"ajax-delete-branch".to_string())
            && command.args[1].contains("branch -D")
            && command.args[4] == "ajax/fix-login"
    }));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "sh"
            && command.args.get(2) == Some(&"ajax-delete-branch".to_string())
            && command.args[1].contains("branch -d")
            && command.args[4] == "ajax/fix-login"
    }));
}
