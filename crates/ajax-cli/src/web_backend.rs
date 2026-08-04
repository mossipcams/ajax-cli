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
    ajax_core::logging::init_to_logs_dir(&context.runtime_paths.logs_dir);
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
    let source = crate::agent_status_cache::AgentStatusFiles::shared_from_runtime_cache(
        &context.runtime_paths.cache_dir,
    );
    ajax_core::runtime_refresh::refresh_runtime_context_with_tier(
        context,
        runner,
        source.as_ref(),
        tier,
    )
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
        // Attention delivery is owned by ajax-web declarative push.
        // CLI must not take_attention_transition or it would stamp without pushing.
        let _ = deliver_notifications;
        if reloaded || state_changed {
            self.persist_changed_state(context)
                .map_err(web_error_from_cli)?;
        }
        Ok(reloaded || state_changed)
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

    fn persist_registry_snapshot(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
    ) -> Result<(), WebError> {
        self.persist_changed_state(context)
            .map_err(web_error_from_cli)
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
mod tests;
