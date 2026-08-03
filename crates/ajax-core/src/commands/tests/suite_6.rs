use super::super::*;
use super::*;

#[test]
fn task_window_repair_plan_recreates_missing_worktree_when_branch_exists() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
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

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(plan.blocked_reasons.is_empty());
    let worktree_add = plan
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("expected git worktree add command");
    assert_eq!(
        worktree_add.args,
        vec![
            "-C",
            "/Users/matt/projects/web",
            "worktree",
            "add",
            "/tmp/worktrees/web-fix-login",
            "ajax/fix-login",
        ]
    );
    assert!(!worktree_add.args.iter().any(|arg| arg == "-b"));
    assert_eq!(
        plan.commands,
        vec![
            worktree_add.clone(),
            CommandSpec::new(
                "tmux",
                [
                    "new-session",
                    "-d",
                    "-s",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn task_window_repair_plan_ignores_stale_current_branch_when_worktree_is_missing() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
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

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(plan.blocked_reasons.is_empty());
    assert!(!plan
        .blocked_reasons
        .iter()
        .any(|reason| reason.contains("occupied")));
    let worktree_add = plan
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("expected git worktree add command");
    assert_eq!(
        worktree_add.args,
        vec![
            "-C",
            "/Users/matt/projects/web",
            "worktree",
            "add",
            "/tmp/worktrees/web-fix-login",
            "ajax/fix-login",
        ]
    );
    assert!(!worktree_add.args.iter().any(|arg| arg == "-b"));
}

#[test]
fn task_window_repair_plan_blocks_missing_worktree_when_branch_missing() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
        branch_exists: false,
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

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(plan.commands.is_empty());
    assert!(!plan.commands.iter().any(is_git_worktree_add_command));
    assert_eq!(
        plan.blocked_reasons,
        vec!["task worktree is missing: /tmp/worktrees/web-fix-login"]
    );
}

#[test]
fn task_window_repair_plan_recreates_missing_tmux_session_with_task() {
    let context = context_with_tasks();

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new(
                "tmux",
                [
                    "new-session",
                    "-d",
                    "-s",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn task_window_repair_plan_switches_client_when_inside_tmux() {
    let context = context_with_tasks();

    let plan =
        task_window_repair_plan_with_open_mode(&context, "web/fix-login", OpenMode::SwitchClient)
            .unwrap();

    assert_eq!(
        plan.commands.last(),
        Some(
            &CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        )
    );
}

#[test]
fn task_window_repair_plan_repairs_task_when_tmux_session_exists() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
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

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("tmux", ["kill-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn task_window_repair_plan_creates_missing_task_when_tmux_session_exists() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: "ajax-web-fix-login".to_string(),
    });
    task.task_window_status = Some(TaskWindowStatus {
        exists: false,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: false,
    });

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktrees/web-fix-login"
                ]
            ),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}

#[test]
fn execute_plan_runs_safe_commands() {
    let context = context_with_tasks();
    let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
    let mut runner = RecordingCommandRunner::default();

    let outputs = execute_plan(&plan, false, &mut runner).unwrap();

    assert_eq!(outputs.len(), 2);
    assert_eq!(runner.commands(), plan.commands.as_slice());
}

#[test]
fn execute_plan_requires_confirmation_for_risky_commands() {
    let context = context_with_tasks();
    let plan = merge_task_plan(&context, "web/fix-login").unwrap();
    let mut runner = RecordingCommandRunner::default();

    let error = execute_plan(&plan, false, &mut runner).unwrap_err();

    assert_eq!(error, CommandError::ConfirmationRequired);
    assert!(runner.commands().is_empty());
}

#[test]
fn execute_plan_refuses_blocked_commands() {
    let mut runner = RecordingCommandRunner::default();
    let mut plan = CommandPlan::new("blocked");
    plan.blocked_reasons.push("worktree is missing".to_string());
    plan.commands.push(CommandSpec::new("git", ["status"]));

    let error = execute_plan(&plan, true, &mut runner).unwrap_err();

    assert_eq!(
        error,
        CommandError::PlanBlocked(vec!["worktree is missing".to_string()])
    );
    assert!(runner.commands().is_empty());
}

#[test]
fn execute_plan_rejects_nonzero_command_outputs() {
    let mut runner = QueuedRunner::new(vec![output(2, "nope")]);
    let mut plan = CommandPlan::new("failing");
    plan.commands
        .push(CommandSpec::new("git", ["merge", "ajax/fix-login"]));

    let error = execute_plan(&plan, true, &mut runner).unwrap_err();

    assert_eq!(
        error,
        CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: 2,
            stderr: String::new(),
            cwd: None,
        })
    );
}

#[test]
fn execute_plan_reports_nonzero_command_cwd() {
    let mut runner = QueuedRunner::new(vec![output(1, "Error: Not in a git repository\n")]);
    let mut plan = CommandPlan::new("failing");
    plan.commands.push(
        CommandSpec::new("git", ["status"]).with_cwd("/Users/matt/Desktop/Projects/autodoctor"),
    );

    let error = execute_plan(&plan, true, &mut runner).unwrap_err();

    assert_eq!(
        error,
        CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: 1,
            stderr: String::new(),
            cwd: Some("/Users/matt/Desktop/Projects/autodoctor".into()),
        })
    );
}
