use ajax_core::{
    adapters::{CommandRunner, ProcessCommandRunner},
    commands::CommandContext,
    registry::InMemoryRegistry,
    runtime_refresh::{refresh_runtime_context_with_tier, NoAgentStatusCache, RefreshTier},
};
#[cfg(test)]
use ajax_web::runtime::Request;
#[cfg(test)]
use ajax_web::slices::{cockpit as web_cockpit, install as web_install};
use ajax_web::{
    runtime::{self, ActionFailure, RuntimeBridge},
    slices::operate::{
        format_operate_error, operate, start_task, OperateError, OperateOutcome, OperateRequest,
        StartTaskRequest,
    },
    WebError,
};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::{
    command_error,
    context::{load_context, save_context},
    CliContextPaths, CliError,
};

#[cfg(test)]
pub(crate) type HttpResponse = runtime::Response;

#[cfg(test)]
pub(crate) fn render_mobile_shell() -> String {
    web_install::pwa_shell()
}

#[cfg(test)]
pub(crate) fn cockpit_json(
    context: &CommandContext<InMemoryRegistry>,
) -> Result<String, serde_json::Error> {
    web_cockpit::browser_cockpit_json(context)
}

#[cfg(test)]
pub(crate) fn handle_http_request(
    method: &str,
    path: &str,
    body: &str,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<HttpResponse, serde_json::Error> {
    runtime::route(Request { method, path, body }, context).map_err(|error| match error {
        runtime::RouteError::Json(error) => error,
    })
}

#[cfg(test)]
pub(crate) fn handle_http_request_with_runner_and_paths(
    method: &str,
    path: &str,
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    let dir = companion_state_dir(paths)?;
    let mut bridge = CliRuntimeBridge {
        paths: paths.cloned(),
        last_loaded_mtime: paths.and_then(state_file_mtime),
    };
    runtime::route_with_bridge(
        Request { method, path, body },
        context,
        runner,
        &mut bridge,
        &dir,
    )
    .map_err(cli_error_from_web)
}

pub(crate) fn serve_mobile_web(
    host: &str,
    port: u16,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<(), CliError> {
    serve_mobile_web_with_paths(host, port, context, runner, None)
}

pub(crate) fn serve_mobile_web_with_paths(
    host: &str,
    port: u16,
    context: &mut CommandContext<InMemoryRegistry>,
    _runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let state_dir = companion_state_dir(paths)?;
    let bridge = CliRuntimeBridge {
        paths: paths.cloned(),
        last_loaded_mtime: paths.as_ref().and_then(|paths| state_file_mtime(paths)),
    };
    let state = runtime::WebAppState::new(context.clone(), ProcessCommandRunner, bridge, state_dir);
    runtime::serve_axum_web(host, port, state).map_err(cli_error_from_web)
}

fn refresh_runtime_context_for_web<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    tier: RefreshTier,
) -> Result<bool, ajax_core::commands::CommandError> {
    #[cfg(feature = "interactive")]
    {
        use ajax_core::runtime_refresh::refresh_runtime_context_with_agent_status_cache_and_tier;

        if let Some(cache) =
            crate::agent_status_cache::TmuxAgentStatusCache::from_default_location()
        {
            return refresh_runtime_context_with_agent_status_cache_and_tier(
                context, runner, &cache, tier,
            );
        }
    }

    refresh_runtime_context_with_tier(context, runner, &NoAgentStatusCache, tier)
}

fn companion_state_dir(paths: Option<&CliContextPaths>) -> Result<PathBuf, CliError> {
    let state_file = match paths {
        Some(paths) => paths.state_file.clone(),
        None => crate::context::default_context_paths()?.state_file,
    };
    state_file
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| CliError::CommandFailed("web companion directory unresolved".to_string()))
}

#[derive(Clone)]
pub(crate) struct CliRuntimeBridge {
    paths: Option<CliContextPaths>,
    last_loaded_mtime: Option<SystemTime>,
}

impl<C: CommandRunner> RuntimeBridge<C> for CliRuntimeBridge {
    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
        tier: RefreshTier,
    ) -> Result<bool, WebError> {
        self.reload_context_if_stale(context)?;
        let state_changed = refresh_runtime_context_for_web(context, runner, tier)
            .map_err(command_error)
            .map_err(web_error_from_cli)?;
        if state_changed {
            if let Some(paths) = self.paths.as_ref() {
                save_context(paths, context).map_err(web_error_from_cli)?;
                self.last_loaded_mtime = state_file_mtime(paths);
            }
        }
        Ok(state_changed)
    }

    fn execute_operate(
        &mut self,
        request: OperateRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<OperateOutcome, ActionFailure> {
        self.persist_operate(operate(context, runner, request), context)
    }

    fn execute_start_task(
        &mut self,
        request: StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<OperateOutcome, ActionFailure> {
        self.persist_operate(start_task(context, runner, request), context)
    }
}

impl CliRuntimeBridge {
    fn reload_context_if_stale(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<(), WebError> {
        let Some(paths) = self.paths.as_ref() else {
            return Ok(());
        };
        let Some(mtime) = state_file_mtime(paths) else {
            return Ok(());
        };
        if self.last_loaded_mtime != Some(mtime) {
            *context = load_context(paths).map_err(web_error_from_cli)?;
            self.last_loaded_mtime = Some(mtime);
        }
        Ok(())
    }

    fn persist_operate(
        &mut self,
        result: Result<OperateOutcome, OperateError>,
        context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<OperateOutcome, ActionFailure> {
        match result {
            Ok(outcome) => {
                if outcome.state_changed {
                    if let Some(paths) = self.paths.as_ref() {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                        self.last_loaded_mtime = state_file_mtime(paths);
                    }
                }
                Ok(outcome)
            }
            Err(error) => {
                let state_changed = matches!(error, OperateError::Command(_, true));
                if state_changed {
                    if let Some(paths) = self.paths.as_ref() {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                        self.last_loaded_mtime = state_file_mtime(paths);
                    }
                }
                Err(ActionFailure {
                    message: format_operate_error(&error),
                    state_changed,
                })
            }
        }
    }
}

fn web_error_from_cli(error: CliError) -> WebError {
    match error {
        CliError::JsonSerialization(message) => WebError::JsonSerialization(message),
        error => WebError::CommandFailed(error.to_string()),
    }
}

fn cli_error_from_web(error: WebError) -> CliError {
    match error {
        WebError::JsonSerialization(message) => CliError::JsonSerialization(message),
        WebError::CommandFailed(message) => CliError::CommandFailed(message),
    }
}

pub(crate) fn state_file_mtime(paths: &CliContextPaths) -> Option<SystemTime> {
    if !paths.state_file.exists() {
        return None;
    }
    std::fs::metadata(&paths.state_file)
        .ok()
        .and_then(|meta| meta.modified().ok())
}

fn action_failure_from_cli(error: CliError) -> ActionFailure {
    ActionFailure {
        message: error.to_string(),
        state_changed: error.state_changed(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        cockpit_json, handle_http_request, handle_http_request_with_runner_and_paths,
        render_mobile_shell,
    };
    use ajax_core::runtime_refresh::RefreshTier;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
    };
    use ajax_web::runtime::{self, RuntimeBridge};

    #[test]
    fn mobile_shell_is_responsive_and_loads_cockpit_data() {
        let html = render_mobile_shell();

        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("name=\"viewport\""));
        assert!(html.contains("width=device-width"));
        assert!(html.contains("href=\"/app.css\""));
        assert!(html.contains("src=\"/app.js\""));
    }

    #[test]
    fn mobile_shell_exposes_redesigned_structure() {
        let html = render_mobile_shell();

        assert!(html.contains("id=\"inbox\""));
        assert!(html.contains("id=\"repos\""));
        assert!(html.contains("class=\"cockpit-chrome\""));
        assert!(html.contains("id=\"pwa-warning\""));
        assert!(html.contains("id=\"connection-status\""));
        assert!(html.contains("id=\"attention-summary\""));
        assert!(html.contains("id=\"new-task-row\""));
        assert!(html.contains("id=\"result-panel\""));
        assert!(html.contains("id=\"settings-view\""));
        assert!(html.contains("id=\"restart-server\""));
    }

    #[test]
    fn cli_web_backend_delegates_pwa_reads_to_ajax_web() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web_backend.rs"),
        )
        .unwrap();

        assert!(source.contains("ajax_web::slices::install"));
        assert!(source.contains("ajax_web::slices::cockpit"));
        assert!(!source.contains("include_str!(\"../web/"));
        assert!(!source.contains("include_bytes!(\"../web/"));
    }

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
        assert!(String::from_utf8_lossy(&shell.body).contains("Ajax Cockpit"));

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

        let js = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert_eq!(js.status_code, 200);
        assert_eq!(js.content_type, "text/javascript; charset=utf-8");
        assert!(!js.body.is_empty());
    }

    #[test]
    fn http_router_serves_web_manifest() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let manifest = handle_http_request("GET", "/manifest.webmanifest", "", &context).unwrap();
        assert_eq!(manifest.status_code, 200);
        assert_eq!(
            manifest.content_type,
            "application/manifest+json; charset=utf-8"
        );

        let value: serde_json::Value = serde_json::from_slice(&manifest.body).unwrap();
        assert!(value["name"].is_string());
        assert_eq!(value["display"], "standalone");
        assert!(value["start_url"].is_string());
        assert!(value["icons"]
            .as_array()
            .is_some_and(|icons| !icons.is_empty()));
    }

    #[test]
    fn http_router_serves_app_icons() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        for path in [
            "/icons/icon-192.png",
            "/icons/icon-512.png",
            "/icons/icon-maskable-512.png",
            "/icons/apple-touch-icon.png",
        ] {
            let icon = handle_http_request("GET", path, "", &context).unwrap();
            assert_eq!(icon.status_code, 200, "{path}");
            assert_eq!(icon.content_type, "image/png", "{path}");
            assert!(
                icon.body
                    .starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]),
                "{path} is not a PNG"
            );
        }
    }

    #[test]
    fn app_script_wires_cockpit_actions() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        let script = String::from_utf8_lossy(&app.body);
        assert!(script.contains("/api/cockpit"));
        assert!(script.contains("cache: \"no-store\""));
        assert!(script.contains("const REFRESH_INTERVAL_MS = 1000"));
        assert!(script.contains("refreshInFlight"));
        assert!(script.contains("/api/operations"));
        assert!(script.contains("request_id"));
        assert!(script.contains("structureFingerprint"));
        assert!(script.contains("updateLiveSummaries"));
        assert!(script.contains("action_states"));
        assert!(script.contains("#/settings"));
        assert!(script.contains("/api/server/restart"));
    }

    #[test]
    fn service_worker_and_app_are_push_free() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        let sw_text = String::from_utf8_lossy(&sw.body);
        assert!(sw_text.contains("self.registration.unregister"));
        assert!(!sw_text.contains("showNotification"));
        assert!(!sw_text.contains("notificationclick"));
        assert!(!sw_text.contains("addEventListener(\"push\""));

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        let app_text = String::from_utf8_lossy(&app.body);
        assert!(!app_text.contains("pushManager.subscribe"));
        assert!(!app_text.contains("/api/push/config"));
        assert!(!app_text.contains("/api/push/subscribe"));
        assert!(app_text.contains("/answer"));
    }

    #[test]
    fn http_router_serves_cleanup_service_worker_and_app_does_not_register_it() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        assert_eq!(sw.status_code, 200);
        assert_eq!(sw.content_type, "text/javascript; charset=utf-8");
        assert!(!sw.body.is_empty());

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert!(!String::from_utf8_lossy(&app.body).contains("serviceWorker.register"));
    }

    #[test]
    fn http_router_reports_unknown_routes_and_unsupported_methods() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let missing = handle_http_request("GET", "/missing", "", &context).unwrap();
        assert_eq!(missing.status_code, 404);
        assert!(String::from_utf8_lossy(&missing.body).contains("not found"));

        let unsupported = handle_http_request("POST", "/", "", &context).unwrap();
        assert_eq!(unsupported.status_code, 405);
        assert!(String::from_utf8_lossy(&unsupported.body).contains("method not allowed"));
    }

    #[test]
    fn web_supported_filter_lives_in_ajax_web_cockpit_slice() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let cli_source = std::fs::read_to_string(manifest_dir.join("src/web_backend.rs")).unwrap();
        let web_source =
            std::fs::read_to_string(manifest_dir.join("../ajax-web/src/slices/cockpit.rs"))
                .unwrap();

        let filter_fn = ["fn ", "is_web_supported"].concat();
        assert!(!cli_source.contains(&filter_fn));
        assert!(web_source.contains(&filter_fn));
        assert!(web_source.contains("OperatorAction::Resume"));
        assert!(web_source.contains("OperatorAction::Start"));
    }

    #[test]
    fn action_endpoint_guards_resume_for_native_cockpit() {
        let mut context = reviewable_context();
        let mut runner = OkRunner;

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"resume"}"#,
            &mut context,
            &mut runner,
            None,
        )
        .unwrap();

        assert_eq!(response.status_code, 409);
        assert!(String::from_utf8_lossy(&response.body).contains("resume requires native cockpit"));
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

    #[test]
    fn cockpit_api_refreshes_live_task_status_before_rendering() {
        let mut context = reviewable_context();
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
        assert_eq!(body["cards"][0]["live_summary"], "agent running");
        assert!(body["cards"][0]["action_states"].is_array());
    }

    #[test]
    fn cockpit_api_reloads_task_state_from_disk_before_rendering() {
        let root = std::env::temp_dir().join(format!("ajax-web-reload-{}", std::process::id()));
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let saved_context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut server_context =
            CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = LiveRefreshRunner;
        let mut bridge = super::CliRuntimeBridge {
            paths: Some(paths.clone()),
            last_loaded_mtime: None,
        };

        let response = runtime::route_with_bridge(
            runtime::Request {
                method: "GET",
                path: "/api/cockpit",
                body: "",
            },
            &mut server_context,
            &mut runner,
            &mut bridge,
            &root,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
        assert_eq!(body["cards"][0]["live_summary"], "agent running");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn web_refresh_cockpit_does_not_reload_sqlite_when_state_unchanged() {
        let root = std::env::temp_dir().join(format!("ajax-web-no-reload-{}", std::process::id()));
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let saved_context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut context = super::load_context(&paths).unwrap();
        let mut runner = LiveRefreshRunner;
        let mut bridge = super::CliRuntimeBridge {
            paths: Some(paths.clone()),
            last_loaded_mtime: super::state_file_mtime(&paths),
        };

        bridge
            .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full)
            .expect("first refresh");
        let tasks_after_first = context.registry.list_tasks().len();

        bridge
            .refresh_cockpit(&mut context, &mut runner, RefreshTier::Full)
            .expect("second refresh");

        assert_eq!(context.registry.list_tasks().len(), tasks_after_first);
        assert_eq!(bridge.last_loaded_mtime, super::state_file_mtime(&paths));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn cli_web_backend_uses_axum_runtime_server() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/web_backend.rs"),
        )
        .unwrap();

        assert!(source.contains("runtime::serve_axum_web"));
        assert!(source.contains("runtime::WebAppState::new"));
        assert!(!source.contains(&["runtime::", "serve_connection"].concat()));
        assert!(!source.contains(&["runtime::", "serve_mobile_web_with_bridge"].concat()));
    }

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-web-be-{tag}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn push_endpoints_are_not_supported() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let dir = scratch_dir("push");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("ajax.db"));

        let config = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/push/config",
            "",
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(config.status_code, 404);

        let subscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/subscribe",
            r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(subscribe.status_code, 405);

        let unsubscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/unsubscribe",
            r#"{"endpoint":"https://push.example/x"}"#,
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(unsubscribe.status_code, 405);

        std::fs::remove_dir_all(&dir).ok();
    }

    struct OkRunner;

    impl CommandRunner for OkRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
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
                _ => "diff stat",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    struct LiveRefreshRunner;

    impl CommandRunner for LiveRefreshRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
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
                    "ajax-web-fix-login\tworktrunk\t/repo/web__worktrees/ajax-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => "codex is working\n",
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    fn reviewable_context() -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Reviewable;
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
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/repo/web__worktrees/ajax-fix-login",
        ));
        context.registry.create_task(task).unwrap();
        context
    }
}
