use super::super::*;
use super::*;

#[test]
fn mark_task_check_failed_records_ci_failed_attention() {
    let mut context = context_with_tasks();

    mark_task_check_failed(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.has_side_flag(SideFlag::TestsFailed));
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CiFailed && status.summary == "check failed"
    }));
}

#[test]
fn mark_task_merge_failed_conflicted_records_merge_conflict_attention() {
    let mut context = context_with_tasks();

    mark_task_merge_failed(&mut context, "web/fix-login", true).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(task.has_side_flag(SideFlag::Conflicted));
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::MergeConflict && status.summary == "merge failed"
    }));
}

#[test]
fn mark_task_merge_failed_nonconflicted_keeps_command_failed_attention() {
    let mut context = context_with_tasks();

    mark_task_merge_failed(&mut context, "web/fix-login", false).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::Conflicted));
    assert!(task.live_status.as_ref().is_some_and(|status| {
        status.kind == LiveStatusKind::CommandFailed && status.summary == "merge failed"
    }));
}

#[test]
fn mark_task_merged_clears_conflicted_merge_failure_attention() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Mergeable;
    task.add_side_flag(SideFlag::Conflicted);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::MergeConflict,
        "merge failed",
    ));

    mark_task_merged(&mut context, "web/fix-login").unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(!task.has_side_flag(SideFlag::Conflicted));
    assert!(task.live_status.is_none());
}

#[rstest]
#[case(
    Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    }),
    None,
    "merge requires clean worktree evidence"
)]
#[case(
    Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: true,
        last_commit: None,
    }),
    None,
    "merge requires clean worktree evidence"
)]
#[case(
    Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 1,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    }),
    None,
    "merge requires clean worktree evidence"
)]
#[case(
    Some(GitStatus {
        worktree_exists: true,
        branch_exists: false,
        current_branch: None,
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    }),
    None,
    "task branch is missing"
)]
#[case(None, Some(SideFlag::Dirty), "merge requires clean worktree evidence")]
#[case(
    None,
    Some(SideFlag::Conflicted),
    "merge requires clean worktree evidence"
)]
#[case(None, Some(SideFlag::BranchMissing), "task branch is missing")]
fn merge_task_plan_blocks_risky_or_missing_branch_evidence(
    #[case] git_status: Option<GitStatus>,
    #[case] side_flag: Option<SideFlag>,
    #[case] expected_reason: &str,
) {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.git_status = git_status;
    if let Some(side_flag) = side_flag {
        task.add_side_flag(side_flag);
    }

    let plan = merge_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(plan.blocked_reasons, vec![expected_reason]);
}

#[test]
fn clean_plan_uses_policy_and_native_cleanup() {
    let context = context_with_cleanable_task();

    let plan = clean_task_plan(&context, "web/fix-login").unwrap();

    assert!(!plan.requires_confirmation);
    assert!(plan.blocked_reasons.is_empty());
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
        ]
    );
}

#[rstest]
#[case(SideFlag::Dirty)]
#[case(SideFlag::Conflicted)]
fn clean_plan_requires_confirmation_for_risky_cleanup(#[case] side_flag: SideFlag) {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(side_flag);
    if let Some(git_status) = task.git_status.as_mut() {
        match side_flag {
            SideFlag::Dirty => {
                git_status.dirty = true;
            }
            SideFlag::Conflicted => {
                git_status.conflicted = true;
            }
            _ => {}
        }
    }

    let plan = clean_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.requires_confirmation);
    assert!(!plan.commands.is_empty());
    assert!(plan.blocked_reasons.is_empty());
}

#[test]
fn clean_task_plan_blocks_non_cleanup_lifecycle() {
    let mut context = context_with_cleanable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Active;

    let plan = clean_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert_eq!(
        plan.blocked_reasons,
        vec!["clean requires merged or cleanable lifecycle"]
    );
}

#[test]
fn remove_task_plan_force_removes_active_task_resources() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.lifecycle_status = LifecycleStatus::Active;
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
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: task.tmux_session.clone(),
    });

    let plan = remove_task_plan(&context, "web/fix-login").unwrap();

    assert!(plan.requires_confirmation);
    assert!(plan.blocked_reasons.is_empty());
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
                    "--force",
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
fn remove_task_plan_keeps_removing_remaining_resources_for_invalid_tasks() {
    for arrange in [
        |task: &mut Task| {
            task.tmux_status = Some(TmuxStatus {
                exists: false,
                session_name: task.tmux_session.clone(),
            });
        },
        |task: &mut Task| {
            task.git_status.as_mut().unwrap().worktree_exists = false;
        },
        |task: &mut Task| {
            task.git_status.as_mut().unwrap().branch_exists = false;
        },
        |task: &mut Task| {
            task.task_window_status = Some(TaskWindowStatus {
                exists: false,
                window_name: task.task_window.clone(),
                current_path: task.worktree_path.clone(),
                points_at_expected_path: false,
            });
        },
    ] {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
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
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });
        task.task_window_status = Some(TaskWindowStatus::present(
            task.task_window.clone(),
            task.worktree_path.clone(),
        ));
        arrange(task);

        let plan = remove_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert!(
            !plan.commands.is_empty(),
            "invalid task should still remove remaining resources"
        );
        assert!(
            plan.commands
                .iter()
                .all(|command| { command.program == "tmux" || command.program == "git" }),
            "unexpected teardown commands: {:?}",
            plan.commands
        );
    }
}

#[test]
fn drop_plan_from_observation_resumes_from_live_resource_state() {
    let observation = DropObservation {
        agent: ResourceState::Present,
        tmux_session: ResourceState::Absent,
        worktree: ResourceState::Unknown,
        branch: ResourceState::Present,
    };

    let ops = plan_drop_from_observation(&observation);

    assert_eq!(
        ops,
        vec![
            DropOp::EnsureAgentStopped,
            DropOp::EnsureWorktreeAbsent,
            DropOp::EnsureBranchAbsent,
        ]
    );
}

#[test]
fn drop_plan_from_observation_tears_down_git_before_tmux() {
    let observation = DropObservation {
        agent: ResourceState::Present,
        tmux_session: ResourceState::Present,
        worktree: ResourceState::Present,
        branch: ResourceState::Present,
    };

    let ops = plan_drop_from_observation(&observation);

    assert_eq!(
        ops,
        vec![
            DropOp::EnsureAgentStopped,
            DropOp::EnsureWorktreeAbsent,
            DropOp::EnsureBranchAbsent,
            DropOp::EnsureTmuxSessionAbsent,
        ]
    );
}

#[test]
fn drop_plan_from_observation_for_task_skips_receipted_steps() {
    use crate::models::{StepReceipt, TaskId, TaskOperationKind};

    let observation = DropObservation {
        agent: ResourceState::Absent,
        tmux_session: ResourceState::Present,
        worktree: ResourceState::Present,
        branch: ResourceState::Present,
    };
    let receipts = vec![
        StepReceipt::succeeded(
            TaskId::new("web/fix-login"),
            TaskOperationKind::Drop,
            "worktree_absent",
            "/repo/web__worktrees/ajax-fix-login",
            "{}",
        ),
        StepReceipt::succeeded(
            TaskId::new("web/fix-login"),
            TaskOperationKind::Drop,
            "branch_absent",
            "ajax/fix-login",
            "{}",
        ),
    ];

    let ops = plan_drop_from_observation_for_task(&observation, &receipts);

    assert_eq!(ops, vec![DropOp::EnsureTmuxSessionAbsent]);
}

#[test]
fn observe_drop_resources_prefers_live_tmux_and_git_state_over_registry_cache() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("task-1");
    context
        .registry
        .update_tmux_status(
            &task_id,
            Some(TmuxStatus {
                exists: false,
                session_name: "ajax-web-fix-login".to_string(),
            }),
        )
        .unwrap();
    let task = context.registry.get_task(&task_id).unwrap().clone();
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
    ]);

    let observation = observe_drop_resources(&mut context, &task, &mut runner).unwrap();

    assert_eq!(observation.tmux_session, ResourceState::Present);
    assert_eq!(observation.worktree, ResourceState::Absent);
    assert_eq!(observation.branch, ResourceState::Present);
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "list",
                    "--porcelain"
                ]
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "--format=%(refname:short)"
                ]
            ),
        ]
    );
    let task = context.registry.get_task(&task_id).unwrap();
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| !status.worktree_exists && status.branch_exists));
}

#[test]
fn observe_drop_resources_marks_worktree_present_when_path_matches_even_if_branch_differs() {
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("task-1");
    let task = context.registry.get_task(&task_id).unwrap().clone();
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "worktree /tmp/worktrees/web-fix-login\nHEAD 1111111\nbranch refs/heads/docs/other\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
    ]);

    let observation = observe_drop_resources(&mut context, &task, &mut runner).unwrap();

    assert_eq!(observation.tmux_session, ResourceState::Present);
    assert_eq!(observation.worktree, ResourceState::Present);
    assert_eq!(observation.branch, ResourceState::Present);
    let task = context.registry.get_task(&task_id).unwrap();
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists && status.branch_exists));
}

#[test]
fn cleanup_and_remove_plans_are_distinct() {
    let mut context = context_with_cleanable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });

    let cleanup = clean_task_plan(&context, "web/fix-login").unwrap();
    let remove = remove_task_plan(&context, "web/fix-login").unwrap();

    assert!(!cleanup.requires_confirmation);
    assert!(remove.requires_confirmation);
    assert_ne!(cleanup.commands, remove.commands);
    assert!(remove.commands.iter().any(|command| {
        command.program == "git"
            && command.args.iter().any(|arg| arg == "--force")
            && command.args.iter().any(|arg| arg == "worktree")
    }));
    assert!(remove
        .commands
        .iter()
        .any(|command| { command.program == "git" && command.args.iter().any(|arg| arg == "-D") }));
}

#[test]
fn teardown_step_result_ignores_unrelated_resource_commands() {
    let unrelated_commands = [
        CommandSpec::new("tmux", ["kill-session", "-t", "other-session"]),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "remove",
                "/tmp/worktrees/other-task",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-d",
                "ajax/other-task",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "remove",
                "/tmp/worktrees/web-fix-login",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "prune",
                "/tmp/worktrees/web-fix-login",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "-d",
                "ajax/fix-login",
            ],
        ),
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "--list",
                "ajax/fix-login",
            ],
        ),
    ];

    for command in unrelated_commands {
        let mut context = context_with_cleanable_task();
        let changed =
            mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!changed);
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists && status.branch_exists));
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));
    }
}

#[test]
fn teardown_step_result_records_matching_tmux_cleanup() {
    let mut context = context_with_cleanable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::TmuxMissing);
    task.add_side_flag(SideFlag::TaskWindowMissing);
    let command = CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]);

    let changed =
        mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| !status.exists && status.session_name == "ajax-web-fix-login"));
    assert!(task.task_window_status.as_ref().is_some_and(|status| {
        !status.exists
            && status.window_name == "task"
            && !status.points_at_expected_path
            && status.current_path == task.worktree_path
    }));
    assert!(
        task.has_side_flag(SideFlag::TmuxMissing),
        "missing-substrate flags stay until drop completes so retries stay visible"
    );
    assert!(
        task.has_side_flag(SideFlag::TaskWindowMissing),
        "missing-substrate flags stay until drop completes so retries stay visible"
    );
}
