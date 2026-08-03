use super::super::*;
use super::*;

#[test]
fn open_use_case_module_targets_task_directly() {
    let context = context_with_tasks();

    let plan = open::open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert_eq!(plan.title, "open task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn open_task_plan_emits_no_commands_for_no_attach_mode() {
    let context = context_with_tasks();

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::NoAttach).unwrap();

    assert!(plan.blocked_reasons.is_empty(), "{plan:?}");
    assert!(plan.commands.is_empty(), "{plan:?}");
}

#[test]
fn open_task_plan_blocks_removed_tasks() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task is removed"]);
}

#[test]
fn direct_task_plans_block_removed_tasks() {
    let mut context = context_with_test_command();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Removed;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });

    let plans = [
        open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap(),
        merge_task_plan(&context, "web/fix-login").unwrap(),
        clean_task_plan(&context, "web/fix-login").unwrap(),
        check_task_plan(&context, "web/fix-login").unwrap(),
        diff_task_plan(&context, "web/fix-login").unwrap(),
    ];

    for plan in plans {
        assert!(plan.commands.is_empty(), "{}", plan.title);
        assert!(
            plan.blocked_reasons
                .iter()
                .any(|reason| reason == "task is removed"),
            "{}: {:?}",
            plan.title,
            plan.blocked_reasons
        );
    }
}

#[test]
fn check_task_plan_runs_configured_command_in_task_worktree() {
    let context = context_with_test_command();

    let plan = check_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan.title, "check task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![CommandSpec::new("sh", ["-lc", "cargo test"]).with_cwd("/tmp/worktrees/web-fix-login")]
    );
}

#[test]
fn check_use_case_module_plans_configured_command_in_task_worktree() {
    let context = context_with_test_command();

    let plan = check::check_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan.title, "check task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![CommandSpec::new("sh", ["-lc", "cargo test"]).with_cwd("/tmp/worktrees/web-fix-login")]
    );
}

#[test]
fn check_task_plan_blocks_missing_worktree() {
    let mut context = context_with_test_command();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::WorktreeMissing);

    let plan = check_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
}

#[test]
fn diff_task_plan_summarizes_branch_diff_in_task_worktree() {
    let context = context_with_tasks();

    let plan = diff_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan.title, "diff task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![CommandSpec::new("git", ["diff", "--stat", "main...HEAD"])
            .with_cwd("/tmp/worktrees/web-fix-login")]
    );
}

#[test]
fn diff_use_case_module_summarizes_branch_diff_in_task_worktree() {
    let context = context_with_tasks();

    let plan = diff::diff_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan.title, "diff task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![CommandSpec::new("git", ["diff", "--stat", "main...HEAD"])
            .with_cwd("/tmp/worktrees/web-fix-login")]
    );
}

#[test]
fn review_slice_facade_summarizes_branch_diff_in_task_worktree() {
    let context = context_with_tasks();

    let plan = crate::commands::diff_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan, diff_task_plan(&context, "web/fix-login").unwrap());
}

#[test]
fn diff_task_plan_blocks_missing_worktree() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::WorktreeMissing);

    let plan = diff_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
}

#[test]
fn checkout_mismatch_keeps_open_check_and_review_available() {
    let mut context = context_with_test_command();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));

    let open_plan = open_task_plan(&context, "web/fix-login", OpenMode::NoAttach).unwrap();
    assert!(open_plan.blocked_reasons.is_empty());

    let check_plan = check_task_plan(&context, "web/fix-login").unwrap();
    assert!(check_plan.blocked_reasons.is_empty());
    assert_eq!(
        check_plan.commands,
        vec![CommandSpec::new("sh", ["-lc", "cargo test"]).with_cwd("/tmp/worktrees/web-fix-login")]
    );

    let diff_plan = diff_task_plan(&context, "web/fix-login").unwrap();
    assert!(diff_plan.blocked_reasons.is_empty());
    assert_eq!(
        diff_plan.commands,
        vec![CommandSpec::new("git", ["diff", "--stat", "main...HEAD"])
            .with_cwd("/tmp/worktrees/web-fix-login")]
    );
}

#[test]
fn task_window_repair_plan_still_repairs_missing_tmux_flag() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::TmuxMissing);

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(!plan.commands.is_empty());
    assert!(plan.blocked_reasons.is_empty());
}

#[test]
fn task_window_use_case_module_repairs_missing_tmux_flag() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::TmuxMissing);

    let plan = task_window::task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(!plan.commands.is_empty());
    assert!(plan.blocked_reasons.is_empty());
}

#[test]
fn open_task_plan_blocks_missing_tmux_instead_of_repairing_task_window() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = None;

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert_eq!(plan.title, "open task: web/fix-login");
    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[test]
fn open_task_plan_blocks_missing_tmux_as_not_openable() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[test]
fn open_task_plan_blocks_missing_tmux_inside_tmux() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = None;

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

    assert_eq!(plan.title, "open task: web/fix-login");
    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[test]
fn open_task_plan_blocks_unobservable_runtime_projection_until_refresh() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Unobservable,
        std::time::SystemTime::UNIX_EPOCH,
        RuntimeObservationSource::TmuxProbe,
    );

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(
        plan.blocked_reasons,
        vec!["runtime state is unobservable; refresh before resume"]
    );
}

#[test]
fn open_task_plan_allows_old_healthy_complete_runtime_projection() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        std::time::SystemTime::UNIX_EPOCH,
        RuntimeObservationSource::TmuxProbe,
    );

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.blocked_reasons.is_empty());
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn open_task_plan_allows_stale_runtime_when_live_status_requests_resume() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
    task.task_window_status = Some(TaskWindowStatus::present(
        "task",
        "/tmp/worktrees/web-fix-login",
    ));
    task.live_status = Some(LiveObservation::new(LiveStatusKind::Blocked, "blocked"));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        std::time::SystemTime::UNIX_EPOCH,
        RuntimeObservationSource::TmuxProbe,
    );

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.blocked_reasons.is_empty());
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn lifecycle_transitions_update_registry_status() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Created;

    mark_task_opened(&mut context, "web/fix-login").unwrap();
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Created
    );

    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Mergeable;
    mark_task_merged(&mut context, "web/fix-login").unwrap();
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged
    );

    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Cleanable;
    mark_task_removed(&mut context, "web/fix-login").unwrap();
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Removed
    );
}

#[test]
fn mark_task_opened_preserves_existing_lifecycle() {
    for status in [
        LifecycleStatus::Reviewable,
        LifecycleStatus::Merged,
        LifecycleStatus::Cleanable,
    ] {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = status;

        mark_task_opened(&mut context, "web/fix-login").unwrap();

        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            status
        );
    }
}

#[test]
fn mark_task_opened_preserves_claude_evidence_and_lifecycle() {
    let mut context = context_with_tasks();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.selected_agent = AgentClient::Claude;
        task.lifecycle_status = LifecycleStatus::Active;
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(900);

    mark_task_opened_at(&mut context, "web/fix-login", at).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.attention_acknowledged_at, Some(at));
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}

#[test]
fn mark_task_opened_preserves_codex_evidence_and_lifecycle() {
    let mut context = context_with_tasks();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.selected_agent = AgentClient::Codex;
        task.lifecycle_status = LifecycleStatus::Active;
        task.agent_status = AgentRuntimeStatus::Waiting;
        task.add_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ));
    }
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(900);

    mark_task_opened_at(&mut context, "web/fix-login", at).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.attention_acknowledged_at, Some(at));
    assert_eq!(task.agent_status, AgentRuntimeStatus::Waiting);
    assert!(task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}

#[test]
fn merge_plan_requires_confirmation_when_task_needs_attention() {
    let context = context_with_tasks();

    let plan = merge_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.requires_confirmation);
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"]),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "merge",
                    "--ff-only",
                    "ajax/fix-login"
                ]
            )
        ]
    );
}

#[test]
fn merge_task_plan_blocks_non_review_states() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Active;

    let plan = merge_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(
        plan.blocked_reasons,
        vec!["merge requires reviewable or mergeable lifecycle"]
    );
}

#[test]
fn merge_task_plan_allows_mergeable_tasks() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Mergeable;

    let plan = merge_task_plan(&context, "web/fix-login").unwrap();

    assert!(!plan.commands.is_empty());
    assert!(plan.blocked_reasons.is_empty());
}

#[test]
fn merge_result_updates_replace_failed_merge_attention() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Mergeable;
    task.add_side_flag(SideFlag::Conflicted);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CommandFailed,
        "merge failed",
    ));

    mark_task_merged(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Merged);
    assert!(!task.has_side_flag(SideFlag::Conflicted));
    assert!(task.live_status.is_none());
}

#[test]
fn merge_result_preserves_unrelated_command_failure_attention() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Mergeable;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CiFailed,
        "check failed",
    ));

    mark_task_merged(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CiFailed && status.summary == "check failed"
    }));
}
