use super::super::*;
use super::*;

#[test]
fn teardown_step_result_records_matching_worktree_cleanup() {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    let git_status = task.git_status.as_mut().unwrap();
    git_status.dirty = true;
    git_status.conflicted = true;
    git_status.untracked_files = 2;
    task.add_side_flag(SideFlag::Dirty);
    task.add_side_flag(SideFlag::Conflicted);
    let command = CommandSpec::new(
        "git",
        [
            "-C",
            "/Users/matt/projects/web",
            "worktree",
            "remove",
            "/tmp/worktrees/web-fix-login",
        ],
    );

    let changed =
        mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(changed);
    assert!(!git_status.worktree_exists);
    assert!(!git_status.dirty);
    assert!(!git_status.conflicted);
    assert_eq!(git_status.untracked_files, 0);
    assert!(!task.has_side_flag(SideFlag::Dirty));
    assert!(!task.has_side_flag(SideFlag::Conflicted));
    assert!(task.has_side_flag(SideFlag::WorktreeMissing));
}

#[test]
fn teardown_step_result_records_matching_branch_cleanup() {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    let git_status = task.git_status.as_mut().unwrap();
    git_status.ahead = 2;
    git_status.behind = 1;
    git_status.unpushed_commits = 2;
    task.add_side_flag(SideFlag::Unpushed);
    let command = CommandSpec::new(
        "git",
        [
            "-C",
            "/Users/matt/projects/web",
            "branch",
            "-d",
            "ajax/fix-login",
        ],
    );

    let changed =
        mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(changed);
    assert!(!git_status.branch_exists);
    assert!(git_status.current_branch.is_none());
    assert_eq!(git_status.ahead, 0);
    assert_eq!(git_status.behind, 0);
    assert_eq!(git_status.unpushed_commits, 0);
    assert!(!task.has_side_flag(SideFlag::Unpushed));
    assert!(task.has_side_flag(SideFlag::BranchMissing));
}

#[test]
fn cleanup_git_status_bookkeeping_updates_only_cleanup_evidence() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Merged;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.git_status = None;
    task.tmux_status = None;
    task.task_window_status = None;
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login\n",
    )]);

    ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

    assert_eq!(
        runner.commands,
        vec![CommandSpec::new(
            "git",
            [
                "-C",
                "/tmp/worktrees/web-fix-login",
                "status",
                "--porcelain=v1",
                "--branch"
            ]
        )]
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Merged);
    assert!(task.git_status.as_ref().is_some_and(|status| {
        status.worktree_exists
            && status.branch_exists
            && status.merged
            && !status.dirty
            && status.untracked_files == 0
    }));
    assert!(task.tmux_status.is_none());
    assert!(task.task_window_status.is_none());
    assert!(task.live_status.is_none());
}

#[test]
fn cleanup_git_status_refreshes_even_when_cached_status_exists() {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status.as_mut().unwrap().dirty = true;
    task.add_side_flag(SideFlag::Dirty);
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login\n",
    )]);

    ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

    assert_eq!(
        runner.commands,
        vec![CommandSpec::new(
            "git",
            [
                "-C",
                "/tmp/worktrees/web-fix-login",
                "status",
                "--porcelain=v1",
                "--branch"
            ]
        )]
    );
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.git_status.as_ref().unwrap().dirty);
    assert!(!task.has_side_flag(SideFlag::Dirty));
}

#[test]
fn cleanup_git_status_keeps_active_unmerged_evidence_unmerged() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
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
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login\n",
    )]);

    ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.git_status.as_ref().unwrap().merged);
}

#[test]
fn cleanup_git_status_treats_cleanable_task_as_merged_even_without_cached_merge() {
    let mut context = context_with_cleanable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .git_status
        .as_mut()
        .unwrap()
        .merged = false;
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login\n",
    )]);

    ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.git_status.as_ref().unwrap().merged);
}

#[test]
fn git_evidence_refresh_parses_status_and_side_flags() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = None;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login [ahead 2]\nUU src/lib.rs\n?? notes.md\n",
    )]);

    refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(git_status.worktree_exists);
    assert!(git_status.branch_exists);
    assert_eq!(git_status.current_branch.as_deref(), Some("ajax/fix-login"));
    assert!(git_status.dirty);
    assert!(git_status.conflicted);
    assert_eq!(git_status.untracked_files, 1);
    assert_eq!(git_status.unpushed_commits, 2);
    assert!(task.has_side_flag(SideFlag::Dirty));
    assert!(task.has_side_flag(SideFlag::Conflicted));
    assert!(task.has_side_flag(SideFlag::Unpushed));
}

#[test]
fn git_evidence_refresh_clears_recovered_missing_worktree_and_branch_flags() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::WorktreeMissing);
    task.add_side_flag(SideFlag::BranchMissing);
    let mut runner = QueuedRunner::new(vec![output(
        0,
        "## ajax/fix-login...origin/ajax/fix-login\n",
    )]);

    refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(!task.has_side_flag(SideFlag::BranchMissing));
}

#[test]
fn git_evidence_refresh_preserves_unresolved_missing_flags() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::BranchMissing);
    let mut runner = QueuedRunner::new(vec![output(0, "## HEAD (no branch)\n")]);

    refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(task.has_side_flag(SideFlag::BranchMissing));
}

#[test]
fn failed_git_evidence_refresh_preserves_existing_missing_flags() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::WorktreeMissing);
    let mut runner = QueuedRunner::new(vec![CommandOutput {
        status_code: 128,
        stdout: String::new(),
        stderr: "not a git repository".to_string(),
    }]);

    let result = refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false);

    assert!(result.is_err());
    assert!(context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .has_side_flag(SideFlag::WorktreeMissing));
}

#[test]
fn confirmed_cleanup_deletes_existing_unmerged_branch() {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status.as_mut().unwrap().merged = false;
    task.add_side_flag(SideFlag::NeedsInput);

    let plan = clean_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-D",
                    "ajax/fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
        ]
    );
}

#[test]
fn sweep_cleanup_plans_only_safe_candidates() {
    let context = context_with_cleanable_task();

    let plan = sweep_cleanup_plan(&context);
    let candidates = sweep_cleanup_candidates(&context);

    assert_eq!(candidates, vec!["web/fix-login"]);
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]),
            CommandSpec::new(
                "sh",
                [
                    "-c",
                    "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi",
                    "ajax-trash-sweep",
                    "/tmp/worktrees/.ajax-trash",
                ]
            )
        ]
    );
}

#[test]
fn sweep_cleanup_ignores_removed_tasks() {
    let mut context = context_with_cleanable_task();
    context
        .registry
        .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Removed)
        .unwrap();

    let plan = sweep_cleanup_plan(&context);
    let candidates = sweep_cleanup_candidates(&context);

    assert_eq!(
        plan.commands,
        vec![CommandSpec::new(
            "sh",
            [
                "-c",
                "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi",
                "ajax-trash-sweep",
                "/tmp/worktrees/.ajax-trash",
            ]
        )]
    );
    assert!(candidates.is_empty());
}

#[test]
fn open_task_plan_blocks_missing_trunk_substrate() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TmuxMissing);
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
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

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

    assert_eq!(plan.title, "open task: web/fix-login");
    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[rstest]
#[case::task_side_flag(|task: &mut Task| task.add_side_flag(SideFlag::TaskWindowMissing))]
#[case::tmux_status_missing(|task: &mut Task| {
    task.tmux_status = Some(TmuxStatus {
        exists: false,
        session_name: "ajax-web-fix-login".to_string(),
    });
})]
#[case::task_window_status_missing(|task: &mut Task| {
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: false,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
})]
#[case::task_wrong_path(|task: &mut Task| {
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/other".into(),
        points_at_expected_path: false,
    });
})]
fn open_task_plan_blocks_each_trunk_substrate_signal(#[case] arrange_task: fn(&mut Task)) {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    arrange_task(task);

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert_eq!(plan.title, "open task: web/fix-login");
    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[test]
fn open_task_plan_blocks_missing_git_substrate_instead_of_repairing_task_window() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TmuxMissing);
    task.add_side_flag(SideFlag::WorktreeMissing);

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[rstest]
#[case::missing_worktree(|status: &mut GitStatus| status.worktree_exists = false)]
#[case::missing_branch(|status: &mut GitStatus| status.branch_exists = false)]
fn open_task_plan_blocks_missing_git_status_instead_of_repairing_task_window(
    #[case] arrange_git_status: fn(&mut GitStatus),
) {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TmuxMissing);
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
    arrange_git_status(task.git_status.as_mut().unwrap());

    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
}

#[test]
fn mark_task_opened_reports_missing_task() {
    let mut context = context_with_tasks();

    let result = mark_task_opened(&mut context, "web/missing");

    assert!(matches!(result, Err(CommandError::TaskNotFound(handle)) if handle == "web/missing"));
}

#[test]
fn command_result_markers_update_visible_task_state() {
    let mut context = context_with_test_command();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::TestsFailed);
    }

    mark_task_check_started(&mut context, "web/fix-login").unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::TestsFailed));
    assert!(task
        .live_status
        .as_ref()
        .is_some_and(|status| status.kind == LiveStatusKind::TestsRunning));

    mark_task_check_succeeded(&mut context, "web/fix-login").unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
    assert!(task.live_status.is_none());

    mark_task_check_failed(&mut context, "web/fix-login").unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.has_side_flag(SideFlag::TestsFailed));
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CiFailed && status.summary == "check failed"
    }));
}

#[test]
fn check_success_preserves_unrelated_live_status() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CommandFailed,
        "agent failed",
    ));

    mark_task_check_succeeded(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CommandFailed && status.summary == "agent failed"
    }));
}

#[test]
fn merge_and_trunk_result_markers_update_recovery_state() {
    let mut context = context_with_tasks();

    mark_task_merge_failed(&mut context, "web/fix-login", true).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.has_side_flag(SideFlag::Conflicted));
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::MergeConflict && status.summary == "merge failed"
    }));

    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::TmuxMissing,
        "tmux session missing",
    ));

    mark_task_window_repaired(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists && status.session_name == "ajax-web-fix-login"));
    assert!(task.task_window_status.as_ref().is_some_and(|status| {
        status.exists
            && status.window_name == "task"
            && status.points_at_expected_path
            && status.current_path == task.worktree_path
    }));
    assert!(task.live_status.is_none());
}

#[test]
fn force_remove_marks_task_removed_and_records_recovery_event() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::Stale);

    mark_task_force_removed(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
    assert!(!task.has_side_flag(SideFlag::Stale));
    assert!(context
        .registry
        .events_for_task(&TaskId::new("task-1"))
        .iter()
        .any(|event| event.message == "lifecycle changed to Removed"));
}
