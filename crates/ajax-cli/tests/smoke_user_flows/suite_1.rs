
#[test]
fn smoke_first_run_health_check() {
    let sandbox = SmokeSandbox::new("first-run-health-check");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    let doctor = assert_json(&sandbox.ajax(["doctor", "--json"]), "ajax doctor --json");
    assert!(doctor["checks"]
        .as_array()
        .expect("doctor checks should be an array")
        .iter()
        .all(|check| check["ok"].as_bool() == Some(true)));
    assert!(doctor["checks"]
        .as_array()
        .expect("doctor checks should be an array")
        .iter()
        .any(|check| check["name"] == "state:path" && check["ok"].as_bool() == Some(true)));

    let repos = assert_json(&sandbox.ajax(["repos", "--json"]), "ajax repos --json");
    assert_eq!(repos["repos"][0]["name"], "web");
    assert_eq!(
        repos["repos"][0]["path"],
        repo_path(&sandbox.root, "web").display().to_string()
    );

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));

    let status = assert_json(&sandbox.ajax(["status", "--json"]), "ajax status --json");
    assert_eq!(status["tasks"], Value::Array(vec![]));
}

#[test]
fn smoke_new_plan_has_no_side_effects() {
    let sandbox = SmokeSandbox::new("new-plan-no-side-effects");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    let plan = assert_json(
        &sandbox.ajax([
            "start",
            "--repo",
            "web",
            "--title",
            "fix login",
            "--agent",
            "codex",
            "--json",
        ]),
        "ajax start --json",
    );

    assert_eq!(plan["title"], "create task: fix login");
    assert_eq!(plan["requires_confirmation"], false);
    assert_eq!(plan["blocked_reasons"], Value::Array(vec![]));
    assert!(plan["commands"]
        .as_array()
        .expect("plan commands should be an array")
        .iter()
        .any(|command| command["program"] == "git"
            && command["args"]
                .as_array()
                .is_some_and(|args| args.iter().any(|arg| arg == "worktree"))));

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));
    let command_log = sandbox.command_log();
    assert!(
        command_log
            .lines()
            .filter(|line| !line.is_empty())
            .all(|line| line.contains("show-ref --verify --quiet")),
        "plan-only new may probe branch existence, but must not mutate substrate:\n{command_log}"
    );
    assert!(
        !sandbox.expected_worktree_path("web", "fix-login").exists(),
        "plan-only new must not create a worktree"
    );
    assert!(
        !sandbox.state_file.exists(),
        "plan-only new should not create durable state"
    );
}

#[test]
fn smoke_new_execute_creates_active_task_environment() {
    let sandbox = SmokeSandbox::new("new-execute-active");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = sandbox.expected_worktree_path("web", "fix-login");

    create_active_web_task(&sandbox);

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Active");
    assert_eq!(tasks["tasks"][0]["needs_attention"], false);

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["qualified_handle"], "web/fix-login");
    assert_eq!(inspect["task"]["lifecycle_status"], "Active");
    assert_eq!(inspect["branch"], "ajax/fix-login");
    assert_eq!(inspect["tmux_session"], "ajax-web-fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());
    assert!(inspect["worktree_path"]
        .as_str()
        .expect("worktree path should be a string")
        .contains("/fix-login"));

    let log = sandbox.command_log();
    assert!(
        log.contains(&format!("git -C {} fetch origin main", repo.display())),
        "fake git log should sync default branch before worktree add:\n{log}"
    );
    assert!(!log.contains("main:main"));
    assert!(
        log.contains(&format!(
            "git -C {} worktree add -b ajax/fix-login {} origin/main",
            repo.display(),
            worktree.display()
        )),
        "fake git log should include worktree add:\n{log}"
    );
    assert!(
        log.contains(&format!(
            "tmux new-session -d -s ajax-web-fix-login -n task -c {}",
            worktree.display()
        )),
        "fake tmux log should include session creation:\n{log}"
    );
    assert!(
        log.contains(&format!(
            "tmux send-keys -t ajax-web-fix-login:task (if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi) >/dev/null 2>&1; ajax-cli __agent-runtime --task-id web/fix-login --state-root {} -- codex --cd {} Enter",
            sandbox.root.join("cache/agent-runtime").display(),
            worktree.display(),
        )),
        "fake tmux log should include agent launch:\n{log}"
    );
}

#[test]
fn smoke_open_and_trunk_are_idempotent_repairs() {
    let sandbox = SmokeSandbox::new("open-trunk-idempotent");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = sandbox.expected_worktree_path("web", "fix-login");
    create_active_web_task(&sandbox);

    for command in [
        ["resume", "web/fix-login", "--execute"],
        ["repair", "web/fix-login", "--execute"],
        ["resume", "web/fix-login", "--execute"],
        ["repair", "web/fix-login", "--execute"],
    ] {
        let output = sandbox.ajax(command);
        assert_success(&output, &format!("ajax {}", command.join(" ")));
    }

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["tmux_session"], "ajax-web-fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());

    let log = sandbox.command_log();
    assert!(
        log.matches("tmux select-window -t ajax-web-fix-login:task")
            .count()
            >= 3,
        "open should select the task window each time:\n{log}"
    );
    assert!(
        log.matches("tmux select-window -t ajax-web-fix-login:task")
            .count()
            >= 5,
        "open and trunk should idempotently target the task window:\n{log}"
    );
    assert!(
        log.contains("tmux attach-session -t ajax-web-fix-login")
            || log.contains("tmux switch-client -t ajax-web-fix-login"),
        "open should attach or switch to the expected session:\n{log}"
    );
}

#[test]
fn smoke_cockpit_ctrl_q_detaches_task_session_and_returns_to_cockpit() {
    let sandbox = SmokeSandbox::new("cockpit-ctrl-q-task-session");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);

    let output = run_ajax_cockpit_ctrl_q_flow(&sandbox);

    assert!(
        matches!(output.status, WaitStatus::Exited(_, 0)),
        "ajax cockpit should exit cleanly after returning from task mode\nstatus: {:?}\nstdout:\n{}",
        output.status,
        output.stdout
    );
    assert!(
        sandbox.tmux_session_path("ajax-web-fix-login").exists(),
        "Ctrl-Q should detach from the attach client without deleting the tmux session"
    );
    assert!(
        output.stdout.contains("attached ajax-web-fix-login"),
        "task session should start the tmux attach client:\n{}",
        output.stdout
    );

    let command_log = sandbox.command_log();
    assert!(
        !command_log.contains("tmux kill-session -t ajax-web-fix-login"),
        "Ctrl-Q detach should not tear down the durable tmux session:\n{command_log}"
    );

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["lifecycle_status"], "Active");
    assert_eq!(inspect["tmux_session"], "ajax-web-fix-login");
}

#[test]
fn smoke_cockpit_reattaches_after_interrupted_attach_client() {
    let sandbox = SmokeSandbox::new("cockpit-reattach-interrupted-task-session");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);

    let output = run_ajax_cockpit_ctrl_q_flow_with_env(
        &sandbox,
        &[("AJAX_SMOKE_TMUX_ATTACH_EINTR_ONCE", "1")],
    );

    assert!(
        matches!(output.status, WaitStatus::Exited(_, 0)),
        "ajax cockpit should exit cleanly after reattaching and returning from task mode\nstatus: {:?}\nstdout:\n{}",
        output.status,
        output.stdout
    );
    let eintr = "tmux: EINTR service interrupted call";
    let attached = "attached ajax-web-fix-login";
    let eintr_at = output.stdout.find(eintr).unwrap_or_else(|| {
        panic!(
            "first attach should expose interrupted call:\n{}",
            output.stdout
        )
    });
    let attached_at = output.stdout.find(attached).unwrap_or_else(|| {
        panic!(
            "second attach should keep the operator inside the task session:\n{}",
            output.stdout
        )
    });
    assert!(
        eintr_at < attached_at,
        "EINTR notice should precede successful attach:\n{}",
        output.stdout
    );

    let command_log = sandbox.command_log();
    assert!(
        command_log
            .matches("tmux attach-session -t ajax-web-fix-login")
            .count()
            >= 2,
        "interrupted attach should be followed by a second attach:\n{command_log}"
    );
}

#[test]
fn smoke_supervise_completion_makes_task_reviewable() {
    let sandbox = SmokeSandbox::new("supervise-reviewable");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);

    let supervise = sandbox.ajax([
        "supervise",
        "--task",
        "web/fix-login",
        "--prompt",
        "finish task",
        "--json",
    ]);
    assert_success(&supervise, "ajax supervise --task --json");
    assert_eq!(stderr(&supervise), "");
    let events = stdout(&supervise)
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("supervise event should be JSON: {error}: {line}"))
        })
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|event| event["Agent"]["Started"]["agent"] == "codex"));
    assert!(events
        .iter()
        .any(|event| event["Agent"] == "Completed" || event["Agent"]["Completed"].is_object()));

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Reviewable");

    let review = assert_json(&sandbox.ajax(["ready", "--json"]), "ajax ready --json");
    assert_eq!(review["tasks"][0]["qualified_handle"], "web/fix-login");

    let next = assert_json(&sandbox.ajax(["next", "--json"]), "ajax next --json");
    assert_eq!(next["item"]["task_handle"], "web/fix-login");

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    assert!(inbox["items"]
        .as_array()
        .expect("inbox items should be an array")
        .iter()
        .any(|item| item["task_handle"] == "web/fix-login"));

    let log = sandbox.command_log();
    assert!(
        log.contains("codex"),
        "fake codex should be launched by supervise:\n{log}"
    );
}

#[test]
fn smoke_merge_and_clean_completed_task() {
    let sandbox = SmokeSandbox::new("merge-clean");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = sandbox.expected_worktree_path("web", "fix-login");
    complete_web_task_to_reviewable(&sandbox);

    let check = sandbox.ajax(["repair", "web/fix-login", "--execute"]);
    assert_success(&check, "ajax repair --execute");
    assert!(
        sandbox.command_log().contains("checked"),
        "check should run the configured test command"
    );

    let diff = sandbox.ajax(["review", "web/fix-login", "--execute"]);
    assert_success(&diff, "ajax review --execute");
    assert!(
        stdout(&diff).contains("smoke.rs | 1 +"),
        "diff should render fake git diff output:\n{}",
        stdout(&diff)
    );

    let merge_plan = assert_json(
        &sandbox.ajax(["ship", "web/fix-login", "--json"]),
        "ajax ship --json",
    );
    assert_eq!(merge_plan["title"], "merge task: web/fix-login");
    assert!(merge_plan["commands"]
        .as_array()
        .expect("merge plan commands should be an array")
        .iter()
        .any(|command| command["program"] == "git"));
    let log_before_merge = sandbox.command_log();

    let merge = sandbox.ajax(["ship", "web/fix-login", "--execute", "--yes"]);
    assert_success(&merge, "ajax ship --execute --yes");
    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Merged");

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["lifecycle_status"], "Merged");
    let log_after_merge = sandbox.command_log();
    assert_eq!(
        log_before_merge.matches("git -C").count() + 3,
        log_after_merge.matches("git -C").count(),
        "merge execution should add status, switch, and merge git calls only after execute"
    );
    assert!(log_after_merge.contains(&format!("git -C {} switch main", repo.display())));
    assert!(log_after_merge.contains(&format!(
        "git -C {} merge --ff-only ajax/fix-login",
        repo.display()
    )));

    let clean_plan = sandbox.ajax(["drop", "web/fix-login"]);
    assert_success(&clean_plan, "ajax drop plan");
    assert!(
        stdout(&clean_plan).contains("clean task: web/fix-login"),
        "clean should return a cleanup plan before execution"
    );
    let log_before_clean = sandbox.command_log();

    let clean = sandbox.ajax(["drop", "web/fix-login", "--execute", "--yes"]);
    assert_success(&clean, "ajax drop --execute --yes");
    let log_after_clean = sandbox.command_log();
    assert_ne!(
        log_before_clean, log_after_clean,
        "confirmed clean should run external cleanup commands"
    );
    assert!(
        log_after_clean.contains("tmux kill-session -t ajax-web-fix-login"),
        "clean should kill the task session:\n{log_after_clean}"
    );
    assert!(
        log_after_clean.contains(&format!(
            "git -C {} worktree remove {}",
            repo.display(),
            worktree.display()
        )) || log_after_clean.contains(&format!(
            "git -C {} worktree remove --force {}",
            repo.display(),
            worktree.display()
        )) || log_after_clean.contains("ajax-fast-worktree-remove"),
        "clean should remove the worktree:\n{log_after_clean}"
    );
    assert!(
        log_after_clean.contains(&format!(
            "git -C {} branch -d ajax/fix-login",
            repo.display()
        )) || log_after_clean.contains(&format!(
            "git -C {} branch -D ajax/fix-login",
            repo.display()
        )),
        "clean should delete the merged task branch:\n{log_after_clean}"
    );

    let tasks_after_clean = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks_after_clean["tasks"], Value::Array(vec![]));
    let cockpit = assert_json(&sandbox.ajax(["cockpit", "--json"]), "ajax cockpit --json");
    assert_eq!(cockpit["summary"]["active_tasks"], 0);
    assert_eq!(cockpit["tasks"]["tasks"], Value::Array(vec![]));
}

#[test]
fn smoke_partial_new_failure_remains_visible_and_recoverable() {
    let sandbox = SmokeSandbox::new("partial-new-failure");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = sandbox.expected_worktree_path("web", "fix-login");

    let failed = sandbox.ajax_with_env(
        [
            "start",
            "--repo",
            "web",
            "--title",
            "fix login",
            "--agent",
            "codex",
            "--execute",
        ],
        [("AJAX_SMOKE_FAIL_AFTER_WORKTREE", "1")],
    );
    assert!(
        !failed.status.success(),
        "ajax start should fail when tmux provisioning fails"
    );
    assert!(
        stderr(&failed).contains("simulated tmux startup failure"),
        "failure should preserve tmux stderr:\n{}",
        stderr(&failed)
    );

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Error");
    assert_eq!(tasks["tasks"][0]["needs_attention"], true);

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    assert!(inbox["items"]
        .as_array()
        .expect("inbox items should be an array")
        .iter()
        .any(|item| item["task_handle"] == "web/fix-login"));

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["lifecycle_status"], "Error");
    assert_eq!(inspect["branch"], "ajax/fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());

    let log = sandbox.command_log();
    assert!(log.contains(&format!("git -C {} fetch origin main", repo.display())));
    assert!(log.contains(&format!(
        "git -C {} worktree add -b ajax/fix-login {} origin/main",
        repo.display(),
        worktree.display()
    )));
    assert!(log.contains(&format!(
        "tmux new-session -d -s ajax-web-fix-login -n task -c {}",
        worktree.display()
    )));
    assert!(
        !log.contains("tmux send-keys -t ajax-web-fix-login:task"),
        "agent launch should not run after tmux session creation fails:\n{log}"
    );
}

#[test]
fn smoke_state_export_writes_json_and_refuses_overwrite() {
    let sandbox = SmokeSandbox::new("state-export");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);
    let backup = sandbox.root.join("ajax-state-backup.json");

    let export = sandbox.ajax([
        "state",
        "export",
        "--output",
        backup.to_str().expect("backup path should be UTF-8"),
    ]);
    assert_success(&export, "ajax state export");
    let exported =
        fs::read_to_string(&backup).expect("state export should create a readable backup file");
    assert!(
        !exported.is_empty(),
        "state export should create a non-empty JSON file"
    );
    let snapshot: Value =
        serde_json::from_str(&exported).expect("state export file should parse as JSON");
    assert_eq!(snapshot["repos"][0]["name"], "web");
    assert_eq!(snapshot["tasks"][0]["repo"], "web");
    assert_eq!(snapshot["tasks"][0]["handle"], "fix-login");
    assert_eq!(snapshot["metadata"]["repo_count"], 1);
    assert_eq!(snapshot["metadata"]["task_count"], 1);
    assert!(snapshot["metadata"]["event_count"]
        .as_u64()
        .is_some_and(|count| count > 0));

    let duplicate = sandbox.ajax([
        "state",
        "export",
        "--output",
        backup.to_str().expect("backup path should be UTF-8"),
    ]);
    assert!(
        !duplicate.status.success(),
        "duplicate state export should fail rather than overwrite"
    );
    assert!(
        stderr(&duplicate).contains("state export target already exists"),
        "duplicate export failure should explain the existing target:\n{}",
        stderr(&duplicate)
    );
}

#[test]
fn smoke_cockpit_tracks_cli_workflow() {
    let sandbox = SmokeSandbox::new("cockpit-parity");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    assert_cockpit_matches_tasks(&sandbox, None);

    create_active_web_task(&sandbox);
    assert_cockpit_matches_tasks(&sandbox, Some("Active"));

    let supervise = sandbox.ajax([
        "supervise",
        "--task",
        "web/fix-login",
        "--prompt",
        "finish task",
        "--json",
    ]);
    assert_success(&supervise, "ajax supervise --task --json");
    assert_cockpit_matches_tasks(&sandbox, Some("Reviewable"));

    let merge = sandbox.ajax(["ship", "web/fix-login", "--execute", "--yes"]);
    assert_success(&merge, "ajax ship --execute --yes");
    assert_cockpit_matches_tasks(&sandbox, Some("Merged"));

    let clean = sandbox.ajax(["drop", "web/fix-login", "--execute", "--yes"]);
    assert_success(&clean, "ajax drop --execute --yes");
    assert_cockpit_matches_tasks(&sandbox, None);
}

#[test]
fn smoke_multi_repo_attention_routing() {
    let sandbox = SmokeSandbox::new("multi-repo-attention");
    sandbox.create_repo("web");
    sandbox.create_repo("api");
    sandbox.write_config(&["web", "api"]);

    create_task(&sandbox, "web", "fix login");
    create_task(&sandbox, "api", "add search");
    supervise_task(&sandbox, "api/add-search");
    create_failing_task(&sandbox, "api", "break cache");

    let all_tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(all_tasks["tasks"].as_array().unwrap().len(), 3);

    let web_tasks = assert_json(
        &sandbox.ajax(["tasks", "--repo", "web", "--json"]),
        "ajax tasks --repo web --json",
    );
    assert_eq!(web_tasks["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(web_tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(web_tasks["tasks"][0]["lifecycle_status"], "Active");

    let api_tasks = assert_json(
        &sandbox.ajax(["tasks", "--repo", "api", "--json"]),
        "ajax tasks --repo api --json",
    );
    assert_eq!(api_tasks["tasks"].as_array().unwrap().len(), 2);
    assert!(api_tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["qualified_handle"] == "api/add-search"
            && task["lifecycle_status"] == "Reviewable"));
    assert!(api_tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["qualified_handle"] == "api/break-cache"
            && task["lifecycle_status"] == "Error"));

    let review = assert_json(&sandbox.ajax(["ready", "--json"]), "ajax ready --json");
    assert_eq!(review["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(review["tasks"][0]["qualified_handle"], "api/add-search");

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    let inbox_handles = inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["task_handle"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(inbox_handles.contains(&"api/add-search"));
    assert!(inbox_handles.contains(&"api/break-cache"));

    let next = assert_json(&sandbox.ajax(["next", "--json"]), "ajax next --json");
    assert_eq!(next["item"]["task_handle"], "api/break-cache");

    let status = assert_json(&sandbox.ajax(["status", "--json"]), "ajax status --json");
    assert_eq!(status["tasks"].as_array().unwrap().len(), 3);
    let cockpit = assert_json(&sandbox.ajax(["cockpit", "--json"]), "ajax cockpit --json");
    assert_eq!(cockpit["summary"]["repos"], 2);
    assert_eq!(cockpit["summary"]["tasks"], 3);
    assert_eq!(cockpit["summary"]["reviewable_tasks"], 1);
    assert!(cockpit["inbox"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["task_handle"] == "api/break-cache"));
}

#[test]
fn smoke_rooted_orphan_recovery_stays_scoped_to_its_repo() {
    let sandbox = SmokeSandbox::new("rooted-orphan-repo-scope");
    sandbox.create_repo("web");
    sandbox.create_repo("api");
    sandbox.write_config(&["web", "api"]);

    create_active_web_task(&sandbox);
    let api_orphan = sandbox.expected_worktree_path("api", "ghost-task");
    fs::create_dir_all(&api_orphan).unwrap_or_else(|error| {
        panic!("failed to create orphan {}: {error}", api_orphan.display())
    });

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    let mut handles = tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| {
            task["qualified_handle"]
                .as_str()
                .unwrap_or_default()
                .to_string()
        })
        .collect::<Vec<_>>();
    handles.sort();

    assert_eq!(
        handles,
        vec!["api/ghost-task".to_string(), "web/fix-login".to_string(),]
    );
}

