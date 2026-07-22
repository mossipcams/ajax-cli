use ajax_core::{
    adapters::{CommandRunner, ProcessCommandRunner},
    commands::CommandContext,
    models::OperatorAction,
    registry::{InMemoryRegistry, Registry},
    runtime_refresh::RefreshTier,
};
#[cfg(test)]
use ajax_web::slices::{cockpit as web_cockpit, install as web_install};
use ajax_web::{
    runtime::{self, ActionFailure, RuntimeBridge},
    slices::operate::{
        format_operate_error, operate, start_task_with_checkpoint, OperateError, OperateOutcome,
        OperateRequest, StartTaskRequest,
    },
    WebError,
};
#[cfg(test)]
use axum::body::{to_bytes, Body};
#[cfg(test)]
use axum::http::{header, Request as AxumRequest};
use std::path::{Path, PathBuf};
use std::time::SystemTime;
#[cfg(test)]
use tower::util::ServiceExt;

use crate::{
    command_error,
    context::{
        context_save_state_from_registry, load_tracked_context, save_context_with_state,
        state_file_mtime, tracked_save_state, ContextSaveState,
    },
    CliContextPaths, CliError,
};

#[cfg(test)]
pub(crate) type HttpResponse = runtime::Response;

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
    let bridge = CliRuntimeBridge::for_context(None, context)
        .map_err(|error| serde_json::Error::io(std::io::Error::other(error.to_string())))?;
    let state = runtime::WebAppState::new(
        context.clone(),
        NoopRunner,
        bridge,
        test_state_dir("http-router"),
    );
    let response = dispatch_axum_request(state, method, path, body);
    Ok(response)
}

#[cfg(test)]
pub(crate) fn handle_http_request_with_runner_and_paths(
    method: &str,
    path: &str,
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut (impl CommandRunner + Clone + Send + Sync + 'static),
    paths: Option<&CliContextPaths>,
) -> Result<HttpResponse, CliError> {
    let dir = companion_state_dir(paths)?;
    let bridge = CliRuntimeBridge::for_context(paths, context)?;
    let state = runtime::WebAppState::new(context.clone(), runner.clone(), bridge, dir);
    Ok(dispatch_axum_request(state, method, path, body))
}

#[cfg(test)]
fn dispatch_axum_request<C, B>(
    state: runtime::WebAppState<C, B>,
    method: &str,
    path: &str,
    body: &str,
) -> HttpResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let cookie = "ajax_browser_session=ajax-test-browser-session";
    let app = runtime::axum_app(state);
    let request = AxumRequest::builder()
        .method(method)
        .uri(path)
        .header(header::COOKIE, cookie)
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move { app.oneshot(request).await.unwrap() });
    axum_response_to_http_response(response)
}

#[cfg(test)]
fn axum_response_to_http_response(response: axum::response::Response) -> HttpResponse {
    let status_code = response.status().as_u16();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(http_content_type_to_static)
        .unwrap_or("text/plain; charset=utf-8");
    let body = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async move { to_bytes(response.into_body(), usize::MAX).await.unwrap() });

    runtime::Response {
        status_code,
        content_type,
        body: body.to_vec(),
    }
}

#[cfg(test)]
fn http_content_type_to_static(value: &str) -> &'static str {
    match value {
        "application/json; charset=utf-8" => "application/json; charset=utf-8",
        "text/html; charset=utf-8" => "text/html; charset=utf-8",
        "text/css; charset=utf-8" => "text/css; charset=utf-8",
        "text/javascript; charset=utf-8" => "text/javascript; charset=utf-8",
        "text/plain; charset=utf-8" => "text/plain; charset=utf-8",
        other => Box::leak(other.to_string().into_boxed_str()),
    }
}

#[cfg(test)]
fn test_state_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("ajax-web-cli-test-{tag}-{}", std::process::id()))
}

#[cfg(test)]
#[derive(Clone)]
struct NoopRunner;

#[cfg(test)]
impl CommandRunner for NoopRunner {
    fn run(
        &mut self,
        _command: &ajax_core::adapters::CommandSpec,
    ) -> Result<ajax_core::adapters::CommandOutput, ajax_core::adapters::CommandRunError> {
        Ok(ajax_core::adapters::CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
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
    let bridge = CliRuntimeBridge::for_context(paths, context)?;
    let _ = crate::agent_event_notify::start_agent_event_notify_listener(
        context.runtime_paths.cache_dir.join("agent-events"),
    );
    let state = runtime::WebAppState::load_or_create(
        context.clone(),
        ProcessCommandRunner,
        bridge,
        state_dir,
    )
    .map_err(cli_error_from_web)?;
    runtime::serve_axum_web(host, port, state).map_err(cli_error_from_web)
}

fn refresh_runtime_context_for_web<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    tier: RefreshTier,
) -> Result<bool, ajax_core::commands::CommandError> {
    let cache = crate::agent_status_cache::TmuxAgentStatusSnapshot::from_runtime_cache(
        &context.runtime_paths.cache_dir,
    );
    ajax_core::runtime_refresh::refresh_runtime_context_with_tier(context, runner, &cache, tier)
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
    save_state: ContextSaveState,
}

impl<C: CommandRunner> RuntimeBridge<C> for CliRuntimeBridge {
    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
        tier: RefreshTier,
        deliver_notifications: bool,
    ) -> Result<bool, WebError> {
        let reloaded = self.reload_context_if_stale(context)?;
        let state_changed = refresh_runtime_context_for_web(context, runner, tier)
            .map_err(command_error)
            .map_err(web_error_from_cli)?;
        let notified = if deliver_notifications {
            crate::notify::notify_attention_transitions(context, runner)
        } else {
            false
        };
        if reloaded || state_changed || notified {
            self.persist_changed_state(context)
                .map_err(web_error_from_cli)?;
        }
        Ok(reloaded || state_changed || notified)
    }

    fn execute_operate(
        &mut self,
        request: OperateRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<OperateOutcome, ActionFailure> {
        let authorize_empty = request.action == OperatorAction::Drop.as_str();
        let result = operate(context, runner, request);
        if authorize_empty
            && match &result {
                Ok(outcome) => outcome.state_changed,
                Err(OperateError::Command(_, true)) => true,
                _ => false,
            }
        {
            self.save_state.allow_empty_registry_once();
        }
        self.persist_operate(result, context)
    }

    fn execute_start_task(
        &mut self,
        request: StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<OperateOutcome, ActionFailure> {
        let paths = self.paths.clone();
        let mut save_state = self.save_state.clone();
        let result = start_task_with_checkpoint(context, runner, request, |checkpoint_context| {
            let Some(paths) = paths.as_ref() else {
                return Ok(());
            };
            save_context_with_state(paths, checkpoint_context, &mut save_state).map_err(|error| {
                ajax_core::commands::CommandError::CommandRun(
                    ajax_core::adapters::CommandRunError::SpawnFailed(format!(
                        "persist start checkpoint: {error}"
                    )),
                )
            })
        });
        let checkpoint_saved = save_state.loaded_revision != self.save_state.loaded_revision;
        self.save_state = save_state;
        if checkpoint_saved {
            self.last_loaded_mtime = self.paths.as_ref().and_then(state_file_mtime);
        }
        self.persist_operate(result, context)
    }

    fn acknowledge_operator_input(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        qualified_handle: &str,
    ) -> Result<bool, WebError> {
        // Coalesce per episode: only acknowledge when there is live waiting
        // evidence observed strictly after the last acknowledgment, so repeat
        // operator typing without newer evidence does not re-persist the
        // registry. (Some(_), None) means the task has live evidence and was
        // never acknowledged; that is actionable. No live evidence yet means
        // there is nothing for the operator to acknowledge.
        let needs_ack = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == qualified_handle)
            .map(
                |task| match (task.live_status_observed_at, task.attention_acknowledged_at) {
                    (Some(observed), Some(ack)) => observed > ack,
                    (Some(_), None) => true,
                    _ => false,
                },
            )
            .unwrap_or(false);

        if !needs_ack {
            return Ok(false);
        }

        ajax_core::commands::mark_task_opened_at(context, qualified_handle, SystemTime::now())
            .map_err(command_error)
            .map_err(web_error_from_cli)?;
        self.persist_changed_state(context)
            .map_err(web_error_from_cli)?;
        Ok(true)
    }
}

impl CliRuntimeBridge {
    fn for_context(
        paths: Option<&CliContextPaths>,
        context: &CommandContext<InMemoryRegistry>,
    ) -> Result<Self, CliError> {
        let save_state = match paths {
            Some(paths) => tracked_save_state(paths, &context.registry)?,
            None => context_save_state_from_registry(&context.registry),
        };
        Ok(Self {
            paths: paths.cloned(),
            last_loaded_mtime: paths.and_then(state_file_mtime),
            save_state,
        })
    }

    fn reload_context_if_stale(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<bool, WebError> {
        let Some(paths) = self.paths.as_ref() else {
            return Ok(false);
        };
        let Some(mtime) = state_file_mtime(paths) else {
            return Ok(false);
        };
        let revision = ajax_core::registry::SqliteRegistryStore::new(&paths.state_file)
            .current_revision()
            .map_err(|error| {
                web_error_from_cli(CliError::ContextLoad(format!(
                    "state revision failed: {error}"
                )))
            })?;
        if self.last_loaded_mtime == Some(mtime) && revision == self.save_state.loaded_revision {
            return Ok(false);
        }
        let tracked = load_tracked_context(paths).map_err(web_error_from_cli)?;
        *context = tracked.context;
        self.save_state = tracked.save_state;
        self.last_loaded_mtime = Some(mtime);
        Ok(true)
    }

    fn persist_changed_state(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<(), CliError> {
        let Some(paths) = self.paths.as_ref() else {
            return Ok(());
        };
        save_context_with_state(paths, context, &mut self.save_state)?;
        context.registry = self.save_state.loaded_registry.clone();
        self.last_loaded_mtime = state_file_mtime(paths);
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
                    self.persist_changed_state(context)
                        .map_err(action_failure_from_cli)?;
                }
                Ok(outcome)
            }
            Err(error) => {
                let state_changed = matches!(error, OperateError::Command(_, true));
                if state_changed {
                    self.persist_changed_state(context)
                        .map_err(action_failure_from_cli)?;
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

fn action_failure_from_cli(error: CliError) -> ActionFailure {
    ActionFailure {
        message: error.to_string(),
        state_changed: error.state_changed(),
    }
}

#[cfg(test)]
mod tests {
    use super::{cockpit_json, handle_http_request, handle_http_request_with_runner_and_paths};
    use ajax_core::runtime_refresh::RefreshTier;
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{
            AgentClient, GitStatus, LifecycleStatus, Task, TaskId, TaskWindowStatus, TmuxStatus,
        },
        registry::{InMemoryRegistry, Registry, SqliteRegistryStore},
    };
    use ajax_web::runtime::{self, RuntimeBridge};
    use axum::{body::Body, http::Request as AxumRequest};
    use std::time::SystemTime;
    use tower::util::ServiceExt;

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
            super::web_install::browser_shell()
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
            super::web_install::static_asset("/app.css").unwrap().body
        );

        let js = handle_http_request("GET", "/app.js", "", &context).unwrap();
        assert_eq!(js.status_code, 200);
        assert_eq!(js.content_type, "text/javascript; charset=utf-8");
        assert!(!js.body.is_empty());
        assert_eq!(
            js.body,
            super::web_install::static_asset("/app.js").unwrap().body
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
        let events_dir = cache_dir.join("agent-events");
        std::fs::create_dir_all(&events_dir).unwrap();
        let now_millis = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        std::fs::write(
            events_dir.join(format!("{}.json", task_id.replace('/', "__"))),
            serde_json::json!({
                "task_id": task_id,
                "run_id": "primary",
                "value": value,
                "observed_at_unix_millis": now_millis,
            })
            .to_string(),
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
            super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
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
        let bridge = super::CliRuntimeBridge {
            paths: Some(paths.clone()),
            last_loaded_mtime: None,
            save_state: crate::context::tracked_save_state(&paths, &server_context.registry)
                .unwrap(),
        };
        let state =
            runtime::WebAppState::new(server_context.clone(), runner.clone(), bridge, root.clone());
        let app = runtime::axum_app(state);

        let response = super::axum_response_to_http_response(
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
        let mut bridge = super::CliRuntimeBridge::for_context(None, &context).unwrap();
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
        let mut bridge = super::CliRuntimeBridge::for_context(None, &context).unwrap();
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
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let saved_context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut context = crate::context::load_context(&paths).unwrap();
        let mut runner = LiveRefreshRunner;
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

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
        let paths = super::CliContextPaths::new(root.join("config.toml"), root.join("state.db"));
        let initial = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&initial.registry)
            .unwrap();

        let mut context = crate::context::load_context(&paths).unwrap();
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

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
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let mut context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&context.registry)
            .unwrap();
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

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

    #[test]
    fn web_bridge_rejects_empty_save_over_non_empty_sqlite_state() {
        let dir = scratch_dir("empty-save-guard");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let saved_context = reviewable_context();
        let store = SqliteRegistryStore::new(&paths.state_file);
        store.save(&saved_context.registry).unwrap();
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut bridge = super::CliRuntimeBridge {
            paths: Some(paths.clone()),
            last_loaded_mtime: crate::context::state_file_mtime(&paths),
            save_state: crate::context::ContextSaveState {
                loaded_registry: InMemoryRegistry::default(),
                loaded_revision: store.current_revision().unwrap(),
                allow_empty_registry_once: false,
            },
        };

        let error = bridge.persist_changed_state(&mut context).unwrap_err();

        assert!(error
            .to_string()
            .contains("refusing to save empty registry"));
        let reloaded = crate::context::load_context(&paths).expect("reload after rejected save");
        assert!(reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .is_some());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn web_bridge_persists_confirmed_mismatch_branch_adoption() {
        let dir = scratch_dir("mismatch-adoption");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let mut saved_context = reviewable_context();
        {
            let task = saved_context
                .registry
                .get_task_mut(&TaskId::new("web/fix-login"))
                .unwrap();
            task.lifecycle_status = LifecycleStatus::Active;
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
        }
        SqliteRegistryStore::new(&paths.state_file)
            .save(&saved_context.registry)
            .unwrap();
        let mut context = saved_context;
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();
        let mut runner = MismatchAdoptionRunner::default();

        let outcome = bridge
            .execute_operate(
                super::OperateRequest {
                    task_handle: "web/fix-login".to_string(),
                    action: "repair".to_string(),
                    confirmed: true,
                    branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                        expected_branch: "ajax/fix-login".to_string(),
                        observed_branch: "fix/pane-stuck".to_string(),
                    }),
                },
                &mut context,
                &mut runner,
            )
            .expect("confirmed mismatch repair should adopt");

        assert!(outcome.state_changed);
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .branch,
            "fix/pane-stuck"
        );
        assert_eq!(runner.commands.len(), 2);
        assert!(runner.commands.iter().all(|command| {
            command.program == "git"
                && (command
                    .args
                    .windows(2)
                    .any(|window| window == ["worktree", "list"])
                    || command
                        .args
                        .windows(2)
                        .any(|window| window == ["branch", "--format=%(refname:short)"]))
        }));

        let reloaded = crate::context::load_context(&paths).expect("reload after adoption");
        let task = reloaded
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert_eq!(task.branch, "fix/pane-stuck");
        assert_eq!(
            task.worktree_path,
            std::path::Path::new("/repo/web__worktrees/ajax-fix-login")
        );
        assert_eq!(task.tmux_session, "ajax-web-fix-login");
        assert!(!task.has_checkout_mismatch());

        let _ = std::fs::remove_dir_all(dir);
    }

    #[derive(Default)]
    struct MismatchAdoptionRunner {
        commands: Vec<CommandSpec>,
    }

    impl CommandRunner for MismatchAdoptionRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            let stdout = match command.args.as_slice() {
                [_, repo, subcommand, ..] if repo == "/repo/web" && subcommand == "worktree" => {
                    "worktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/fix/pane-stuck\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/repo/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\najax/fix-login\n"
                }
                _ => "",
            };
            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn web_bridge_drop_of_last_task_persists_empty_registry() {
        let dir = scratch_dir("drop-empty-registry");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let context = reviewable_context();
        SqliteRegistryStore::new(&paths.state_file)
            .save(&context.registry)
            .unwrap();
        let mut context = context;
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();
        let mut runner = AbsentDropRunner;

        let outcome = bridge
            .execute_operate(
                super::OperateRequest {
                    task_handle: "web/fix-login".to_string(),
                    action: "drop".to_string(),
                    confirmed: false,
                    branch_adoption: None,
                },
                &mut context,
                &mut runner,
            )
            .expect("drop of the sole task should succeed");

        assert!(outcome.state_changed);
        assert!(context.registry.list_tasks().is_empty());

        let reloaded = crate::context::load_context(&paths).expect("reload after drop");
        assert!(
            reloaded.registry.list_tasks().is_empty(),
            "empty registry should be persisted after dropping the last task"
        );

        let _ = std::fs::remove_dir_all(dir);
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
        assert_eq!(subscribe.status_code, 404);

        let unsubscribe = handle_http_request_with_runner_and_paths(
            "POST",
            "/api/push/unsubscribe",
            r#"{"endpoint":"https://push.example/x"}"#,
            &mut context,
            &mut runner,
            Some(&paths),
        )
        .unwrap();
        assert_eq!(unsubscribe.status_code, 404);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[derive(Clone)]
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

    #[derive(Clone)]
    struct AbsentDropRunner;

    impl CommandRunner for AbsentDropRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            let stdout = match command.args.as_slice() {
                [command, ..] if command == "list-sessions" => "",
                [_, repo, subcommand, action, flag]
                    if repo == "/repo/web"
                        && subcommand == "worktree"
                        && action == "list"
                        && flag == "--porcelain" =>
                {
                    "worktree /repo/web\nHEAD 1111111\nbranch refs/heads/main\n\n"
                }
                [_, repo, subcommand, format]
                    if repo == "/repo/web"
                        && subcommand == "branch"
                        && format == "--format=%(refname:short)" =>
                {
                    "main\n"
                }
                _ => "",
            };

            Ok(CommandOutput {
                status_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            })
        }
    }

    #[derive(Clone)]
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
                    "ajax-web-fix-login\ttask\t/repo/web__worktrees/ajax-fix-login\n"
                }
                [command, ..] if command == "capture-pane" => {
                    // Structured Cursor lifecycle evidence — generic busy chrome
                    // alone no longer projects AgentRunning.
                    "{\"type\":\"thinking\"}\n"
                }
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
            "task",
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
        task.task_window_status = Some(TaskWindowStatus::present(
            "task",
            "/repo/web__worktrees/ajax-fix-login",
        ));
        context.registry.create_task(task).unwrap();
        context
    }

    #[test]
    fn acknowledge_operator_input_marks_attention_and_persists_across_reload() {
        let dir = scratch_dir("ack-operator-input");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let mut context = reviewable_context();
        // Make the task waiting & un-acknowledged: live evidence observed after
        // the last acknowledgment (which is None), so the bridge acknowledges.
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("web/fix-login"))
                .expect("task present");
            task.live_status_observed_at = Some(SystemTime::now());
            task.attention_acknowledged_at = None;
        }
        SqliteRegistryStore::new(&paths.state_file)
            .save(&context.registry)
            .unwrap();
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

        let acked =
            <super::CliRuntimeBridge as RuntimeBridge<super::NoopRunner>>::acknowledge_operator_input(
                &mut bridge,
                &mut context,
                "web/fix-login",
            )
            .expect("ack ok");
        assert!(acked, "first operator input acknowledges and persists");
        assert!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .attention_acknowledged_at
                .is_some(),
            "in-context task stamped with attention_acknowledged_at"
        );

        // Persisted across reload: the saved state carries the acknowledgment.
        let reloaded = crate::context::load_context(&paths).expect("reload saved state");
        assert!(
            reloaded
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .attention_acknowledged_at
                .is_some(),
            "acknowledgment survived reload"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn acknowledge_operator_input_skips_persist_without_newer_evidence() {
        let dir = scratch_dir("ack-no-newer-evidence");
        let paths = super::CliContextPaths::new(dir.join("config.toml"), dir.join("state.db"));
        let mut context = reviewable_context();
        // Stamp live evidence strictly before the last ack, so needs_ack is
        // false: the operator has already acknowledged everything newer.
        let earlier = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000);
        let later = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2_000);
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("web/fix-login"))
                .expect("task present");
            task.live_status_observed_at = Some(earlier);
            task.attention_acknowledged_at = Some(later);
        }
        SqliteRegistryStore::new(&paths.state_file)
            .save(&context.registry)
            .unwrap();
        let mut bridge = super::CliRuntimeBridge::for_context(Some(&paths), &context).unwrap();

        let store = SqliteRegistryStore::new(&paths.state_file);
        let revision_before = store.current_revision().unwrap();

        let acked =
            <super::CliRuntimeBridge as RuntimeBridge<super::NoopRunner>>::acknowledge_operator_input(
                &mut bridge,
                &mut context,
                "web/fix-login",
            )
            .expect("ack ok");
        assert!(!acked, "no newer evidence => no ack");

        let revision_after = store.current_revision().unwrap();
        assert_eq!(
            revision_before, revision_after,
            "idempotent call did not persist a new revision"
        );
        // The in-context acknowledgment is unchanged too.
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("web/fix-login"))
                .unwrap()
                .attention_acknowledged_at,
            Some(later),
            "in-context acknowledgment unchanged"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
