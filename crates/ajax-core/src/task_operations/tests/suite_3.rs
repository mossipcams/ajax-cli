use super::*;

#[test]
fn confirmed_drop_renames_worktree_to_trash_instead_of_deleting_inline() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    {
        let task = context.registry.get_task_mut(&task_id).unwrap();
        task.add_side_flag(SideFlag::Dirty);
        if let Some(git_status) = task.git_status.as_mut() {
            git_status.dirty = true;
        }
    }
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
    // Final re-observe after successful tear-down: path must be gone (path-only
    // presence would correctly keep TeardownIncomplete).
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (command_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    let fast_remove = runner
        .commands
        .iter()
        .find(|command| {
            command.program == "sh"
                && command.args.first().map(String::as_str) == Some("-c")
                && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
        })
        .expect("fast remove command");
    assert_eq!(
        fast_remove.args[1],
        "mkdir -p \"$(dirname \"$3\")\" && { [ ! -e \"$2\" ] || mv \"$2\" \"$3\"; } && { git -C \"$1\" worktree prune || git -C \"$1\" worktree remove --force \"$2\"; } && { rm -rf \"$3\" >/dev/null 2>&1 & }"
    );
    assert_eq!(fast_remove.args[3], "/repo/web");
    assert_eq!(fast_remove.args[4], "/repo/web__worktrees/ajax-fix-login");
    assert!(fast_remove.args[5].starts_with("/repo/web__worktrees/.ajax-trash/fix-login-"));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "git"
            && command.args.iter().any(|arg| arg == "worktree")
            && command.args.iter().any(|arg| arg == "remove")
    }));
    assert_eq!(command_outputs.len(), 3);
    assert_eq!(completion, DropTaskCompletion::Removed);

    assert!(context.registry.get_task(&task_id).is_none());
}

#[test]
fn execute_drop_always_force_deletes_branch_with_d() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

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
    let uses_force_worktree_remove = runner.commands.iter().any(|command| {
        command.program == "sh"
            && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
    }) || runner.commands.iter().any(|command| {
        command.program == "git"
            && command.args.iter().any(|arg| arg == "worktree")
            && command.args.iter().any(|arg| arg == "remove")
            && command.args.iter().any(|arg| arg == "--force")
    });
    assert!(
        uses_force_worktree_remove,
        "drop execute must force-remove worktree"
    );
    assert_eq!(completion, DropTaskCompletion::Removed);
}

#[test]
fn drop_execute_always_uses_force_worktree_remove_when_cleanable_merged() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert!(runner.commands.iter().any(|command| {
        command.program == "sh"
            && command.args.get(2).map(String::as_str) == Some("ajax-fast-worktree-remove")
    }));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "git"
            && command.args.iter().any(|arg| arg == "worktree")
            && command.args.iter().any(|arg| arg == "remove")
            && !command.args.iter().any(|arg| arg == "--force")
    }));
    assert_eq!(completion, DropTaskCompletion::Removed);
}

#[test]
fn fast_drop_mv_failure_marks_teardown_incomplete() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    {
        let task = context.registry.get_task_mut(&task_id).unwrap();
        task.add_side_flag(SideFlag::Dirty);
        if let Some(git_status) = task.git_status.as_mut() {
            git_status.dirty = true;
        }
    }
    let mut outputs = present_drop_observation_outputs();
    outputs.push(output(1, "", "mv: cannot move: No such file or directory"));
    outputs.extend([output(0, "", ""), output(0, "", "")]);
    outputs.extend([
        output(0, "", ""),
        output(0, "worktree /repo/web__worktrees/ajax-fix-login\n", ""),
        output(0, "", ""),
    ]);
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
        .unwrap_err();

    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.metadata
            .get("drop_failed_step_key")
            .map(String::as_str),
        Some("worktree_absent")
    );
    assert!(task
        .metadata
        .get("drop_failed_detail")
        .is_some_and(|detail| detail.contains("No such file or directory")));
}
#[test]
fn drop_failure_keeps_task_and_tmux_when_worktree_remove_fails_before_session_kill() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.push(output(
        2,
        "",
        "error: failed to remove worktree: permission denied",
    ));
    outputs.extend([
        output(0, "ajax-web-fix-login\n", ""),
        output(
            0,
            "worktree /repo/web__worktrees/ajax-fix-login\nbranch refs/heads/ajax/fix-login\n\n",
            "",
        ),
        output(0, "ajax/fix-login\n", ""),
    ]);
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
        .unwrap_err();

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("failed git step should leave task resumable");
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.metadata.get("drop_failed_step").map(String::as_str),
        Some("remove worktree")
    );
    assert!(task
        .metadata
        .get("drop_failed_detail")
        .is_some_and(|detail| detail.contains("permission denied")));
    assert!(context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .iter()
        .any(|event| event.message.contains("drop step failed: remove worktree")));
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(!runner.commands.iter().any(|command| {
        command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
    }));
}

#[test]
fn drop_failure_keeps_task_when_branch_remove_fails_after_worktree_removed() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([
        output(0, "", ""),
        output(0, "", ""),
        output(2, "", "error: refusing to delete checked out branch"),
        output(0, "ajax-web-fix-login\n", ""),
        output(0, "", ""),
        output(0, "ajax/fix-login\n", ""),
    ]);
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
        .unwrap_err();
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("branch-only cleanup should remain resumable");
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
}

#[test]
fn drop_operation_resumes_from_receipts_after_partial_success() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    context
        .registry
        .record_step_receipt(StepReceipt::succeeded(
            task_id.clone(),
            TaskOperationKind::Drop,
            "worktree_absent",
            "/repo/web__worktrees/ajax-fix-login",
            "{}",
        ))
        .unwrap();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
        output(0, "", ""),
    ]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (command_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(command_outputs.len(), 2);
    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(!runner.commands.iter().any(|command| {
        command.program == "git"
            && command.args.contains(&"worktree".to_string())
            && command.args.contains(&"remove".to_string())
    }));
    assert!(runner.commands.iter().any(|command| {
        command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
    }));
}

#[test]
fn drop_retry_repeats_receipted_step_when_fresh_observation_finds_resource_present() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    context
        .registry
        .get_task_mut(&task_id)
        .unwrap()
        .lifecycle_status = LifecycleStatus::TeardownIncomplete;
    context
        .registry
        .get_task_mut(&task_id)
        .unwrap()
        .metadata
        .insert(
            "drop_failed_step_key".to_string(),
            "branch_absent".to_string(),
        );
    context
        .registry
        .record_step_receipt(StepReceipt::succeeded(
            task_id,
            TaskOperationKind::Drop,
            "branch_absent",
            "ajax/fix-login",
            "{}",
        ))
        .unwrap();
    let mut outputs = absent_drop_observation_outputs();
    outputs[2] = output(0, "ajax/fix-login\n", "");
    outputs.push(output(0, "", ""));
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
            && command.args[4] == "ajax/fix-login"
    }));
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
}

#[test]
fn drop_operation_records_remaining_resource_when_empty_plan_still_finishes_incomplete() {
    let mut context = context_with_cleanable_task();
    let mut outputs = absent_drop_observation_outputs();
    outputs.extend(vec![
        output(0, "", ""),
        output(0, "", ""),
        output(0, "ajax/fix-login\n", ""),
    ]);
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner)
        .expect("drop operation should plan");

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .expect("drop operation should complete with incomplete teardown");

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(matches!(
        completion,
        DropTaskCompletion::TeardownIncomplete {
            failed_step: DropOp::EnsureBranchAbsent,
            ..
        }
    ));
    assert_eq!(
        task.metadata.get("drop_failed_step").map(String::as_str),
        Some("delete branch")
    );
}

#[test]
fn drop_completion_hard_deletes_task_when_final_observation_is_absent() {
    let mut context = context_with_cleanable_task();

    let completion = complete_drop_task_operation(
        &mut context,
        "web/fix-login",
        &crate::commands::DropObservation {
            agent: ResourceState::Absent,
            tmux_session: ResourceState::Absent,
            worktree: ResourceState::Absent,
            branch: ResourceState::Absent,
        },
    )
    .unwrap();

    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
}

#[test]
fn drop_completion_marks_teardown_incomplete_when_resources_remain() {
    let mut context = context_with_cleanable_task();

    let completion = complete_drop_task_operation(
        &mut context,
        "web/fix-login",
        &crate::commands::DropObservation {
            agent: ResourceState::Absent,
            tmux_session: ResourceState::Absent,
            worktree: ResourceState::Absent,
            branch: ResourceState::Present,
        },
    )
    .unwrap();

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(matches!(
        completion,
        DropTaskCompletion::TeardownIncomplete {
            failed_step: DropOp::EnsureBranchAbsent,
            detail,
        } if detail.contains("branch still present")
    ));
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.metadata.get("drop_failed_step").map(String::as_str),
        Some("delete branch")
    );
    assert!(task
        .metadata
        .get("drop_latest_observation")
        .is_some_and(|observation| observation.contains("branch=Present")));
}

#[test]
fn drop_operation_executes_teardown_and_completes_from_final_observation() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([output(0, "", ""), output(0, "", ""), output(0, "", "")]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(outputs.len(), 3);
    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
    assert!(runner.commands.iter().any(|command| {
        command.program == "tmux" && command.args.iter().any(|arg| arg == "kill-session")
    }));
    assert!(runner.commands.iter().any(|command| {
        command.program == "git" && command.args.iter().any(|arg| arg == "worktree")
    }));
    assert!(runner.commands.iter().any(|command| {
        command.program == "sh" && command.args.get(2) == Some(&"ajax-delete-branch".to_string())
    }));

    assert!(context
        .registry
        .step_receipts_for_task(&TaskId::new("web/fix-login"))
        .is_empty());
}

#[test]
fn drop_operation_records_skipped_receipts_for_already_missing_resources() {
    let mut context = context_with_cleanable_task();
    let mut outputs = absent_drop_observation_outputs();
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
        .unwrap();

    assert!(context
        .registry
        .step_receipts_for_task(&TaskId::new("web/fix-login"))
        .is_empty());
}

#[test]
fn drop_operation_treats_invalid_branch_delete_error_as_already_absent() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([
        output(0, "", ""),
        output(
            128,
            "",
            "fatal: 'ajax/fix-login' is not a valid branch name",
        ),
        output(0, "", ""),
    ]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(outputs.len(), 3);
    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
}

#[test]
fn drop_operation_treats_remote_branch_not_found_as_already_absent() {
    let mut context = context_with_cleanable_task();
    let mut outputs = present_drop_observation_outputs();
    outputs.extend([
        output(0, "", ""),
        output(
            1,
            "",
            "error: unable to delete 'ajax/fix-login': remote ref does not exist",
        ),
        output(0, "", ""),
    ]);
    outputs.extend(absent_drop_observation_outputs());
    let mut runner = RecordingQueuedRunner::new(outputs);
    let operation = plan_drop_task_operation(&mut context, "web/fix-login", &mut runner).unwrap();

    let (_outputs, completion) =
        execute_drop_task_operation(&mut context, "web/fix-login", operation, true, &mut runner)
            .unwrap();

    assert_eq!(completion, DropTaskCompletion::Removed);
    assert!(context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .is_none());
}

#[test]
fn sweep_cleanup_marks_teardown_incomplete_when_final_observation_still_finds_tmux() {
    let mut context = context_with_cleanable_task();
    let plan = crate::commands::clean_task_plan(&context, "web/fix-login").unwrap();
    let mut runner_outputs = crate::commands::sweep_trash_commands(&context)
        .iter()
        .map(|_| output(0, "", ""))
        .collect::<Vec<_>>();
    runner_outputs.push(output(0, "ajax-web-fix-login\n", ""));
    runner_outputs.extend(plan.commands.iter().map(|_| output(0, "", "")));
    let mut runner = RecordingQueuedRunner::new(runner_outputs);

    execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("task should remain when tmux is still present");
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert!(task
        .metadata
        .get("drop_failed_detail")
        .is_some_and(|detail| detail.contains("tmux")));
}

#[test]
fn tidy_still_projects_each_successful_cleanup_command() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");
    {
        let task = context.registry.get_task_mut(&task_id).unwrap();
        task.agent_status = AgentRuntimeStatus::Running;
    }

    let trash_sweeps = crate::commands::sweep_trash_commands(&context);
    let plan = crate::commands::clean_task_plan(&context, "web/fix-login").unwrap();
    let mut runner_outputs: Vec<CommandOutput> =
        trash_sweeps.iter().map(|_| output(0, "", "")).collect();
    runner_outputs.push(output(1, "", "boom"));
    runner_outputs.extend(plan.commands.iter().map(|_| output(0, "", "")));
    runner_outputs.push(output(1, "", "unexpected git command"));
    runner_outputs.push(output(1, "", "unexpected git command"));
    let mut runner = RecordingQueuedRunner::new(runner_outputs);

    let (_outputs, state_changed) =
        execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    let task = context.registry.get_task(&task_id).unwrap();
    assert!(state_changed);
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists && !status.branch_exists));
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| !status.exists));
    assert!(task
        .task_window_status
        .as_ref()
        .is_some_and(|status| !status.exists));
}

#[test]
fn sweep_cleanup_removes_stale_trash_entries() {
    let mut context = context_with_cleanable_task();
    let mut runner = RecordingQueuedRunner::new(sweep_success_runner_outputs(&context));

    execute_sweep_cleanup_operation(&mut context, true, &mut runner, None).unwrap();

    let trash_sweep = runner
        .commands
        .iter()
        .find(|command| {
            command.program == "sh"
                && command.args.first().map(String::as_str) == Some("-c")
                && command.args.get(2).map(String::as_str) == Some("ajax-trash-sweep")
        })
        .expect("trash sweep command");
    assert_eq!(
        trash_sweep.args[1],
        "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi"
    );
    assert_eq!(trash_sweep.args[3], "/repo/web__worktrees/.ajax-trash");
}
