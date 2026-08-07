use super::super::*;
use super::*;

#[test]
fn doctor_reports_repo_config_problems() {
    let config = Config {
        repos: vec![
            ManagedRepo::new("web", "/repos/web", "main"),
            ManagedRepo::new("web", "/missing/web-copy", "main"),
            ManagedRepo::new("api", "/missing/api", "main"),
        ],
        test_commands: vec![TestCommand::new("web", "cargo test")],
        ..Config::default()
    };
    let context = CommandContext::new(config, InMemoryRegistry::default());
    let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
        .with_existing_paths(["/repos/web"]);

    let doctor = doctor_with_environment(&context, &environment);

    assert_eq!(
        doctor
            .checks
            .iter()
            .find(|check| check.name == "config:repo-names")
            .map(|check| (check.ok, check.message.as_str())),
        Some((false, "duplicate repo name: web"))
    );
    assert_eq!(
        doctor
            .checks
            .iter()
            .find(|check| check.name == "repo:api:path")
            .map(|check| check.ok),
        Some(false)
    );
    assert_eq!(
        doctor
            .checks
            .iter()
            .find(|check| check.name == "repo:api:test-command")
            .map(|check| (check.ok, check.message.as_str())),
        Some((false, "no test command configured"))
    );
}

#[test]
fn stale_task_marking_marks_inactive_old_tasks() {
    let mut context = context_with_tasks();
    let old_activity = std::time::SystemTime::UNIX_EPOCH;
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .last_activity_at = old_activity;

    let changed = mark_stale_tasks(
        &mut context,
        old_activity + std::time::Duration::from_secs(8 * 24 * 60 * 60),
    );

    assert_eq!(changed, 1);
    assert!(context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .has_side_flag(SideFlag::Stale));
}

#[test]
fn refresh_git_substrate_evidence_updates_stale_missing_worktree_and_branch() {
    let mut context = context_with_tasks();
    let task_id = TaskId::new("task-1");
    context
        .registry
        .update_git_status(
            &task_id,
            GitStatus {
                worktree_exists: true,
                branch_exists: true,
                current_branch: Some("ajax/fix-login".to_string()),
                dirty: true,
                ahead: 2,
                behind: 0,
                merged: false,
                untracked_files: 1,
                unpushed_commits: 2,
                conflicted: true,
                last_commit: Some("abc123".to_string()),
            },
        )
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\najax/other\n"),
    ]);

    let changed = refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert_eq!(
        runner.commands,
        vec![
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
            )
        ]
    );
    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
    assert_eq!(git_status.current_branch, None);
    assert!(!git_status.dirty);
    assert_eq!(git_status.untracked_files, 0);
    assert_eq!(git_status.unpushed_commits, 0);
    assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(task.has_side_flag(SideFlag::BranchMissing));
}

#[test]
fn refresh_git_substrate_evidence_treats_other_branch_at_expected_path_as_present() {
    let mut context = context_with_tasks();
    let task_id = TaskId::new("task-1");
    context
        .registry
        .update_git_status(
            &task_id,
            GitStatus {
                worktree_exists: false,
                branch_exists: true,
                current_branch: None,
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: Some("abc123".to_string()),
            },
        )
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /tmp/worktrees/web-fix-login\nHEAD 1111111\nbranch refs/heads/dependabot/pip/minor\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
    ]);

    let changed = refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

    assert!(changed);
    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert!(git_status.worktree_exists);
    assert!(git_status.branch_exists);
    assert_eq!(
        git_status.current_branch.as_deref(),
        Some("dependabot/pip/minor")
    );
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(!task.has_side_flag(SideFlag::BranchMissing));
}

#[test]
fn repair_plan_does_not_add_worktree_when_expected_path_is_on_another_branch() {
    let mut context = context_with_tasks();
    let task_id = TaskId::new("task-1");
    context
        .registry
        .update_git_status(
            &task_id,
            GitStatus {
                worktree_exists: false,
                branch_exists: true,
                current_branch: None,
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: Some("abc123".to_string()),
            },
        )
        .unwrap();
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /tmp/worktrees/web-fix-login\nHEAD 1111111\nbranch refs/heads/dependabot/pip/minor\n\n",
        ),
        output(0, "main\najax/fix-login\n"),
    ]);

    refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&task_id).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert_eq!(
        git_status.current_branch.as_deref(),
        Some("dependabot/pip/minor")
    );

    let plan = task_window_repair_plan(&context, "web/fix-login").unwrap();

    assert!(!plan
        .blocked_reasons
        .iter()
        .any(|reason| reason.contains("occupied")));
    assert!(!plan.commands.iter().any(is_git_worktree_add_command));
}

#[test]
fn refresh_git_substrate_evidence_ignores_empty_worktree_listing() {
    let mut context = context_with_tasks();
    let task_id = TaskId::new("task-1");
    let cached_status = GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 2,
        behind: 0,
        merged: false,
        untracked_files: 1,
        unpushed_commits: 2,
        conflicted: true,
        last_commit: Some("abc123".to_string()),
    };
    context
        .registry
        .update_git_status(&task_id, cached_status.clone())
        .unwrap();
    let mut runner = QueuedRunner::new(vec![output(0, ""), output(0, "main\n")]);

    let changed = refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

    assert!(!changed);
    assert_eq!(
        context.registry.get_task(&task_id).unwrap().git_status,
        Some(cached_status)
    );
}

#[test]
fn new_task_plan_validates_repo_and_builds_native_lifecycle() {
    let context = context_with_tasks();

    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "fix logout".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap();

    assert!(!plan.requires_confirmation);
    let git = GitAdapter::new("git");
    assert_eq!(plan.commands.len(), 4);
    assert_eq!(
        plan.commands[0],
        git.fetch_origin_branch("/Users/matt/projects/web", "main")
    );
    assert_eq!(
        plan.commands[1],
        CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "add",
                "-b",
                "ajax/fix-logout",
                "/Users/matt/projects/web__worktrees/ajax-fix-logout",
                "origin/main"
            ]
        )
    );
    assert_eq!(
        plan.commands[2],
        CommandSpec::new(
            "tmux",
            [
                "new-session",
                "-d",
                "-s",
                "ajax-web-fix-logout",
                "-n",
                "task",
                "-c",
                "/Users/matt/projects/web__worktrees/ajax-fix-logout"
            ]
        )
    );
    assert_eq!(plan.commands[3].program, "tmux");
    assert_eq!(plan.commands[3].args[0], "send-keys");
    assert_eq!(plan.commands[3].args[2], "ajax-web-fix-logout:task");
    assert_eq!(
        plan.commands[3].args[3],
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-logout --state-root .cache/ajax/agent-runtime -- codex --cd /Users/matt/projects/web__worktrees/ajax-fix-logout"
    );
}

#[test]
fn new_task_plan_preserves_paths_with_spaces_as_command_arguments() {
    let context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new(
                "web",
                "/Users/matt/projects/web app",
                "main",
            )],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );

    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "fix login".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap();

    assert_eq!(plan.commands.len(), 4);
    assert_eq!(plan.commands[0].args[1], "/Users/matt/projects/web app");
    assert_eq!(plan.commands[1].args[1], "/Users/matt/projects/web app");
    assert_eq!(
        plan.commands[1].args[6],
        "/Users/matt/projects/web app__worktrees/ajax-fix-login"
    );
    assert_eq!(plan.commands[2].args[3], "ajax-web-fix-login");
    assert_eq!(
        plan.commands[2].args[7],
        "/Users/matt/projects/web app__worktrees/ajax-fix-login"
    );
    let launch_words = shell_words(&plan.commands[3].args[3]);
    assert_eq!(
        &launch_words[launch_words.len() - 3..],
        &[
            "codex".to_string(),
            "--cd".to_string(),
            "/Users/matt/projects/web app__worktrees/ajax-fix-login".to_string(),
        ]
    );
}

#[test]
fn new_task_plan_rejects_unknown_repo() {
    let context = context_with_tasks();

    let error = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "missing".to_string(),
            title: "fix login".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap_err();

    assert_eq!(error, CommandError::RepoNotFound("missing".to_string()));
}

#[test]
fn new_task_plan_slugifies_title_into_branch_session_and_handle() {
    let context = context_with_tasks();

    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "api".to_string(),
            title: "Ship oauth v2!".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap();
    assert_eq!(plan.title, "create task: Ship oauth v2!");
    let worktree_command = plan
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("worktree add command");
    let send_keys = plan
        .commands
        .iter()
        .find(|command| is_agent_send_keys_command(command))
        .expect("agent send-keys command");
    assert_eq!(plan.commands.len(), 4);
    assert_eq!(worktree_command.args[5], "ajax/ship-oauth-v2");
    assert_eq!(
        worktree_command.args[6],
        "/Users/matt/projects/api__worktrees/ajax-ship-oauth-v2"
    );
    assert_eq!(plan.commands[2].args[3], "ajax-api-ship-oauth-v2");
    assert_eq!(send_keys.args[2], "ajax-api-ship-oauth-v2:task");
    let launch_words = shell_words(&send_keys.args[3]);
    assert_eq!(
        &launch_words[launch_words.len() - 3..],
        &[
            "codex".to_string(),
            "--cd".to_string(),
            "/Users/matt/projects/api__worktrees/ajax-ship-oauth-v2".to_string(),
        ]
    );
}

#[test]
fn new_task_plan_allows_reusing_removed_task_handle() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;
    let removed_duplicate = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap();

    let removed_worktree = removed_duplicate
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("worktree add command");
    let removed_session = removed_duplicate
        .commands
        .iter()
        .find(|command| is_task_window_new_session_command(command))
        .expect("tmux session command");
    assert_eq!(removed_worktree.args[5], "ajax/fix-login");
    assert_eq!(removed_session.args[3], "ajax-web-fix-login");
}

#[test]
fn new_task_request_creates_provisional_task_record() {
    let context = context_with_tasks();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login!".to_string(),
        agent: "codex".to_string(),
        orchestration_chat: false,
    };

    let task = task_from_new_request(&context, &request).unwrap();

    assert_eq!(task.id.as_str(), "web/fix-login");
    assert_eq!(task.handle, "fix-login");
    assert_eq!(task.branch, "ajax/fix-login");
    assert_eq!(task.tmux_session, "ajax-web-fix-login");
    assert_eq!(
        task.worktree_path.to_string_lossy(),
        "/Users/matt/projects/web__worktrees/ajax-fix-login"
    );
    assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
    assert_eq!(task.selected_agent, AgentClient::Codex);
}

#[test]
fn new_task_request_slugifies_blank_titles_to_task() {
    let context = context_with_tasks();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "!!!".to_string(),
        agent: "claude".to_string(),
        orchestration_chat: false,
    };

    let task = task_from_new_request(&context, &request).unwrap();

    assert_eq!(task.handle, "task");
    assert_eq!(task.selected_agent, AgentClient::Claude);
}

#[test]
fn record_new_task_adds_provisional_task_to_registry() {
    let mut context = context_with_tasks();
    let request = NewTaskRequest {
        repo: "api".to_string(),
        title: "Add cache".to_string(),
        agent: "codex".to_string(),
        orchestration_chat: false,
    };

    let task = record_new_task(&mut context, &request).unwrap();

    assert_eq!(task.qualified_handle(), "api/add-cache");
    assert!(context
        .registry
        .list_tasks()
        .iter()
        .any(|task| task.qualified_handle() == "api/add-cache"));
}

#[test]
fn record_new_task_reuses_removed_task_handle() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let mut removed = task_from_new_request(
        &context,
        &NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
            orchestration_chat: false,
        },
    )
    .unwrap();
    removed.lifecycle_status = LifecycleStatus::Removed;
    context.registry.create_task(removed).unwrap();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login!".to_string(),
        agent: "codex".to_string(),
        orchestration_chat: false,
    };

    let task = record_new_task(&mut context, &request).unwrap();

    assert_eq!(task.qualified_handle(), "web/fix-login");
    assert_eq!(context.registry.list_tasks().len(), 1);
    assert_eq!(
        context.registry.list_tasks()[0].lifecycle_status,
        LifecycleStatus::Provisioning
    );
}

#[test]
fn new_task_provisioning_state_updates_live_in_core() {
    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        orchestration_chat: false,
    };
    let task = record_new_task(&mut context, &request).unwrap();
    let task_id = task.id.clone();

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::WorktreeCreated,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists && status.branch_exists));
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(!task.has_side_flag(SideFlag::BranchMissing));

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::TaskSessionCreated,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(
        task.tmux_status,
        Some(TmuxStatus::present("ajax-web-fix-login"))
    );
    assert_eq!(
        task.task_window_status,
        Some(TaskWindowStatus::present(
            "task",
            "/Users/matt/projects/web__worktrees/ajax-fix-login"
        ))
    );
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::AgentCommandSent,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert_eq!(task.agent_attempts.len(), 1);
    assert_eq!(task.agent_attempts[0].agent, AgentClient::Codex);
    assert_eq!(
        task.agent_attempts[0].launch_target,
        "/Users/matt/projects/web__worktrees/ajax-fix-login"
    );
    assert!(task.has_side_flag(SideFlag::AgentRunning));

    let mut failing_context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let failing_task = record_new_task(&mut failing_context, &request).unwrap();
    mark_new_task_provisioning_failed(&mut failing_context, &failing_task.id).unwrap();
    let failing_task = failing_context.registry.get_task(&failing_task.id).unwrap();
    assert_eq!(failing_task.lifecycle_status, LifecycleStatus::Error);
    assert!(failing_task.has_side_flag(SideFlag::NeedsInput));
}

#[test]
fn open_task_plan_targets_task_directly() {
    let context = context_with_tasks();

    let outside_tmux = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
    let inside_tmux = open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

    assert_eq!(
        outside_tmux.commands,
        vec![
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
    assert_eq!(
        inside_tmux.commands,
        vec![
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"]),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        ]
    );
}
