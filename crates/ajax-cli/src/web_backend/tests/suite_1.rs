
#[test]
fn cockpit_json_serializes_the_current_cockpit_projection() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let json = cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["repos"]["repos"], serde_json::json!([]));
    assert_eq!(value["cards"], serde_json::json!([]));
    assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    assert_eq!(value["backend"]["authority"], "host-native");
}

#[test]
fn http_router_serves_mobile_shell_and_cockpit_json() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let shell = handle_http_request("GET", "/", "", &context).unwrap();
    assert_eq!(shell.status_code, 200);
    assert_eq!(shell.content_type, "text/html; charset=utf-8");
    // ajax-web owns the shell's content; ajax-cli only proves it serves those bytes.
    assert_eq!(
        String::from_utf8_lossy(&shell.body),
        web_install::browser_shell()
    );

    let cockpit = handle_http_request("GET", "/api/cockpit", "", &context).unwrap();
    assert_eq!(cockpit.status_code, 200);
    assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
        serde_json::json!([])
    );
}

#[test]
fn http_router_serves_static_css_and_js() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let css = handle_http_request("GET", "/app.css", "", &context).unwrap();
    assert_eq!(css.status_code, 200);
    assert_eq!(css.content_type, "text/css; charset=utf-8");
    assert!(!css.body.is_empty());
    assert_eq!(
        css.body,
        web_install::static_asset("/app.css").unwrap().body
    );

    let js = handle_http_request("GET", "/app.js", "", &context).unwrap();
    assert_eq!(js.status_code, 200);
    assert_eq!(js.content_type, "text/javascript; charset=utf-8");
    assert!(!js.body.is_empty());
    assert_eq!(
        js.body,
        web_install::static_asset("/app.js").unwrap().body
    );
}

#[test]
fn http_router_does_not_serve_retired_pwa_install_assets() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    for path in [
        "/manifest.webmanifest",
        "/sw.js",
        "/icons/icon-192.png",
        "/icons/icon-512.png",
        "/icons/icon-maskable-512.png",
        "/icons/apple-touch-icon.png",
    ] {
        let response = handle_http_request("GET", path, "", &context).unwrap();
        assert_eq!(response.status_code, 404, "{path}");
        assert_eq!(response.content_type, "text/plain; charset=utf-8", "{path}");
    }
}

#[test]
fn http_router_reports_unknown_routes_and_unsupported_methods() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let missing = handle_http_request("GET", "/missing", "", &context).unwrap();
    assert_eq!(missing.status_code, 404);
    assert!(String::from_utf8_lossy(&missing.body).contains("not found"));

    let unsupported = handle_http_request("POST", "/", "", &context).unwrap();
    assert_eq!(unsupported.status_code, 405);
}

#[test]
fn action_endpoint_guards_start_for_dedicated_new_task_flow() {
    let mut context = reviewable_context();
    let mut runner = OkRunner;

    let response = handle_http_request_with_runner_and_paths(
        "POST",
        "/api/actions",
        r#"{"task_handle":"web/fix-login","action":"start"}"#,
        &mut context,
        &mut runner,
        None,
    )
    .unwrap();

    assert_eq!(response.status_code, 409);
    assert!(String::from_utf8_lossy(&response.body)
        .contains("start uses the dedicated Web Cockpit new-task operation"));
}

#[test]
fn action_endpoint_executes_non_interactive_task_actions() {
    let mut context = reviewable_context();
    let mut runner = OkRunner;

    let response = handle_http_request_with_runner_and_paths(
        "POST",
        "/api/actions",
        r#"{"task_handle":"web/fix-login","action":"review"}"#,
        &mut context,
        &mut runner,
        None,
    )
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

    assert_eq!(response.status_code, 200);
    assert_eq!(body["ok"], true);
    assert!(body["output"].is_string());
    assert_eq!(
        body["cockpit"]["cards"][0]["qualified_handle"],
        "web/fix-login"
    );
}

#[test]
fn action_endpoint_returns_json_when_underlying_command_fails() {
    #[derive(Clone)]
    struct FailingRunner;
    impl CommandRunner for FailingRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 1,
                stdout: String::new(),
                stderr: "merge failed".to_string(),
            })
        }
    }
    let mut context = reviewable_context();
    let mut runner = FailingRunner;

    let response = handle_http_request_with_runner_and_paths(
        "POST",
        "/api/actions",
        r#"{"task_handle":"web/fix-login","action":"ship"}"#,
        &mut context,
        &mut runner,
        None,
    )
    .expect("handler should return a JSON error, not propagate the CliError");
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

    assert_eq!(response.status_code, 409);
    assert_eq!(body["ok"], false);
    assert!(
        !body["error"].as_str().unwrap_or_default().is_empty(),
        "error message should be populated, got: {:?}",
        body["error"]
    );
    assert!(body["cockpit"].is_object());
}

fn write_agent_status_event(cache_dir: &std::path::Path, task_id: &str, value: &str) {
    use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};

    let events_dir = cache_dir.join("agent-events");
    let runtime_dir = cache_dir.join("agent-runtime");
    std::fs::create_dir_all(&events_dir).unwrap();
    std::fs::create_dir_all(&runtime_dir).unwrap();
    let now_millis = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let stem = task_file_stem(task_id);

    let state = match value {
        "done" => AgentRuntimeState::ExitedSuccess,
        "failed" => AgentRuntimeState::ExitedFailure,
        _ => AgentRuntimeState::Running,
    };
    let snapshot = AgentRuntimeSnapshot {
        task_id: task_id.to_string(),
        state,
        observed_at_unix_millis: now_millis,
        pid: Some(1),
        exit_code: None,
        message: None,
    };
    std::fs::write(
        runtime_dir.join(format!("{stem}.json")),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();

    let (kind, detail) = match value {
        "ask" => (
            "attention_requested",
            serde_json::json!({"attention": {"attention": "permission"}}),
        ),
        "wait" => (
            "attention_requested",
            serde_json::json!({"attention": {"attention": "question"}}),
        ),
        "done" => (
            "turn_settled",
            serde_json::json!({"outcome": {"outcome": "completed"}}),
        ),
        "failed" => (
            "turn_settled",
            serde_json::json!({"outcome": {"outcome": "failed"}}),
        ),
        _ => ("turn_started", serde_json::Value::Null),
    };
    let mut envelope = serde_json::json!({
        "schema_version": 1,
        "kind": kind,
        "received_at_unix_millis": now_millis,
        "occurred_at_unix_millis": now_millis,
    });
    if !detail.is_null() {
        envelope["detail"] = detail;
    }
    std::fs::write(
        events_dir.join(format!("{stem}.jsonl")),
        format!("{}\n", serde_json::to_string(&envelope).unwrap()),
    )
    .unwrap();
}

#[test]
fn cockpit_api_refreshes_live_task_status_before_rendering() {
    let cache_dir = std::env::temp_dir().join(format!(
        "ajax-web-api-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_agent_status_event(&cache_dir, "web/fix-login", "working");
    let mut context = reviewable_context();
    context.runtime_paths.cache_dir = cache_dir.clone();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    let mut runner = LiveRefreshRunner;

    let response = handle_http_request_with_runner_and_paths(
        "GET",
        "/api/cockpit",
        "",
        &mut context,
        &mut runner,
        None,
    )
    .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

    assert_eq!(response.status_code, 200);
    assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(body["cards"][0]["status"], "running");
    assert_eq!(body["cards"][0]["status_explanation"], "Agent working");
    assert!(body["cards"][0]["actions"].is_array());
    for legacy in ["ui_state", "status_label", "live_summary", "action_states"] {
        assert!(
            body["cards"][0].get(legacy).is_none(),
            "legacy field {legacy}"
        );
    }
    let _ = std::fs::remove_dir_all(cache_dir);
}

#[test]
fn cockpit_api_reloads_task_state_from_disk_before_rendering() {
    let root = std::env::temp_dir().join(format!("ajax-web-reload-{}", std::process::id()));
    let mut paths =
        CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    paths.runtime_paths.cache_dir = root.join("cache");
    write_agent_status_event(&paths.runtime_paths.cache_dir, "web/fix-login", "working");
    let mut saved_context = reviewable_context();
    let task = saved_context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    SqliteRegistryStore::new(&paths.state_file)
        .save(&saved_context.registry)
        .unwrap();
    let server_context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let runner = LiveRefreshRunner;
    let bridge = CliRuntimeBridge {
        paths: Some(paths.clone()),
        last_loaded_mtime: None,
        save_state: crate::context::tracked_save_state(&paths, &server_context.registry)
            .unwrap(),
    };
    let state =
        runtime::WebAppState::new(server_context.clone(), runner.clone(), bridge, root.clone());
    let app = runtime::axum_app(state);

    let response = axum_response_to_http_response(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                app.oneshot(
                    AxumRequest::builder()
                        .uri("/api/cockpit")
                        .header("cookie", "ajax_browser_session=ajax-test-browser-session")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
            }),
    );
    let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

    assert_eq!(response.status_code, 200);
    assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(body["cards"][0]["status"], "running");
    assert_eq!(body["cards"][0]["status_explanation"], "Agent working");

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn web_refresh_cockpit_notify_respects_deliver_flag() {
    use ajax_core::config::NotifyConfig;
    use ajax_core::lifecycle::mark_active;
    use ajax_core::models::{AgentClient, SideFlag, Task, TaskId};
    use ajax_core::registry::Registry;

    #[derive(Clone)]
    struct RecordingRunner {
        specs: std::sync::Arc<std::sync::Mutex<Vec<CommandSpec>>>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.specs.lock().unwrap().push(command.clone());
            Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    let mut task = Task::new(
        TaskId::new("web/notify"),
        "web",
        "notify",
        "Notify",
        "ajax/notify",
        "main",
        "/tmp/worktrees/web-notify",
        "ajax-web-notify",
        "task",
        AgentClient::Codex,
    );
    mark_active(&mut task).unwrap();
    task.add_side_flag(SideFlag::NeedsInput);
    let mut registry = InMemoryRegistry::default();
    registry.create_task(task).unwrap();
    let mut context = CommandContext::new(
        Config {
            notify: Some(NotifyConfig {
                webhook_url: "https://ntfy.sh/topic".to_string(),
                poll_seconds: None,
            }),
            ..Config::default()
        },
        registry,
    );
    let mut runner = RecordingRunner {
        specs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let specs = runner.specs.clone();
    let mut bridge = CliRuntimeBridge::for_context(None, &context).unwrap();
    let curl_count = || {
        specs
            .lock()
            .unwrap()
            .iter()
            .filter(|spec| spec.program == "curl")
            .count()
    };

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, false)
        .expect("refresh without notify");
    assert_eq!(curl_count(), 0);

    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/notify"))
        .unwrap();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    task.metadata.insert(
        ajax_core::attention::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        (now_secs.saturating_sub(20)).to_string(),
    );

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("refresh with notify");
    assert_eq!(curl_count(), 1);
    assert_eq!(
        specs
            .lock()
            .unwrap()
            .iter()
            .find(|spec| spec.program == "curl")
            .unwrap()
            .program,
        "curl"
    );
}

#[test]
fn web_refresh_cockpit_lifecycle_wait_notifies_once() {
    use ajax_core::config::NotifyConfig;

    let cache_dir = std::env::temp_dir().join(format!(
        "ajax-web-lifecycle-wait-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    write_agent_status_event(&cache_dir, "web/fix-login", "wait");

    let mut context = reviewable_context();
    context.runtime_paths.cache_dir = cache_dir.clone();
    context.config.notify = Some(NotifyConfig {
        webhook_url: "https://ntfy.sh/topic".to_string(),
        poll_seconds: None,
    });
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;

    #[derive(Clone)]
    struct NotifyRecordingRunner {
        specs: std::sync::Arc<std::sync::Mutex<Vec<CommandSpec>>>,
    }

    impl CommandRunner for NotifyRecordingRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            if command.program == "curl" {
                self.specs.lock().unwrap().push(command.clone());
                return Ok(CommandOutput {
                    status_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                });
            }
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "ajax-web-fix-login\n",
                [_, repo, subcommand, action, flag]
                    if repo == "/repo/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/repo/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                [command, ..] if command == "list-windows" => {
                    "ajax-web-fix-login\ttask\t/repo/web__worktrees/ajax-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "{\"type\":\"thinking\"}\n",
                _ => "",
            };
            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    let mut runner = NotifyRecordingRunner {
        specs: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
    };
    let specs = runner.specs.clone();
    let curl_count = || {
        specs
            .lock()
            .unwrap()
            .iter()
            .filter(|spec| spec.program == "curl")
            .count()
    };
    let mut bridge = CliRuntimeBridge::for_context(None, &context).unwrap();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    task.metadata.insert(
        ajax_core::attention::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        (now_secs.saturating_sub(20)).to_string(),
    );

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("first refresh with notify");
    assert_eq!(curl_count(), 1);
    let curl_body = specs
        .lock()
        .unwrap()
        .iter()
        .find(|spec| spec.program == "curl")
        .unwrap()
        .args[4]
        .clone();
    assert!(curl_body.contains("Waiting"));
    assert!(curl_body.contains("(codex)"));

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("second refresh with notify");
    assert_eq!(curl_count(), 1);

    let _ = std::fs::remove_dir_all(cache_dir);
}

#[test]
fn web_refresh_cockpit_does_not_reload_sqlite_when_state_unchanged() {
    let root = std::env::temp_dir().join(format!("ajax-web-no-reload-{}", std::process::id()));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let saved_context = reviewable_context();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&saved_context.registry)
        .unwrap();
    let mut context = crate::context::load_context(&paths).unwrap();
    let mut runner = LiveRefreshRunner;
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("first refresh");
    let tasks_after_first = context.registry.list_tasks().len();

    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("second refresh");

    assert_eq!(context.registry.list_tasks().len(), tasks_after_first);
    assert!(bridge.last_loaded_mtime.is_some());

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn web_refresh_reloads_sqlite_even_when_mtime_stays_the_same() {
    let root = std::env::temp_dir().join(format!("ajax-web-revision-{}", std::process::id()));
    let paths = CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
    let initial = reviewable_context();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&initial.registry)
        .unwrap();

    let mut context = crate::context::load_context(&paths).unwrap();
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    let mut concurrent = initial.registry.clone();
    concurrent
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("concurrent task")
        .metadata
        .insert("web".to_string(), "persisted".to_string());
    SqliteRegistryStore::new(&paths.state_file)
        .save(&concurrent)
        .unwrap();

    // Simulate a missed mtime window: the disk revision changed, but the
    // cached timestamp still points at the rewritten file.
    bridge.last_loaded_mtime = crate::context::state_file_mtime(&paths);

    let mut runner = LiveRefreshRunner;
    bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("refresh should reload the newer SQLite revision");

    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .expect("reloaded task")
        .metadata
        .insert("native".to_string(), "persisted".to_string());

    bridge
        .persist_changed_state(&mut context)
        .expect("save after web reload with stale mtime");

    let reloaded = crate::context::load_context(&paths).expect("reload saved state");
    let task = reloaded
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .expect("saved task");
    assert_eq!(
        task.metadata.get("web").map(String::as_str),
        Some("persisted")
    );
    assert_eq!(
        task.metadata.get("native").map(String::as_str),
        Some("persisted")
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn cockpit_refresh_recovers_when_task_is_deleted_from_disk() {
    let dir = scratch_dir("disk-deletion");
    let paths = CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
    let mut context = reviewable_context();
    SqliteRegistryStore::new(&paths.state_file)
        .save(&context.registry)
        .unwrap();
    let mut bridge = CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

    // Another writer deletes the task from disk, and the bridge misses the
    // reload window because its recorded mtime already matches the file.
    let store = SqliteRegistryStore::new(&paths.state_file);
    let revision = store.current_revision().unwrap();
    store
        .save_if_revision_allowing_empty_rewrite(&InMemoryRegistry::default(), revision)
        .unwrap();
    bridge.last_loaded_mtime = crate::context::state_file_mtime(&paths);

    let mut runner = LiveRefreshRunner;
    let state_changed = bridge
        .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full, true)
        .expect("refresh accepts the disk-side deletion instead of failing every poll");

    assert!(state_changed);
    assert!(context.registry.list_tasks().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

