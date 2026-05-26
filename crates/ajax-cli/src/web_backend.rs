use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    models::OperatorAction,
    registry::InMemoryRegistry,
    runtime_refresh::refresh_runtime_context,
    task_operations::task_command::{
        execute_task_command_operation, plan_task_command_operation, TaskCommandKind,
    },
};
#[cfg(test)]
use ajax_web::runtime::Request;
#[cfg(test)]
use ajax_web::slices::{cockpit as web_cockpit, install as web_install};
use ajax_web::{
    runtime::{self, ActionFailure, RuntimeBridge},
    slices::operate::{start_task, StartTaskRequest},
    WebError,
};
#[cfg(test)]
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::{
    command_error,
    context::{load_context, save_context},
    dispatch::execute_observed_drop,
    CliContextPaths, CliError,
};

#[cfg(test)]
pub(crate) type HttpResponse = runtime::Response;

#[cfg(test)]
pub(crate) fn render_mobile_shell() -> String {
    web_install::pwa_shell().to_string()
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
        paths,
        snapshot_only: false,
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
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let state_dir = companion_state_dir(paths)?;
    let mut bridge = CliRuntimeBridge {
        paths,
        snapshot_only: web_snapshot_only(),
    };
    runtime::serve_mobile_web_with_bridge(host, port, context, runner, &mut bridge, &state_dir)
        .map_err(cli_error_from_web)
}

/// Resolves the directory the companion persists its files in (TLS identity,
/// VAPID keys, push subscriptions): the directory holding the Ajax state
/// database.
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

fn web_snapshot_only() -> bool {
    std::env::var_os("AJAX_WEB_SNAPSHOT_ONLY").is_some_and(|value| {
        let value = value.to_string_lossy();
        value != "0" && !value.eq_ignore_ascii_case("false")
    })
}

struct CliRuntimeBridge<'a> {
    paths: Option<&'a CliContextPaths>,
    snapshot_only: bool,
}

impl<C: CommandRunner> RuntimeBridge<C> for CliRuntimeBridge<'_> {
    fn backend_authority(&self) -> ajax_web::slices::cockpit::BackendAuthority {
        if self.snapshot_only {
            ajax_web::slices::cockpit::BackendAuthority::SnapshotOnly
        } else {
            ajax_web::slices::cockpit::BackendAuthority::HostNative
        }
    }

    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, WebError> {
        if let Some(paths) = self.paths {
            *context = load_context(paths).map_err(web_error_from_cli)?;
        }
        if self.snapshot_only {
            return Ok(false);
        }
        let state_changed = refresh_runtime_context(context, runner)
            .map_err(command_error)
            .map_err(web_error_from_cli)?;
        if state_changed {
            if let Some(paths) = self.paths {
                save_context(paths, context).map_err(web_error_from_cli)?;
            }
        }
        Ok(state_changed)
    }

    fn execute_mobile_action(
        &mut self,
        action: OperatorAction,
        task_handle: &str,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, ActionFailure> {
        match execute_mobile_action(action, task_handle, context, runner) {
            Ok(state_changed) => {
                if state_changed {
                    if let Some(paths) = self.paths {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                    }
                }
                Ok(state_changed)
            }
            Err(error) => {
                let state_changed = error.state_changed();
                if state_changed {
                    if let Some(paths) = self.paths {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                    }
                }
                Err(ActionFailure {
                    message: error.to_string(),
                    state_changed,
                })
            }
        }
    }

    fn execute_start_task(
        &mut self,
        request: StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, ActionFailure> {
        match start_task(context, runner, request) {
            Ok(outcome) => {
                if outcome.state_changed {
                    if let Some(paths) = self.paths {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                    }
                }
                Ok(outcome.state_changed)
            }
            Err(error) => {
                let state_changed = matches!(
                    error,
                    ajax_web::slices::operate::OperateError::Command(_, true)
                );
                if state_changed {
                    if let Some(paths) = self.paths {
                        save_context(paths, context).map_err(action_failure_from_cli)?;
                    }
                }
                Err(ActionFailure {
                    message: format_start_error(&error),
                    state_changed,
                })
            }
        }
    }
}

fn format_start_error(error: &ajax_web::slices::operate::OperateError) -> String {
    use ajax_web::slices::operate::OperateError;
    match error {
        OperateError::UnknownAction(action) => format!("unknown action: {action}"),
        OperateError::UnsupportedCapability(message) => (*message).to_string(),
        OperateError::Command(error, _) => command_error(error.clone()).to_string(),
    }
}

#[cfg(test)]
fn serve_connection<S: Read + Write>(
    stream: S,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    paths: Option<&CliContextPaths>,
) -> Result<(), CliError> {
    let state_dir = companion_state_dir(paths)?;
    let mut bridge = CliRuntimeBridge {
        paths,
        snapshot_only: false,
    };
    runtime::serve_connection(stream, context, runner, &mut bridge, &state_dir)
        .map_err(cli_error_from_web)
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

fn action_failure_from_cli(error: CliError) -> ActionFailure {
    ActionFailure {
        message: error.to_string(),
        state_changed: error.state_changed(),
    }
}

fn execute_mobile_action(
    action: OperatorAction,
    task_handle: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CliError> {
    if action == OperatorAction::Drop {
        return execute_observed_drop(context, task_handle, true, runner)
            .map(|rendered| rendered.state_changed);
    }

    let kind = match action {
        OperatorAction::Review => TaskCommandKind::Review,
        OperatorAction::Ship => TaskCommandKind::Ship,
        OperatorAction::Repair => TaskCommandKind::Repair,
        OperatorAction::Start | OperatorAction::Resume | OperatorAction::Drop => {
            return Err(CliError::CommandFailed(format!(
                "unsupported mobile action: {}",
                action.as_str()
            )));
        }
    };
    let plan = plan_task_command_operation(context, kind, task_handle, commands::OpenMode::Attach)
        .map_err(command_error)?;
    let confirmed = !plan.requires_confirmation;
    execute_task_command_operation(context, kind, task_handle, &plan, confirmed, runner)
        .map(|(_outputs, state_changed)| state_changed)
        .map_err(|(error, state_changed)| {
            let error = command_error(error);
            if state_changed {
                error.after_state_change()
            } else {
                error
            }
        })
}

#[cfg(test)]
mod tests {
    use super::{
        cockpit_json, handle_http_request, handle_http_request_with_runner_and_paths,
        render_mobile_shell, serve_connection,
    };
    use std::cell::RefCell;
    use std::collections::BTreeSet;
    use std::io::{Cursor, Read, Write};
    use std::rc::Rc;

    /// An in-memory bidirectional stream for exercising the generic connection
    /// path without binding a socket.
    struct MockStream {
        input: Cursor<Vec<u8>>,
        output: Rc<RefCell<Vec<u8>>>,
    }

    impl Read for MockStream {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for MockStream {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.borrow_mut().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
    };

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
        assert!(html.contains("id=\"offline-banner\""));
        assert!(html.contains("id=\"refresh-button\""));
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
        assert!(script.contains("operator_token"));
    }

    #[test]
    fn service_worker_and_app_handle_push_notifications() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        let sw_text = String::from_utf8_lossy(&sw.body);
        assert!(sw_text.contains("ajax-cockpit-v15"));
        assert!(sw_text.contains("\"push\""));
        assert!(sw_text.contains("notificationclick"));
        assert!(sw_text.contains("showNotification"));

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        let app_text = String::from_utf8_lossy(&app.body);
        assert!(app_text.contains("pushManager.subscribe"));
        assert!(app_text.contains("/api/push/config"));
        assert!(app_text.contains("/api/push/subscribe"));
    }

    #[test]
    fn http_router_serves_service_worker_and_app_registers_it() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let sw = handle_http_request("GET", "/sw.js", "", &context).unwrap();
        assert_eq!(sw.status_code, 200);
        assert_eq!(sw.content_type, "text/javascript; charset=utf-8");
        assert!(!sw.body.is_empty());

        let app = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert!(String::from_utf8_lossy(&app.body).contains("serviceWorker.register"));
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
        let (dir, paths, token) = paired_paths("resume");

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/actions",
            &format!(
                r#"{{"task_handle":"web/fix-login","action":"resume","request_id":"req-resume","operator_token":"{token}"}}"#
            ),
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();

        assert_eq!(response.status_code, 409);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["status"], "needs_terminal");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("resume requires native cockpit"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn action_endpoint_executes_non_interactive_task_actions() {
        let mut context = reviewable_context();
        let mut runner = OkRunner;
        let (dir, paths, token) = paired_paths("action");

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/operations",
            &format!(
                r#"{{"task_handle":"web/fix-login","action":"review","request_id":"req-review","operator_token":"{token}"}}"#
            ),
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(body["status"], "succeeded");
        assert_eq!(
            body["cockpit"]["cards"][0]["qualified_handle"],
            "web/fix-login"
        );
        std::fs::remove_dir_all(&dir).ok();
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
        let (dir, paths, token) = paired_paths("action-fail");

        let response = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/actions",
            &format!(
                r#"{{"task_handle":"web/fix-login","action":"ship","request_id":"req-ship","operator_token":"{token}"}}"#
            ),
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .expect("handler should return a JSON error, not propagate the CliError");
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 409);
        assert_eq!(body["ok"], false);
        assert_eq!(body["status"], "failed");
        assert!(
            !body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "error message should be populated, got: {:?}",
            body["error"]
        );
        assert!(body["cockpit"].is_object());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cockpit_api_refreshes_live_task_status_before_rendering() {
        let mut context = reviewable_context();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
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

        let response = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/cockpit",
            "",
            &mut server_context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
        assert_eq!(body["cards"][0]["live_summary"], "agent running");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_only_cockpit_api_reloads_state_without_live_refresh() {
        struct PanicRunner;
        impl CommandRunner for PanicRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                panic!("snapshot-only web API should not run {command:?}");
            }
        }

        let root =
            std::env::temp_dir().join(format!("ajax-web-snapshot-only-{}", std::process::id()));
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let saved_context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut server_context =
            CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = PanicRunner;
        let mut bridge = super::CliRuntimeBridge {
            paths: Some(&paths),
            snapshot_only: true,
        };

        let response = ajax_web::runtime::route_with_bridge(
            ajax_web::runtime::Request {
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
        assert_eq!(body["backend"]["authority"], "snapshot-only");
        assert_eq!(body["backend"]["control_enabled"], false);
        assert!(body["backend"]["warning"]
            .as_str()
            .unwrap_or_default()
            .contains("host-native Ajax"));
        assert_eq!(body["cards"][0]["qualified_handle"], "web/fix-login");
        assert_ne!(body["cards"][0]["live_summary"], "agent running");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn snapshot_only_backend_rejects_mutable_mobile_operations() {
        struct PanicRunner;
        impl CommandRunner for PanicRunner {
            fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
                panic!("snapshot-only web API should not run {command:?}");
            }
        }

        let mut context = reviewable_context();
        let mut runner = PanicRunner;
        let mut bridge = super::CliRuntimeBridge {
            paths: None,
            snapshot_only: true,
        };
        let (dir, _paths, token) = paired_paths("snapshot-actions");

        for action in ["review", "ship", "repair", "drop"] {
            let response = ajax_web::runtime::route_with_bridge(
                ajax_web::runtime::Request {
                    method: "POST",
                    path: "/api/actions",
                    body: &format!(
                        r#"{{"task_handle":"web/fix-login","action":"{action}","request_id":"snapshot-{action}","operator_token":"{token}"}}"#
                    ),
                },
                &mut context,
                &mut runner,
                &mut bridge,
                &dir,
            )
            .unwrap();
            let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

            assert_eq!(response.status_code, 409);
            assert_eq!(body["ok"], false);
            assert_eq!(body["status"], "unsupported");
            assert_eq!(body["error"]["code"], "snapshot_only");
            assert!(body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("host-native Ajax"));
            assert_eq!(body["cockpit"]["backend"]["control_enabled"], false);
        }

        let start = ajax_web::runtime::route_with_bridge(
            ajax_web::runtime::Request {
                method: "POST",
                path: "/api/tasks",
                body: &format!(
                    r#"{{"repo":"web","title":"Fix search","agent":"codex","request_id":"snapshot-start","operator_token":"{token}"}}"#
                ),
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&start.body).unwrap();

        assert_eq!(start.status_code, 409);
        assert_eq!(body["ok"], false);
        assert_eq!(body["status"], "unsupported");
        assert_eq!(body["error"]["code"], "snapshot_only");
        assert!(body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("host-native Ajax"));
        assert_eq!(body["cockpit"]["backend"]["control_enabled"], false);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn serve_connection_serves_a_request_over_a_generic_stream() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let output = Rc::new(RefCell::new(Vec::new()));
        let stream = MockStream {
            input: Cursor::new(b"GET /app.css HTTP/1.1\r\nHost: ajax\r\n\r\n".to_vec()),
            output: Rc::clone(&output),
        };

        serve_connection(stream, &mut context, &mut runner, None).unwrap();

        let written = String::from_utf8_lossy(&output.borrow()).into_owned();
        assert!(written.starts_with("HTTP/1.1 200 OK"), "{written}");
        assert!(written.contains("Content-Type: text/css"), "{written}");
    }

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-web-be-{tag}-{}-{nanos}", std::process::id()))
    }

    fn paired_paths(tag: &str) -> (std::path::PathBuf, super::CliContextPaths, String) {
        let dir = scratch_dir(tag);
        std::fs::create_dir_all(&dir).unwrap();
        let token = "test-operator-token".to_string();
        std::fs::write(dir.join("web-operator-token"), format!("{token}\n")).unwrap();
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("ajax.db"));
        (dir, paths, token)
    }

    #[test]
    fn push_config_and_subscribe_endpoints_round_trip() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let (dir, paths, token) = paired_paths("push");

        let config = handle_http_request_with_runner_and_paths(
            "GET",
            "/api/push/config",
            "",
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(config.status_code, 200);
        let config_body: serde_json::Value = serde_json::from_slice(&config.body).unwrap();
        assert_eq!(config_body["public_key"].as_array().map(Vec::len), Some(65));

        let subscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/subscribe",
            &format!(
                r#"{{"operator_token":"{token}","subscription":{{"endpoint":"https://push.example/x","keys":{{"p256dh":"k","auth":"a"}}}}}}"#
            ),
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(subscribe.status_code, 200);
        assert_eq!(ajax_web::adapters::push::load_subscriptions(&dir).len(), 1);

        let unsubscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/unsubscribe",
            &format!(r#"{{"operator_token":"{token}","endpoint":"https://push.example/x"}}"#),
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(unsubscribe.status_code, 200);
        assert!(ajax_web::adapters::push::load_subscriptions(&dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn newly_attention_returns_only_freshly_added_handles() {
        let previous: BTreeSet<String> = ["web/a".to_string(), "web/b".to_string()]
            .into_iter()
            .collect();
        let current: BTreeSet<String> = [
            "web/b".to_string(),
            "web/c".to_string(),
            "web/d".to_string(),
        ]
        .into_iter()
        .collect();

        assert_eq!(
            ajax_web::slices::attention::new_attention_handles(&previous, &current),
            vec!["web/c", "web/d"]
        );
        assert!(ajax_web::slices::attention::new_attention_handles(&current, &current).is_empty());
    }

    struct OkRunner;

    impl CommandRunner for OkRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 0,
                stdout: "diff stat".to_string(),
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
            TaskId::new("task-1"),
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
