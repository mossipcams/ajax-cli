#[test]
fn cockpit_json_refreshes_live_status_even_when_projection_is_fresh() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::now(),
            RuntimeObservationSource::TmuxProbe,
        );
    }
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let output =
        run_with_context_and_runner(["ajax", "cockpit", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        parsed["tasks"]["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn cockpit_json_watch_renders_refreshed_live_status_over_iterations() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut runner = AgentStatusMutatingRunner::new(
        watch_refresh_outputs(),
        cache_dir.clone(),
        "task-1",
        &["ask", "wait"],
    );
    let output = run_with_context_and_runner(
        [
            "ajax",
            "cockpit",
            "--json",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ],
        &mut context,
        &mut runner,
    )
    .unwrap();
    let frames: Vec<_> = output.split("\n\n").collect();
    assert_eq!(frames.len(), 2);
    let first: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(frames[1]).unwrap();
    assert_eq!(
        first["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for approval"
    );
    assert_eq!(
        second["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for input"
    );
    assert!(runner.inner.commands.len() >= 4);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn cockpit_json_watch_streams_each_refreshed_frame_to_writer() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut runner = AgentStatusMutatingRunner::new(
        watch_refresh_outputs(),
        cache_dir.clone(),
        "task-1",
        &["ask", "wait"],
    );
    let mut writer = FlushingWriter::default();
    let state_changed = run_with_context_and_runner_to_writer(
        [
            "ajax",
            "cockpit",
            "--json",
            "--watch",
            "--iterations",
            "2",
            "--interval-ms",
            "0",
        ],
        &mut context,
        &mut runner,
        &mut writer,
    )
    .unwrap();
    assert!(state_changed);
    assert_eq!(writer.flushes, 2);
    let frames: Vec<_> = writer.output.trim_end().split("\n\n").collect();
    assert_eq!(frames.len(), 2);
    let first: serde_json::Value = serde_json::from_str(frames[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(frames[1]).unwrap();
    assert_eq!(
        first["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for approval"
    );
    assert_eq!(
        second["tasks"]["tasks"][0]["live_status"]["summary"],
        "waiting for input"
    );
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn cockpit_watch_renders_refreshed_live_status_in_frame() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let output = run_with_context_and_runner(
        ["ajax", "cockpit", "--watch", "--iterations", "1"],
        &mut context,
        &mut runner,
    )
    .unwrap();
    assert_eq!(
        output
            .lines()
            .find(|line| line.starts_with("web/fix-login")),
        Some("web/fix-login\tRunning - Agent working\tFix login")
    );
    let mut expected = tmux_live_commands_with_running_reconcile();
    extend_expected_ci_monitor_commands(&mut expected);
    assert_eq!(runner.commands, expected);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn status_command_refreshes_live_state_from_tmux() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let output =
        run_with_context_and_runner(["ajax", "status"], &mut context, &mut runner).unwrap();
    assert_eq!(
        output
            .lines()
            .find(|line| line.starts_with("web/fix-login")),
        Some("web/fix-login\tWaiting - Waiting for approval\tFix login")
    );
    assert!(context
        .registry
        .get_task(&TaskId::new("task-1"))
        .unwrap()
        .has_side_flag(SideFlag::NeedsInput));
    let mut expected = tmux_live_commands();
    extend_expected_ci_monitor_commands(&mut expected);
    assert_eq!(runner.commands, expected);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn status_command_renders_json_from_refreshed_live_state() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let output =
        run_with_context_and_runner(["ajax", "status", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(
        parsed["tasks"][0]["live_status"]["summary"],
        "agent running"
    );
    let mut expected = tmux_live_commands_with_running_reconcile();
    extend_expected_ci_monitor_commands(&mut expected);
    assert_eq!(runner.commands, expected);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn read_json_commands_refresh_live_state_even_when_projection_is_fresh() {
    for command in [
        vec!["ajax", "tasks", "--json"],
        vec!["ajax", "status", "--json"],
        vec!["ajax", "cockpit", "--json"],
    ] {
        let mut context = sample_context();
        let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.runtime_projection = RuntimeProjection::new(
                RuntimeHealth::Healthy,
                SystemTime::now(),
                RuntimeObservationSource::TmuxProbe,
            );
        }
        let mut outputs = tmux_live_outputs();
        outputs.push(output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n",
        ));
        let mut runner = QueuedRunner::new(outputs);
        let output = run_with_context_and_runner(command.clone(), &mut context, &mut runner)
            .unwrap_or_else(|error| panic!("{command:?} failed: {error}"));
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        let task_json = if command[1] == "cockpit" {
            &parsed["tasks"]["tasks"]
        } else {
            &parsed["tasks"]
        };
        assert_eq!(task_json[0]["qualified_handle"], "web/fix-login");
        assert_eq!(task_json[0]["live_status"]["summary"], "agent running");
        let mut expected = tmux_live_commands_with_running_reconcile();
        extend_expected_ci_monitor_commands(&mut expected);
        assert_eq!(runner.commands, expected, "{command:?}");
        let _ = std::fs::remove_dir_all(cache_dir);
    }
}
#[test]
fn read_commands_share_live_refresh_contract() {
    for args in [
        vec!["ajax", "repos", "--json"],
        vec!["ajax", "tasks", "--json"],
        vec!["ajax", "inbox", "--json"],
        vec!["ajax", "next", "--json"],
        vec!["ajax", "ready", "--json"],
        vec!["ajax", "status", "--json"],
        vec!["ajax", "cockpit", "--json"],
    ] {
        let mut context = sample_context();
        let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
        let mut runner = QueuedRunner::new(tmux_live_outputs());
        let output = run_with_context_and_runner(args.clone(), &mut context, &mut runner)
            .unwrap_or_else(|error| panic!("{args:?} failed: {error}"));
        assert!(!output.is_empty(), "{args:?} should render a response");
        let mut expected = tmux_live_commands_with_running_reconcile();
        extend_expected_ci_monitor_commands(&mut expected);
        assert_eq!(runner.commands, expected, "{args:?}");
        let _ = std::fs::remove_dir_all(cache_dir);
    }
}
#[test]
fn read_command_skips_live_pane_probe_when_cached_runtime_is_fresh() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
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
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    let mut runner = QueuedRunner::default();
    let output = run_with_context_and_runner(["ajax", "tasks"], &mut context, &mut runner).unwrap();
    let handles: Vec<&str> = output
        .lines()
        .map(|line| line.split('\t').next().unwrap_or_default())
        .collect();
    assert_eq!(handles, vec!["web/fix-login"]);
    assert!(runner.commands.is_empty());
}
#[test]
fn read_refresh_failure_keeps_task_visible_with_missing_tmux_attention() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut outputs = git_live_outputs();
    outputs.push(output(0, "other-session\n"));
    outputs.extend(ci_monitor_live_outputs());
    let mut runner = QueuedRunner::new(outputs);
    let output =
        run_with_context_and_runner(["ajax", "tasks", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(
        parsed["tasks"][0]["live_status"]["summary"],
        "tmux session missing"
    );
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
    assert_eq!(
        runner.commands,
        vec![
            tmux_live_commands()[0].clone(),
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
            expected_ci_discovery_command(),
            expected_ci_probe_command(),
        ]
    );
}
#[test]
fn read_refresh_updates_stale_git_substrate_evidence() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: true,
        ahead: 1,
        behind: 0,
        merged: false,
        untracked_files: 1,
        unpushed_commits: 1,
        conflicted: true,
        last_commit: Some("abc123".to_string()),
    });
    let mut runner = QueuedRunner::new(vec![
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
        ),
        output(0, "main\n"),
        output(0, "origin/main\n"),
        output(0, "other-session\n"),
    ]);
    let output =
        run_with_context_and_runner(["ajax", "tasks", "--json"], &mut context, &mut runner)
            .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    let git_status = task.git_status.as_ref().unwrap();
    assert_eq!(parsed["tasks"][0]["qualified_handle"], "web/fix-login");
    assert!(!git_status.worktree_exists);
    assert!(!git_status.branch_exists);
    assert_eq!(git_status.current_branch, None);
    assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    assert!(task.has_side_flag(SideFlag::BranchMissing));
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
            ),
            tmux_live_commands()[0].clone(),
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
        ]
    );
}
#[test]
fn cockpit_refresh_snapshot_reports_refreshed_tmux_state() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "ask");
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let mut state_changed = false;
    let snapshot =
        refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();
    assert!(state_changed);
    assert_eq!(
        snapshot.cards[0].status_explanation.as_deref(),
        Some("Waiting for approval")
    );
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
    let mut expected = tmux_live_commands();
    extend_expected_ci_monitor_commands(&mut expected);
    assert_eq!(runner.commands, expected);
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn live_refresh_clears_stale_tmux_missing_when_session_exists_without_task() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TmuxMissing);
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            0,
            "ajax-web-fix-login\tagent\t/tmp/worktrees/web-fix-login\n",
        ),
    ]);
    let mut state_changed = false;
    let snapshot =
        refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(state_changed);
    assert!(!task.has_side_flag(SideFlag::TmuxMissing));
    assert!(task.has_side_flag(SideFlag::TaskWindowMissing));
    assert_eq!(snapshot.cards.len(), 1);
    assert_eq!(snapshot.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(snapshot.inbox.items.len(), 1);
    assert_eq!(snapshot.inbox.items[0].task_handle, "web/fix-login");
}
#[test]
fn live_refresh_reports_changed_when_same_status_updates_activity() {
    let mut context = sample_context();
    let cache_dir = prepare_active_task_agent_status(&mut context, "task-1", "working");
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::AgentRunning,
        "agent running",
    ));
    task.tmux_status = Some(TmuxStatus {
        exists: true,
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
    task.task_window_status = Some(TaskWindowStatus {
        exists: true,
        window_name: "task".to_string(),
        current_path: "/tmp/worktrees/web-fix-login".into(),
        points_at_expected_path: true,
    });
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    task.last_activity_at = SystemTime::UNIX_EPOCH;
    let previous_activity = task.last_activity_at;
    let mut runner = QueuedRunner::new(tmux_live_outputs());
    let mut state_changed = false;
    let _snapshot =
        refresh_cockpit_snapshot(&mut context, &mut runner, &mut state_changed, &mut None)
            .unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(state_changed);
    assert!(task.last_activity_at > previous_activity);
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| status.summary.as_str()),
        Some("agent running")
    );
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn live_refresh_records_nonzero_session_listing_as_probe_failure() {
    let mut context = sample_context();
    let mut runner = QueuedRunner::new(vec![output(1, "ajax-web-fix-login\n")]);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    assert!(task.tmux_status.is_none());
    assert!(task.task_window_status.is_none());
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-sessions probe failed: exited with status 1")
    );
    assert_eq!(runner.commands, vec![tmux_live_commands()[0].clone()]);
}
#[test]
fn live_refresh_skips_cleanable_tasks_without_tmux_probe() {
    let mut context = cleanable_context();
    let mut runner = QueuedRunner::new(Vec::new());
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    assert!(!changed);
    assert!(runner.commands.is_empty());
}
#[test]
fn live_refresh_lists_tmux_windows_once_for_multiple_active_tasks() {
    let mut context = two_active_tasks_context();
    let cache_dir = test_agent_cache_directory("two-active");
    context.runtime_paths.cache_dir = cache_dir.clone();
    write_agent_status_event(&cache_dir, "task-1", "working");
    write_agent_status_event(&cache_dir, "task-2", "ask");
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\najax-web-fix-sidebar\n"),
        output(
            0,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\najax-web-fix-sidebar\ttask\t/tmp/worktrees/web-fix-sidebar\n",
        ),
        output(0, "codex is working\n"),
        output(
            0,
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\nworktree /tmp/worktrees/web-fix-sidebar\nHEAD 3333333\nbranch refs/heads/ajax/fix-sidebar\n\n",
        ),
        ci_pr_list_output(),
        ci_pr_checks_output(),
        ci_pr_list_output_for(43, "ajax/fix-sidebar", "3333333"),
        ci_pr_checks_output(),
    ]);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    assert!(changed);
    assert_eq!(
        runner.commands,
        vec![
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-a",
                    "-F",
                    "#{session_name}\t#{window_name}\t#{pane_current_path}",
                ],
            )
            .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "tmux",
                ["capture-pane", "-p", "-t", "ajax-web-fix-login:task"],
            )
            .with_timeout(std::time::Duration::from_secs(8)),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "list",
                    "--porcelain",
                ],
            ),
            expected_ci_discovery_command(),
            expected_ci_probe_command(),
            expected_ci_discovery_command_for_branch(
                "/tmp/worktrees/web-fix-sidebar",
                "ajax/fix-sidebar",
            ),
            expected_ci_probe_command_for_pr("/tmp/worktrees/web-fix-sidebar", 43),
        ]
    );
    let _ = std::fs::remove_dir_all(cache_dir);
}
#[test]
fn live_refresh_nonzero_window_listing_preserves_evidence_and_stops_before_pane_capture() {
    let mut context = sample_context();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    let mut runner = QueuedRunner::new(vec![
        output(0, "ajax-web-fix-login\n"),
        output(
            1,
            "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        ),
        ci_pr_list_output(),
        ci_pr_checks_output(),
    ]);
    let changed = crate::cockpit_backend::refresh_live_context(&mut context, &mut runner).unwrap();
    let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
    assert!(changed);
    let expected_commands = tmux_live_commands();
    assert_eq!(
        runner.commands,
        vec![
            expected_commands[0].clone(),
            expected_commands[1].clone(),
            expected_ci_discovery_command(),
            expected_ci_probe_command(),
        ]
    );
    assert!(!task.has_side_flag(SideFlag::TaskWindowMissing));
    assert!(task.task_window_status.is_none());
    assert_eq!(
        task.runtime_projection.observation_error.as_deref(),
        Some("tmux list-windows probe failed: exited with status 1")
    );
}
