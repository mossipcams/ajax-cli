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
            CommandSpec::new("tmux", ["select-window", "-t", "=ajax-web-fix-login:=task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "=ajax-web-fix-login"])
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
        vec![
            CommandSpec::new("git", ["diff", "--stat", "origin/main...HEAD"])
                .with_cwd("/tmp/worktrees/web-fix-login")
        ]
    );
}

#[test]
fn diff_use_case_module_summarizes_branch_diff_in_task_worktree() {
    let context = context_with_tasks();

    let plan = diff::diff_task_plan(&context, "web/fix-login").unwrap();

    assert_eq!(plan.title, "diff task: web/fix-login");
    assert_eq!(
        plan.commands,
        vec![
            CommandSpec::new("git", ["diff", "--stat", "origin/main...HEAD"])
                .with_cwd("/tmp/worktrees/web-fix-login")
        ]
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

struct TempRepoRoot(std::path::PathBuf);

impl TempRepoRoot {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempRepoRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git failed to spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_planned(spec: &CommandSpec) -> String {
    let output = std::process::Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(
            spec.cwd
                .as_deref()
                .expect("planned diff must run in a worktree"),
        )
        .output()
        .expect("planned diff failed to spawn");
    assert!(
        output.status.success(),
        "planned diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn diff_commands_ignore_upstream_commits_missing_from_local_base_1454() {
    let root = TempRepoRoot::new("ajax-1454");
    let origin = root.0.join("origin.git");
    let repo = root.0.join("repo");
    let upstream = root.0.join("upstream");
    let worktree = root.0.join("task");
    let origin_str = origin.to_str().unwrap();
    let repo_str = repo.to_str().unwrap();
    let upstream_str = upstream.to_str().unwrap();
    let worktree_str = worktree.to_str().unwrap();

    git_in(&root.0, &["init", "--bare", origin_str]);
    git_in(&root.0, &["init", repo_str]);
    std::fs::write(repo.join("README.md"), "base\n").unwrap();
    git_in(&repo, &["add", "README.md"]);
    git_in(&repo, &["commit", "-m", "base"]);
    git_in(&repo, &["remote", "add", "origin", origin_str]);
    git_in(&repo, &["push", "origin", "main"]);

    git_in(&root.0, &["clone", origin_str, upstream_str]);
    std::fs::write(upstream.join("upstream.txt"), "upstream\n").unwrap();
    git_in(&upstream, &["add", "upstream.txt"]);
    git_in(&upstream, &["commit", "-m", "upstream"]);
    git_in(&upstream, &["push", "origin", "main"]);

    git_in(&repo, &["fetch", "origin"]);
    git_in(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "ajax/fix-login",
            worktree_str,
            "origin/main",
        ],
    );
    std::fs::write(worktree.join("task.txt"), "task\n").unwrap();
    git_in(&worktree, &["add", "task.txt"]);
    git_in(&worktree, &["commit", "-m", "task"]);

    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .worktree_path = worktree.clone();

    let plan = diff_task_plan(&context, "web/fix-login").unwrap();
    let stat = run_planned(&plan.commands[0]);
    let stat_paths: Vec<&str> = stat
        .lines()
        .filter_map(|line| line.split_once(" | ").map(|(path, _)| path.trim()))
        .collect();
    assert_eq!(stat_paths, vec!["task.txt"], "{stat}");

    let github = crate::adapters::GithubChecksAdapter::new("gh");
    let review = run_planned(&github.local_diff(worktree_str, "main"));
    let review_files: Vec<&str> = review
        .lines()
        .filter(|line| line.starts_with("diff --git"))
        .collect();
    assert_eq!(
        review_files,
        vec!["diff --git a/task.txt b/task.txt"],
        "{review}"
    );
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
        vec![
            CommandSpec::new("git", ["diff", "--stat", "origin/main...HEAD"])
                .with_cwd("/tmp/worktrees/web-fix-login")
        ]
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
            CommandSpec::new("tmux", ["select-window", "-t", "=ajax-web-fix-login:=task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "=ajax-web-fix-login"])
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
            CommandSpec::new("tmux", ["select-window", "-t", "=ajax-web-fix-login:=task"]),
            CommandSpec::new("tmux", ["attach-session", "-t", "=ajax-web-fix-login"])
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
