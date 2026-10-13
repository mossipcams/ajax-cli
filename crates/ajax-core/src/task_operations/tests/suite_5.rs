use super::*;

fn run_git(args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .env_clear()
        .envs([("PATH", "/usr/bin:/bin"), ("HOME", "/tmp")])
        .output()
        .expect("git failed to spawn in test")
}

#[test]
fn drop_delete_branch_step_reports_git_stdout_diagnostics_when_deletion_is_blocked() {
    // #1260: `git branch -D` reports blocked deletions on stdout ("error: cannot
    // delete branch ... used by worktree at ...") with empty stderr. The drop's
    // delete-branch step must exit non-zero and re-emit that diagnostic on stderr;
    // before the fix the step exited 0 while the local branch survived (silent
    // partial failure) or persisted "sh exited with status 1:" without context.
    let root = std::env::temp_dir().join(format!(
        "ajax-drop-1260-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let repo_path = root.join("web");
    let blocked_checkout = root.join("blocked-checkout");
    std::fs::create_dir_all(&repo_path).unwrap();
    let repo_str = repo_path.to_str().unwrap();

    assert!(
        run_git(&["-C", repo_str, "init"]).status.success(),
        "git init failed"
    );
    assert!(
        run_git(&["-C", repo_str, "config", "user.email", "test@example.com"])
            .status
            .success()
    );
    assert!(run_git(&["-C", repo_str, "config", "user.name", "test"])
        .status
        .success());
    std::fs::write(repo_path.join("README.md"), "init\n").unwrap();
    assert!(run_git(&["-C", repo_str, "add", "README.md"])
        .status
        .success());
    assert!(
        run_git(&["-C", repo_str, "commit", "-m", "init"])
            .status
            .success(),
        "git commit failed"
    );

    // Give the repo a bare origin (no matching remote ref) so the script's push leg
    // reports "remote ref does not exist" and tolerates it; without this the push
    // fails with an unrelated fatal that masks the silent-success defect.
    let origin_path = root.join("origin.git");
    assert!(
        run_git(&["init", "--bare", origin_path.to_str().unwrap()])
            .status
            .success(),
        "git init --bare failed"
    );
    assert!(run_git(&[
        "-C",
        repo_str,
        "remote",
        "add",
        "origin",
        origin_path.to_str().unwrap()
    ])
    .status
    .success());

    // Check out the task branch in a second worktree so `git branch -D` is blocked.
    let blocked_str = blocked_checkout.to_str().unwrap();
    assert!(
        run_git(&[
            "-C",
            repo_str,
            "worktree",
            "add",
            "-b",
            "ajax/fix-login",
            blocked_str,
            "HEAD"
        ])
        .status
        .success(),
        "git worktree add failed"
    );

    let mut context = CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", repo_str, "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    context
        .registry
        .create_task(Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            blocked_str,
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        ))
        .unwrap();

    let decision =
        drop_op_execution_decision(&context, "web/fix-login", DropOp::EnsureBranchAbsent, true)
            .unwrap();
    let DropExecutionDecision::Command(command) = decision else {
        panic!("expected delete-branch command, got {decision:?}");
    };
    assert_eq!(
        command.args.get(2).map(String::as_str),
        Some("ajax-delete-branch")
    );

    let mut runner = crate::adapters::ProcessCommandRunner;
    let output = runner.run(&command).unwrap();
    let _ = std::fs::remove_dir_all(root);

    assert_eq!(
        output.status_code, 1,
        "blocked branch deletion must fail the step; stdout: {} stderr: {}",
        output.stdout, output.stderr
    );
    assert!(
        !output.stderr.trim().is_empty(),
        "blocked local branch deletion must surface git diagnostics on stderr (issue #1260); stdout was: {}",
        output.stdout
    );
    assert!(
        output.stderr.contains("cannot delete branch"),
        "expected git's 'cannot delete branch' diagnostic; stderr: {} stdout: {}",
        output.stderr,
        output.stdout
    );
}

#[test]
fn failed_delete_branch_step_persists_stdout_detail_when_stderr_is_empty() {
    // #1260: a non-zero drop step with empty stderr persisted the detail "sh exited
    // with status 1:" with nothing after the colon. Captured stdout now fills that
    // gap in the recorded drop failure metadata.
    let mut context = context_with_cleanable_task();
    let task_id = TaskId::new("web/fix-login");

    // The worktree step succeeds; the delete-branch step exits 1 carrying its
    // diagnostic only on stdout (as real git does for blocked deletions). Steps run
    // in worktree -> branch -> tmux order after the four plan-observation slots.
    let mut outputs = present_drop_observation_outputs();
    outputs.push(output(0, "", ""));
    outputs.push(output(
        1,
        "error: cannot delete branch 'ajax/fix-login' used by worktree at \"/repo/web__worktrees/ajax-fix-login\"\n",
        "",
    ));
    outputs.extend(absent_drop_observation_outputs());
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
        Some("branch_absent")
    );
    let detail = task
        .metadata
        .get("drop_failed_detail")
        .cloned()
        .expect("drop failure detail must be recorded");
    assert!(
        detail.contains("cannot delete branch"),
        "delete-branch failure detail must carry the stdout diagnostic, got: {detail:?}"
    );
}
