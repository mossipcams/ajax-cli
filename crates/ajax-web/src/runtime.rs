//! Web companion runtime wiring.

use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    models::OperatorAction,
    output::CockpitView,
    registry::{InMemoryRegistry, Registry},
};
use serde::Deserialize;
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    adapters::{push, tls},
    slices::{attention, cockpit, install},
    WebError,
};

pub struct Request<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a str,
}

pub struct Response {
    pub status_code: u16,
    pub content_type: &'static str,
    pub cache_control: &'static str,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub enum RouteError {
    Json(serde_json::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionFailure {
    pub message: String,
    pub state_changed: bool,
}

pub trait RuntimeBridge<C: CommandRunner> {
    fn backend_authority(&self) -> cockpit::BackendAuthority {
        cockpit::BackendAuthority::HostNative
    }

    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, WebError>;

    fn execute_mobile_action(
        &mut self,
        action: OperatorAction,
        task_handle: &str,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, ActionFailure>;

    fn execute_start_task(
        &mut self,
        request: crate::slices::operate::StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, ActionFailure>;
}

#[derive(Deserialize)]
struct MobileActionRequest {
    task_handle: String,
    action: String,
    request_id: String,
}

#[derive(Deserialize)]
struct PushSubscriptionRequest {
    subscription: push::PushSubscription,
}

#[derive(Deserialize)]
struct PushUnsubscribeRequest {
    endpoint: String,
}

const CACHE_NO_STORE: &str = "no-store";
const CACHE_REVALIDATE: &str = "no-cache, must-revalidate";
const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

pub fn route<R: Registry>(
    request: Request<'_>,
    context: &CommandContext<R>,
) -> Result<Response, RouteError> {
    let path = request.path.split('?').next().unwrap_or(request.path);
    match (request.method, path) {
        ("GET", "/") => Ok(Response {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            cache_control: CACHE_REVALIDATE,
            body: install::pwa_shell().as_bytes().to_vec(),
        }),
        ("GET", "/healthz") => Ok(text_response(200, "ok")),
        ("GET", "/api/cockpit") => Ok(Response {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
            cache_control: CACHE_NO_STORE,
            body: cockpit::browser_cockpit_json(context)
                .map_err(RouteError::Json)?
                .into_bytes(),
        }),
        ("GET", path) if path.starts_with("/api/tasks/") => {
            let handle = &path["/api/tasks/".len()..];
            match cockpit::browser_task_detail_json(context, handle) {
                Some(Ok(body)) => Ok(Response {
                    status_code: 200,
                    content_type: "application/json; charset=utf-8",
                    cache_control: CACHE_NO_STORE,
                    body: body.into_bytes(),
                }),
                Some(Err(error)) => Err(RouteError::Json(error)),
                None => json_route_error(404, "task not found"),
            }
        }
        ("GET", asset_path) if !asset_path.starts_with("/api/") => {
            match install::static_asset(asset_path) {
                Some(asset) => Ok(Response {
                    status_code: 200,
                    content_type: asset.content_type,
                    cache_control: static_asset_cache_control(asset_path),
                    body: asset.body.to_vec(),
                }),
                None => Ok(text_response(404, "not found")),
            }
        }
        (_, api_path) if is_known_api_path(api_path) => json_route_error(405, "method not allowed"),
        (_, api_path) if api_path.starts_with("/api/") => json_route_error(404, "not found"),
        _ => Ok(text_response(405, "method not allowed")),
    }
}

pub fn route_with_bridge<C: CommandRunner>(
    request: Request<'_>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<Response, WebError> {
    let path = request.path.split('?').next().unwrap_or(request.path);
    match (request.method, path) {
        ("GET", "/api/cockpit") => handle_refreshed_cockpit_request(context, runner, bridge),
        ("POST", "/api/actions") | ("POST", "/api/operations") => {
            handle_action_request(request.body, context, runner, bridge, state_dir)
        }
        ("POST", "/api/tasks") => {
            handle_start_task_request(request.body, context, runner, bridge, state_dir)
        }
        ("GET", "/api/push/config") => handle_push_config(state_dir),
        ("POST", "/api/push/subscribe") => handle_push_subscribe(request.body, state_dir),
        ("POST", "/api/push/unsubscribe") => handle_push_unsubscribe(request.body, state_dir),
        _ => route(request, context).map_err(|error| match error {
            RouteError::Json(error) => WebError::JsonSerialization(error.to_string()),
        }),
    }
}

pub fn serve_mobile_web_with_bridge<C: CommandRunner>(
    host: &str,
    port: u16,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<(), WebError> {
    let identity = tls::load_or_create_identity(state_dir)?;
    let tls_config = tls::tls_server_config(&identity)?;

    let listener = TcpListener::bind((host, port))
        .map_err(|error| WebError::CommandFailed(format!("web bind failed: {error}")))?;
    eprintln!("Ajax mobile web listening on https://{host}:{port}");

    let shared = Mutex::new(context);
    std::thread::scope(|scope| {
        let poller_state = &shared;
        let poller_dir = state_dir.to_path_buf();
        scope.spawn(move || run_attention_poller(poller_state, &poller_dir));

        for stream in listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(error) => {
                    eprintln!("Ajax web accept error: {error}");
                    continue;
                }
            };
            let mut guard = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let context: &mut CommandContext<InMemoryRegistry> = &mut guard;
            if let Err(error) =
                serve_tls_connection(stream, &tls_config, context, runner, bridge, state_dir)
            {
                eprintln!("Ajax web connection error: {error}");
            }
        }
    });

    Ok(())
}

fn handle_refreshed_cockpit_request<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    let _state_changed = bridge.refresh_cockpit(context, runner)?;
    let backend = bridge.backend_authority();
    json_response(
        200,
        serde_json::to_value(cockpit::browser_cockpit_view_with_backend(context, backend))
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    )
}

fn handle_action_request<C: CommandRunner>(
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<Response, WebError> {
    let request: MobileActionRequest = serde_json::from_str(body)
        .map_err(|error| WebError::JsonSerialization(error.to_string()))?;
    if request.request_id.trim().is_empty() {
        return operation_error_response(
            400,
            None,
            "unsupported",
            "missing_request_id",
            "request_id is required",
            context,
            bridge.backend_authority(),
        );
    }
    if let Some(response) = load_idempotent_response(state_dir, &request.request_id)? {
        return Ok(response);
    }

    let Some(action) = OperatorAction::from_label(&request.action) else {
        let response = operation_error_response(
            400,
            Some(&request.request_id),
            "unsupported",
            "unknown_action",
            &format!("unknown action: {}", request.action),
            context,
            bridge.backend_authority(),
        );
        return store_idempotent_response(state_dir, &request.request_id, response?);
    };

    if action == OperatorAction::Resume {
        let response = operation_error_response(
            409,
            Some(&request.request_id),
            "needs_terminal",
            "needs_terminal",
            "resume requires native cockpit task entry",
            context,
            bridge.backend_authority(),
        );
        return store_idempotent_response(state_dir, &request.request_id, response?);
    }
    if action == OperatorAction::Start {
        let response = operation_error_response(
            400,
            Some(&request.request_id),
            "unsupported",
            "unsupported",
            "start requires task title input",
            context,
            bridge.backend_authority(),
        );
        return store_idempotent_response(state_dir, &request.request_id, response?);
    }

    let backend = bridge.backend_authority();
    if !backend.control_enabled() {
        let response = control_disabled_response(Some(&request.request_id), context, backend)?;
        return store_idempotent_response(state_dir, &request.request_id, response);
    }

    let Some(lock) =
        OperationLock::try_acquire(state_dir, &request.task_handle, &request.request_id)?
    else {
        let response = operation_error_response(
            409,
            Some(&request.request_id),
            "blocked",
            "operation_in_progress",
            "another operation is already running for this task",
            context,
            backend,
        )?;
        return store_idempotent_response(state_dir, &request.request_id, response);
    };

    let response = match bridge.execute_mobile_action(action, &request.task_handle, context, runner)
    {
        Ok(state_changed) => {
            operation_success_response(&request.request_id, state_changed, context, backend)
        }
        Err(error) => operation_error_response(
            409,
            Some(&request.request_id),
            "failed",
            "operation_failed",
            &error.message,
            context,
            backend,
        ),
    }?;
    drop(lock);
    store_idempotent_response(state_dir, &request.request_id, response)
}

fn handle_start_task_request<C: CommandRunner>(
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<Response, WebError> {
    let request: crate::slices::operate::StartTaskRequest = serde_json::from_str(body)
        .map_err(|error| WebError::JsonSerialization(error.to_string()))?;
    if request.request_id.trim().is_empty() {
        return operation_error_response(
            400,
            None,
            "unsupported",
            "missing_request_id",
            "request_id is required",
            context,
            bridge.backend_authority(),
        );
    }
    if let Some(response) = load_idempotent_response(state_dir, &request.request_id)? {
        return Ok(response);
    }
    let request_id = request.request_id.clone();
    let backend = bridge.backend_authority();
    if !backend.control_enabled() {
        let response = control_disabled_response(Some(&request_id), context, backend)?;
        return store_idempotent_response(state_dir, &request_id, response);
    }

    match bridge.execute_start_task(request, context, runner) {
        Ok(state_changed) => {
            let response =
                operation_success_response(&request_id, state_changed, context, backend)?;
            store_idempotent_response(state_dir, &request_id, response)
        }
        Err(error) => {
            let response = operation_error_response(
                409,
                Some(&request_id),
                "failed",
                "operation_failed",
                &error.message,
                context,
                backend,
            )?;
            store_idempotent_response(state_dir, &request_id, response)
        }
    }
}

fn operation_success_response(
    request_id: &str,
    state_changed: bool,
    context: &CommandContext<InMemoryRegistry>,
    backend: cockpit::BackendAuthority,
) -> Result<Response, WebError> {
    json_response(
        200,
        serde_json::json!({
            "ok": true,
            "operation_id": operation_id(request_id),
            "status": "succeeded",
            "state_changed": state_changed,
            "cockpit": cockpit::browser_cockpit_view_with_backend(context, backend),
        }),
    )
}

fn operation_error_response(
    status_code: u16,
    request_id: Option<&str>,
    status: &str,
    code: &str,
    message: &str,
    context: &CommandContext<InMemoryRegistry>,
    backend: cockpit::BackendAuthority,
) -> Result<Response, WebError> {
    let mut value = serde_json::json!({
        "ok": false,
        "status": status,
        "error": {
            "code": code,
            "message": message,
        },
        "cockpit": cockpit::browser_cockpit_view_with_backend(context, backend),
    });
    if let Some(request_id) = request_id {
        value["operation_id"] = serde_json::Value::String(operation_id(request_id));
    }
    json_response(status_code, value)
}

fn control_disabled_response(
    request_id: Option<&str>,
    context: &CommandContext<InMemoryRegistry>,
    backend: cockpit::BackendAuthority,
) -> Result<Response, WebError> {
    operation_error_response(
        409,
        request_id,
        "unsupported",
        "snapshot_only",
        "mutable PWA actions require the host-native Ajax web backend with access to SQLite, repo paths, worktrees, tmux sessions, agent CLIs, and host process state",
        context,
        backend,
    )
}

fn handle_push_config(state_dir: &Path) -> Result<Response, WebError> {
    let keys = push::load_or_create_vapid_keys(state_dir)?;
    json_response(
        200,
        serde_json::json!({
            "public_key": keys.public_key,
        }),
    )
}

fn handle_push_subscribe(body: &str, state_dir: &Path) -> Result<Response, WebError> {
    let request: PushSubscriptionRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return json_response(
                400,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            );
        }
    };
    push::add_subscription(state_dir, request.subscription)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

fn handle_push_unsubscribe(body: &str, state_dir: &Path) -> Result<Response, WebError> {
    let request: PushUnsubscribeRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(error) => {
            return json_response(
                400,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            );
        }
    };
    push::remove_subscription(state_dir, &request.endpoint)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

fn operation_id(request_id: &str) -> String {
    format!("web-{request_id}")
}

fn operation_dir(state_dir: &Path) -> std::path::PathBuf {
    state_dir.join("web-operations")
}

fn idempotency_path(state_dir: &Path, request_id: &str) -> std::path::PathBuf {
    operation_dir(state_dir).join(format!("{}.json", storage_key(request_id)))
}

fn idempotency_status_path(state_dir: &Path, request_id: &str) -> std::path::PathBuf {
    operation_dir(state_dir).join(format!("{}.status", storage_key(request_id)))
}

fn load_idempotent_response(
    state_dir: &Path,
    request_id: &str,
) -> Result<Option<Response>, WebError> {
    let path = idempotency_path(state_dir, request_id);
    if !path.exists() {
        return Ok(None);
    }
    let body = fs::read(path)
        .map_err(|error| WebError::CommandFailed(format!("web operation read failed: {error}")))?;
    let status_code = fs::read_to_string(idempotency_status_path(state_dir, request_id))
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(200);
    Ok(Some(Response {
        status_code,
        content_type: "application/json; charset=utf-8",
        cache_control: CACHE_NO_STORE,
        body,
    }))
}

fn store_idempotent_response(
    state_dir: &Path,
    request_id: &str,
    response: Response,
) -> Result<Response, WebError> {
    fs::create_dir_all(operation_dir(state_dir)).map_err(|error| {
        WebError::CommandFailed(format!("web operation dir create failed: {error}"))
    })?;
    fs::write(idempotency_path(state_dir, request_id), &response.body)
        .map_err(|error| WebError::CommandFailed(format!("web operation write failed: {error}")))?;
    fs::write(
        idempotency_status_path(state_dir, request_id),
        response.status_code.to_string(),
    )
    .map_err(|error| {
        WebError::CommandFailed(format!("web operation status write failed: {error}"))
    })?;
    Ok(response)
}

struct OperationLock {
    path: std::path::PathBuf,
}

impl OperationLock {
    fn try_acquire(
        state_dir: &Path,
        task_handle: &str,
        request_id: &str,
    ) -> Result<Option<Self>, WebError> {
        fs::create_dir_all(operation_dir(state_dir)).map_err(|error| {
            WebError::CommandFailed(format!("web operation dir create failed: {error}"))
        })?;
        let path = operation_lock_path(state_dir, task_handle);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(request_id.as_bytes()).map_err(|error| {
                    WebError::CommandFailed(format!("web operation lock write failed: {error}"))
                })?;
                Ok(Some(Self { path }))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(None),
            Err(error) => Err(WebError::CommandFailed(format!(
                "web operation lock create failed: {error}"
            ))),
        }
    }
}

impl Drop for OperationLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn operation_lock_path(state_dir: &Path, task_handle: &str) -> std::path::PathBuf {
    operation_dir(state_dir).join(format!("{}.lock", storage_key(task_handle)))
}

fn storage_key(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

const ATTENTION_POLL_INTERVAL: Duration = Duration::from_secs(15);

fn run_attention_poller(state: &Mutex<&mut CommandContext<InMemoryRegistry>>, dir: &Path) {
    let mut known: BTreeSet<String> = BTreeSet::new();
    loop {
        std::thread::sleep(ATTENTION_POLL_INTERVAL);
        let current = {
            let guard = state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            attention_handles(&commands::rebuild_cockpit_view(&**guard))
        };
        for handle in attention::new_attention_handles(&known, &current) {
            let notification = push::PushNotification {
                title: "Ajax task needs attention".to_string(),
                body: handle.clone(),
                tag: handle.clone(),
            };
            if let Err(error) = push::send_to_all(dir, &notification) {
                eprintln!("Ajax web push notification failed: {error}");
            }
        }
        known = current;
    }
}

fn attention_handles(view: &CockpitView) -> BTreeSet<String> {
    view.inbox
        .items
        .iter()
        .map(|item| item.task_handle.clone())
        .collect()
}

fn serve_tls_connection<C: CommandRunner>(
    tcp: TcpStream,
    tls_config: &Arc<rustls::ServerConfig>,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<(), WebError> {
    let connection = rustls::ServerConnection::new(Arc::clone(tls_config))
        .map_err(|error| WebError::CommandFailed(format!("web tls session failed: {error}")))?;
    let stream = rustls::StreamOwned::new(connection, tcp);
    serve_connection(stream, context, runner, bridge, state_dir)
}

pub fn serve_connection<S: Read + Write, C: CommandRunner>(
    mut stream: S,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    state_dir: &Path,
) -> Result<(), WebError> {
    let mut buffer = [0_u8; 8192];
    let bytes_read = stream
        .read(&mut buffer)
        .map_err(|error| WebError::CommandFailed(format!("web request read failed: {error}")))?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let Some(request_line) = request.lines().next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let mut parts = request_line.split_whitespace();
    let Some(method) = parts.next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let Some(path) = parts.next() else {
        return write_http_response(stream, text_response(400, "bad request"));
    };
    let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
    let response = route_with_bridge(
        Request { method, path, body },
        context,
        runner,
        bridge,
        state_dir,
    )?;

    write_http_response(stream, response)
}

fn write_http_response<S: Write>(mut stream: S, response: Response) -> Result<(), WebError> {
    let status_text = match response.status_code {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        _ => "Internal Server Error",
    };
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nCache-Control: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status_code,
        status_text,
        response.content_type,
        response.cache_control,
        response.body.len()
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(&response.body))
        .and_then(|_| stream.flush())
        .map_err(|error| WebError::CommandFailed(format!("web response write failed: {error}")))
}

fn text_response(status_code: u16, body: &str) -> Response {
    Response {
        status_code,
        content_type: "text/plain; charset=utf-8",
        cache_control: CACHE_NO_STORE,
        body: body.as_bytes().to_vec(),
    }
}

fn json_response(status_code: u16, value: serde_json::Value) -> Result<Response, WebError> {
    Ok(Response {
        status_code,
        content_type: "application/json; charset=utf-8",
        cache_control: CACHE_NO_STORE,
        body: serde_json::to_vec(&value)
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    })
}

fn json_route_error(status_code: u16, error: &str) -> Result<Response, RouteError> {
    Ok(Response {
        status_code,
        content_type: "application/json; charset=utf-8",
        cache_control: CACHE_NO_STORE,
        body: serde_json::to_vec(&serde_json::json!({
            "ok": false,
            "error": error,
        }))
        .map_err(RouteError::Json)?,
    })
}

fn is_known_api_path(path: &str) -> bool {
    matches!(
        path,
        "/api/cockpit"
            | "/api/actions"
            | "/api/tasks"
            | "/api/push/config"
            | "/api/push/subscribe"
            | "/api/push/unsubscribe"
    ) || path.starts_with("/api/tasks/")
}

fn static_asset_cache_control(path: &str) -> &'static str {
    if path.starts_with("/icons/") {
        CACHE_IMMUTABLE
    } else {
        CACHE_REVALIDATE
    }
}

#[cfg(test)]
mod tests {
    use super::{
        route, route_with_bridge, serve_connection, ActionFailure, Request, RuntimeBridge,
    };
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::Config,
        models::OperatorAction,
        registry::InMemoryRegistry,
    };
    use std::cell::RefCell;
    use std::io::{Cursor, Read, Write};
    use std::rc::Rc;

    struct TestBridge {
        refreshed: bool,
        action: Option<(OperatorAction, String)>,
        action_calls: usize,
        action_result: Result<bool, ActionFailure>,
        start: Option<crate::slices::operate::StartTaskRequest>,
        start_result: Result<bool, ActionFailure>,
    }

    impl Default for TestBridge {
        fn default() -> Self {
            Self {
                refreshed: false,
                action: None,
                action_calls: 0,
                action_result: Ok(true),
                start: None,
                start_result: Ok(true),
            }
        }
    }

    impl RuntimeBridge<OkRunner> for TestBridge {
        fn refresh_cockpit(
            &mut self,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<bool, crate::WebError> {
            self.refreshed = true;
            Ok(false)
        }

        fn execute_mobile_action(
            &mut self,
            action: OperatorAction,
            task_handle: &str,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<bool, ActionFailure> {
            self.action_calls += 1;
            self.action = Some((action, task_handle.to_string()));
            self.action_result.clone()
        }

        fn execute_start_task(
            &mut self,
            request: crate::slices::operate::StartTaskRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<bool, ActionFailure> {
            self.start = Some(request);
            self.start_result.clone()
        }
    }

    struct OkRunner;

    impl CommandRunner for OkRunner {
        fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            Ok(CommandOutput {
                status_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

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

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ajax-web-runtime-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    fn serve_raw_request(path: &str) -> String {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("raw");
        let output = Rc::new(RefCell::new(Vec::new()));
        let stream = MockStream {
            input: Cursor::new(format!("GET {path} HTTP/1.1\r\nHost: ajax\r\n\r\n").into_bytes()),
            output: Rc::clone(&output),
        };

        serve_connection(stream, &mut context, &mut runner, &mut bridge, &dir).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        let written = String::from_utf8_lossy(&output.borrow()).into_owned();
        written
    }

    #[test]
    fn runtime_routes_to_vertical_slices() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let shell = route(
            Request {
                method: "GET",
                path: "/",
                body: "",
            },
            &context,
        )
        .unwrap();
        assert_eq!(shell.status_code, 200);
        assert_eq!(shell.content_type, "text/html; charset=utf-8");
        assert!(std::str::from_utf8(&shell.body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = route(
            Request {
                method: "GET",
                path: "/api/cockpit",
                body: "",
            },
            &context,
        )
        .unwrap();
        assert_eq!(cockpit.status_code, 200);
        assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
            serde_json::json!([])
        );
    }

    #[test]
    fn healthz_endpoint_reports_container_readiness() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let response = route(
            Request {
                method: "GET",
                path: "/healthz",
                body: "",
            },
            &context,
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "text/plain; charset=utf-8");
        assert_eq!(response.body, b"ok");
    }

    #[test]
    fn http_cache_policy_matches_pwa_runtime_contract() {
        let api = serve_raw_request("/api/cockpit");
        assert!(api.contains("Cache-Control: no-store"), "{api}");

        let health = serve_raw_request("/healthz");
        assert!(health.contains("Cache-Control: no-store"), "{health}");

        let shell = serve_raw_request("/");
        assert!(
            shell.contains("Cache-Control: no-cache, must-revalidate"),
            "{shell}"
        );

        let service_worker = serve_raw_request("/sw.js");
        assert!(
            service_worker.contains("Cache-Control: no-cache, must-revalidate"),
            "{service_worker}"
        );

        let manifest = serve_raw_request("/manifest.webmanifest");
        assert!(
            manifest.contains("Cache-Control: no-cache, must-revalidate"),
            "{manifest}"
        );

        let icon = serve_raw_request("/icons/icon-192.png");
        assert!(
            icon.contains("Cache-Control: public, max-age=31536000, immutable"),
            "{icon}"
        );
    }

    #[test]
    fn cockpit_api_refreshes_before_rendering() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("refresh");

        let response = route_with_bridge(
            Request {
                method: "GET",
                path: "/api/cockpit",
                body: "",
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert!(bridge.refreshed);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn action_endpoint_executes_bridge_action_and_returns_cockpit() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("action");

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/actions",
                body: r#"{"task_handle":"web/fix-login","action":"review","request_id":"req-action"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(body["status"], "succeeded");
        assert_eq!(body["state_changed"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            bridge.action,
            Some((OperatorAction::Review, "web/fix-login".to_string()))
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn operation_endpoint_returns_typed_operation_status() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("operation-status");

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/operations",
                body: r#"{"task_handle":"web/fix-login","action":"review","request_id":"req-1"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert_eq!(body["operation_id"], "web-req-1");
        assert_eq!(body["status"], "succeeded");
        assert_eq!(body["state_changed"], true);
        assert_eq!(bridge.action_calls, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_operation_request_id_is_idempotent() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("operation-idempotent");
        let body = r#"{"task_handle":"web/fix-login","action":"review","request_id":"req-same"}"#;

        let first = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/operations",
                body,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        let second = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/operations",
                body,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();

        assert_eq!(first.status_code, second.status_code);
        assert_eq!(first.body, second.body);
        assert_eq!(bridge.action_calls, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn conflicting_task_operation_is_blocked() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("operation-lock");
        std::fs::create_dir_all(super::operation_dir(&dir)).unwrap();
        std::fs::write(super::operation_lock_path(&dir, "web/fix-login"), "other").unwrap();
        let request_body =
            r#"{"task_handle":"web/fix-login","action":"drop","request_id":"req-drop"}"#;

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/operations",
                body: request_body,
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
        assert_eq!(body["status"], "blocked");
        assert_eq!(body["error"]["code"], "operation_in_progress");
        assert_eq!(bridge.action_calls, 0);
        std::fs::remove_file(super::operation_lock_path(&dir, "web/fix-login")).unwrap();

        let repeat = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/operations",
                body: request_body,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(repeat.status_code, 409);
        assert_eq!(repeat.body, response.body);
        assert_eq!(bridge.action_calls, 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn get_task_detail_returns_json_for_existing_handle() {
        use ajax_core::config::ManagedRepo;
        use ajax_core::models::{AgentClient, Task, TaskId};
        use ajax_core::registry::Registry as _;

        let config = ajax_core::config::Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..ajax_core::config::Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
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
            ))
            .unwrap();
        let context = CommandContext::new(config, registry);

        let response = route(
            Request {
                method: "GET",
                path: "/api/tasks/web/fix-login",
                body: "",
            },
            &context,
        )
        .unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["qualified_handle"], "web/fix-login");
        assert_eq!(body["title"], "Fix login");
        assert_eq!(body["branch"], "ajax/fix-login");
    }

    #[test]
    fn get_task_detail_returns_404_for_unknown_handle() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let response = route(
            Request {
                method: "GET",
                path: "/api/tasks/web/missing",
                body: "",
            },
            &context,
        )
        .unwrap();

        assert_eq!(response.status_code, 404);
    }

    #[test]
    fn api_errors_return_json_bodies() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let missing = route(
            Request {
                method: "GET",
                path: "/api/missing",
                body: "",
            },
            &context,
        )
        .unwrap();
        let missing_body: serde_json::Value = serde_json::from_slice(&missing.body).unwrap();
        assert_eq!(missing.status_code, 404);
        assert_eq!(missing.content_type, "application/json; charset=utf-8");
        assert_eq!(missing_body["ok"], false);
        assert_eq!(missing_body["error"], "not found");

        let wrong_method = route(
            Request {
                method: "POST",
                path: "/api/cockpit",
                body: "",
            },
            &context,
        )
        .unwrap();
        let wrong_method_body: serde_json::Value =
            serde_json::from_slice(&wrong_method.body).unwrap();
        assert_eq!(wrong_method.status_code, 405);
        assert_eq!(wrong_method.content_type, "application/json; charset=utf-8");
        assert_eq!(wrong_method_body["ok"], false);
        assert_eq!(wrong_method_body["error"], "method not allowed");
    }

    #[test]
    fn post_tasks_endpoint_delegates_to_start_bridge_method() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("start");

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/tasks",
                body: r#"{"repo":"web","title":"Fix login","agent":"codex","request_id":"req-start"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();

        assert_eq!(response.status_code, 200);
        assert_eq!(body["ok"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            bridge.start,
            Some(crate::slices::operate::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: "req-start".to_string(),
            })
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn action_endpoint_keeps_native_only_actions_out_of_bridge() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("native-action");

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/actions",
                body: r#"{"task_handle":"web/fix-login","action":"resume","request_id":"req-resume"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();

        assert_eq!(response.status_code, 409);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["status"], "needs_terminal");
        assert_eq!(body["error"]["code"], "needs_terminal");
        assert_eq!(bridge.action, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn push_config_and_subscribe_endpoints_round_trip() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("push");

        let config = route_with_bridge(
            Request {
                method: "GET",
                path: "/api/push/config",
                body: "",
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(config.status_code, 200);
        let config_body: serde_json::Value = serde_json::from_slice(&config.body).unwrap();
        assert_eq!(config_body["public_key"].as_array().map(Vec::len), Some(65));

        let subscribe = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/push/subscribe",
                body: r#"{"subscription":{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(subscribe.status_code, 200);
        assert_eq!(crate::adapters::push::load_subscriptions(&dir).len(), 1);

        let unsubscribe = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/push/unsubscribe",
                body: r#"{"endpoint":"https://push.example/x"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(unsubscribe.status_code, 200);
        assert!(crate::adapters::push::load_subscriptions(&dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn runtime_serves_a_request_over_a_generic_stream() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("stream");
        let output = Rc::new(RefCell::new(Vec::new()));
        let stream = MockStream {
            input: Cursor::new(b"GET /app.css HTTP/1.1\r\nHost: ajax\r\n\r\n".to_vec()),
            output: Rc::clone(&output),
        };

        serve_connection(stream, &mut context, &mut runner, &mut bridge, &dir).unwrap();

        let written = String::from_utf8_lossy(&output.borrow()).into_owned();
        assert!(written.starts_with("HTTP/1.1 200 OK"), "{written}");
        assert!(written.contains("Content-Type: text/css"), "{written}");
        assert!(
            written.contains("Cache-Control: no-cache, must-revalidate"),
            "{written}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
