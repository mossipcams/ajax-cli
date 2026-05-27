//! Web companion runtime wiring.

use ajax_core::{
    adapters::CommandRunner,
    commands::{self, CommandContext},
    models::OperatorAction,
    output::CockpitView,
    registry::{InMemoryRegistry, Registry},
};
use axum::{
    body::Bytes,
    extract::{Path as AxumPath, State},
    http::Uri,
    response::Response as AxumResponse,
    routing::{get, post},
    serve::Listener,
    Json, Router,
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap},
    net::{SocketAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tower_http::trace::TraceLayer;

use crate::{
    action_vocabulary::{supported_web_action, SYNC_ACTION},
    adapters::{push, tls},
    slices::{attention, cockpit, install},
    WebError,
};

pub use crate::adapters::http::{Request, Response};

use crate::adapters::http::{
    bytes_axum_response, html_response, json_response, json_value_response,
    operation_response_with_request_id, response_from_web_error, text_axum_response, text_response,
    web_error_response,
};

pub struct WebAppState<C, B> {
    shared: Arc<Mutex<WebSharedState<C, B>>>,
    operations: Arc<Mutex<OperationCoordinator>>,
    state_dir: Arc<PathBuf>,
}

struct WebSharedState<C, B> {
    context: CommandContext<InMemoryRegistry>,
    runner: C,
    bridge: B,
}

impl<C, B> Clone for WebAppState<C, B> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            operations: Arc::clone(&self.operations),
            state_dir: Arc::clone(&self.state_dir),
        }
    }
}

impl<C, B> WebAppState<C, B> {
    pub fn new(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        state_dir: PathBuf,
    ) -> Self {
        Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            state_dir: Arc::new(state_dir),
        }
    }
}

#[derive(Default)]
struct OperationCoordinator {
    completed: HashMap<String, Response>,
    in_flight_requests: BTreeSet<String>,
    in_flight_tasks: BTreeSet<String>,
}

pub fn axum_app<C, B>(state: WebAppState<C, B>) -> Router
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    Router::new()
        .route("/", get(axum_pwa_shell))
        .route("/index.html", get(axum_pwa_shell))
        .route("/app.css", get(axum_app_css))
        .route("/app.js", get(axum_app_js))
        .route("/manifest.webmanifest", get(axum_manifest))
        .route("/sw.js", get(axum_service_worker))
        .route("/icons/{*path}", get(axum_icon))
        .route("/api/health", get(axum_health))
        .route("/api/cockpit", get(axum_cockpit::<C, B>))
        .route("/api/tasks/{*handle}", get(axum_task_detail::<C, B>))
        .route("/api/tasks", post(axum_start_task::<C, B>))
        .route("/api/actions", post(axum_action::<C, B>))
        .route("/api/operations", post(axum_action::<C, B>))
        .route("/api/push/config", get(axum_push_config::<C, B>))
        .route("/api/push/subscribe", post(axum_push_subscribe::<C, B>))
        .route("/api/push/unsubscribe", post(axum_push_unsubscribe::<C, B>))
        .fallback(axum_fallback)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub fn serve_axum_web<C, B>(host: &str, port: u16, state: WebAppState<C, B>) -> Result<(), WebError>
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let identity = tls::load_or_create_identity(&state.state_dir)?;
    let address = resolve_bind_address(host, port)?;
    eprintln!("Ajax mobile web listening on https://{host}:{port}");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| WebError::CommandFailed(format!("web runtime failed: {error}")))?;

    runtime.block_on(async move {
        let tls_config = tls::tls_server_config(&identity)?;
        let tcp_listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| WebError::CommandFailed(format!("web bind failed: {error}")))?;
        let tls_listener = TlsListener {
            listener: tcp_listener,
            acceptor: tokio_rustls::TlsAcceptor::from(tls_config),
        };
        tokio::spawn(run_attention_poller_for_state(state.clone()));
        axum::serve(tls_listener, axum_app(state))
            .await
            .map_err(|error| WebError::CommandFailed(format!("web server failed: {error}")))
    })
}

struct TlsListener {
    listener: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
}

impl Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (stream, address) = match self.listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    eprintln!("Ajax web accept error: {error}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };
            match self.acceptor.accept(stream).await {
                Ok(stream) => return (stream, address),
                Err(error) => {
                    eprintln!("Ajax web TLS handshake error from {address}: {error}");
                    continue;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

fn resolve_bind_address(host: &str, port: u16) -> Result<SocketAddr, WebError> {
    (host, port)
        .to_socket_addrs()
        .map_err(|error| WebError::CommandFailed(format!("web bind address failed: {error}")))?
        .next()
        .ok_or_else(|| {
            WebError::CommandFailed(format!("web bind address unresolved: {host}:{port}"))
        })
}

async fn run_attention_poller_for_state<C, B>(state: WebAppState<C, B>)
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let initial = {
        let mut guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        refresh_attention_handles(&mut guard)
    };
    let mut notifier = attention::AttentionNotifier::seeded_with(initial);

    loop {
        tokio::time::sleep(ATTENTION_POLL_INTERVAL).await;
        let current = {
            let mut guard = state
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            refresh_attention_handles(&mut guard)
        };
        for handle in notifier.poll(current) {
            let notification = push::PushNotification {
                title: "Ajax task needs attention".to_string(),
                body: handle.clone(),
                tag: handle.clone(),
                task_handle: handle.clone(),
            };
            if let Err(error) = push::send_to_all(&state.state_dir, &notification) {
                eprintln!("Ajax web push notification failed: {error}");
            }
        }
    }
}

fn refresh_attention_handles<C, B>(guard: &mut WebSharedState<C, B>) -> BTreeSet<String>
where
    C: CommandRunner,
    B: RuntimeBridge<C>,
{
    let WebSharedState {
        context,
        runner,
        bridge,
    } = guard;
    if let Err(error) = bridge.refresh_cockpit(context, runner) {
        eprintln!("Ajax web attention refresh failed: {error}");
    }
    attention_handles(&commands::cockpit_view(context))
}

async fn axum_pwa_shell() -> AxumResponse {
    html_response(install::pwa_shell().as_bytes().to_vec())
}

async fn axum_app_css() -> AxumResponse {
    static_asset_response("/app.css")
}

async fn axum_app_js() -> AxumResponse {
    static_asset_response("/app.js")
}

async fn axum_manifest() -> AxumResponse {
    static_asset_response("/manifest.webmanifest")
}

async fn axum_service_worker() -> AxumResponse {
    static_asset_response("/sw.js")
}

async fn axum_icon(AxumPath(path): AxumPath<String>) -> AxumResponse {
    static_asset_response(&format!("/icons/{path}"))
}

async fn axum_health() -> AxumResponse {
    json_value_response(200, serde_json::json!({ "ok": true }))
}

async fn axum_cockpit<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let mut guard = state
        .shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let WebSharedState {
        context,
        runner,
        bridge,
    } = &mut *guard;
    match handle_refreshed_cockpit_request(context, runner, bridge) {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

async fn axum_task_detail<C, B>(
    State(state): State<WebAppState<C, B>>,
    AxumPath(handle): AxumPath<String>,
) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let guard = state
        .shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match cockpit::browser_task_detail_view(&guard.context, &handle) {
        Some(detail) => json_value_response(200, serde_json::to_value(detail).unwrap_or_default()),
        None => json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "task not found" }),
        ),
    }
}

async fn axum_start_task<C, B>(
    State(state): State<WebAppState<C, B>>,
    Json(request): Json<crate::slices::operate::StartTaskRequest>,
) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let mut guard = state
        .shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let WebSharedState {
        context,
        runner,
        bridge,
    } = &mut *guard;
    match handle_start_task_request(
        &serde_json::to_string(&request).unwrap_or_default(),
        context,
        runner,
        bridge,
    ) {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

async fn axum_action<C, B>(State(state): State<WebAppState<C, B>>, body: Bytes) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    let request: MobileActionRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return json_value_response(
                400,
                serde_json::json!({
                    "ok": false,
                    "error": format!("json parse failed: {error}"),
                }),
            );
        }
    };
    let request_id = request.request_id.clone();
    let task_key = request.task_handle.clone();
    {
        let mut operations = state
            .operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(request_id) = request_id.as_ref() {
            if let Some(response) = operations.completed.get(request_id) {
                return response.clone().into_axum_response();
            }
            if !operations.in_flight_requests.insert(request_id.clone()) {
                return json_value_response(
                    409,
                    serde_json::json!({
                        "ok": false,
                        "request_id": request_id,
                        "error": "operation already in progress",
                    }),
                );
            }
        }
        if !operations.in_flight_tasks.insert(task_key.clone()) {
            if let Some(request_id) = request_id.as_ref() {
                operations.in_flight_requests.remove(request_id);
            }
            return json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": "task operation already in progress",
                }),
            );
        }
    }

    let mut guard = state
        .shared
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let WebSharedState {
        context,
        runner,
        bridge,
    } = &mut *guard;
    let response = match handle_action_request(
        &serde_json::to_string(&request).unwrap_or_default(),
        context,
        runner,
        bridge,
    ) {
        Ok(response) => operation_response_with_request_id(response, request_id.as_deref()),
        Err(error) => response_from_web_error(error, request_id.as_deref()),
    };
    drop(guard);

    let mut operations = state
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations.in_flight_tasks.remove(&task_key);
    if let Some(request_id) = request_id.as_ref() {
        operations.in_flight_requests.remove(request_id);
        operations
            .completed
            .insert(request_id.clone(), response.clone());
    }

    response.into_axum_response()
}

async fn axum_push_config<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    match handle_push_config(&state.state_dir) {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

async fn axum_push_subscribe<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: String,
) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    match handle_push_subscribe(&body, &state.state_dir) {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

async fn axum_push_unsubscribe<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: String,
) -> AxumResponse
where
    C: CommandRunner + Send + 'static,
    B: RuntimeBridge<C> + Send + 'static,
{
    match handle_push_unsubscribe(&body, &state.state_dir) {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

async fn axum_fallback(uri: Uri) -> AxumResponse {
    if uri.path().starts_with("/api/") {
        return json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "not found" }),
        );
    }
    text_axum_response(404, "not found")
}

fn static_asset_response(path: &str) -> AxumResponse {
    match install::static_asset(path) {
        Some(asset) => bytes_axum_response(200, asset.content_type, asset.body.to_vec()),
        None => text_axum_response(404, "not found"),
    }
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
    fn refresh_cockpit(
        &mut self,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<bool, WebError>;

    fn execute_operate(
        &mut self,
        request: crate::slices::operate::OperateRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<crate::slices::operate::OperateOutcome, ActionFailure>;

    fn execute_start_task(
        &mut self,
        request: crate::slices::operate::StartTaskRequest,
        context: &mut CommandContext<InMemoryRegistry>,
        runner: &mut C,
    ) -> Result<crate::slices::operate::OperateOutcome, ActionFailure>;
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct MobileActionRequest {
    #[serde(default)]
    request_id: Option<String>,
    task_handle: String,
    action: String,
}

pub fn route<R: Registry>(
    request: Request<'_>,
    context: &CommandContext<R>,
) -> Result<Response, RouteError> {
    let path = request.path.split('?').next().unwrap_or(request.path);
    match (request.method, path) {
        ("GET", "/") => Ok(Response {
            status_code: 200,
            content_type: "text/html; charset=utf-8",
            body: install::pwa_shell().as_bytes().to_vec(),
        }),
        ("GET", "/api/cockpit") => Ok(Response {
            status_code: 200,
            content_type: "application/json; charset=utf-8",
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
                    body: body.into_bytes(),
                }),
                Some(Err(error)) => Err(RouteError::Json(error)),
                None => Ok(text_response(404, "task not found")),
            }
        }
        ("GET", asset_path) => match install::static_asset(asset_path) {
            Some(asset) => Ok(Response {
                status_code: 200,
                content_type: asset.content_type,
                body: asset.body.to_vec(),
            }),
            None => Ok(text_response(404, "not found")),
        },
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
            handle_action_request(request.body, context, runner, bridge)
        }
        ("POST", "/api/tasks") => handle_start_task_request(request.body, context, runner, bridge),
        ("GET", "/api/push/config") => handle_push_config(state_dir),
        ("POST", "/api/push/subscribe") => handle_push_subscribe(request.body, state_dir),
        ("POST", "/api/push/unsubscribe") => handle_push_unsubscribe(request.body, state_dir),
        _ => route(request, context).map_err(|error| match error {
            RouteError::Json(error) => WebError::JsonSerialization(error.to_string()),
        }),
    }
}

fn handle_refreshed_cockpit_request<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    let _state_changed = bridge.refresh_cockpit(context, runner)?;
    json_response(
        200,
        serde_json::to_value(cockpit::browser_cockpit_view(context))
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    )
}

fn handle_action_request<C: CommandRunner>(
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    let request: MobileActionRequest = serde_json::from_str(body)
        .map_err(|error| WebError::JsonSerialization(error.to_string()))?;

    if let Some(failure) = unsupported_operate_action(&request.action) {
        return operation_error_response(failure, context);
    }

    match bridge.execute_operate(
        crate::slices::operate::OperateRequest {
            task_handle: request.task_handle,
            action: request.action,
        },
        context,
        runner,
    ) {
        Ok(outcome) => operation_success_response(outcome, context),
        Err(error) => operation_error_response(error, context),
    }
}

fn operation_success_response(
    outcome: crate::slices::operate::OperateOutcome,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<Response, WebError> {
    json_response(
        200,
        serde_json::json!({
            "ok": true,
            "state_changed": outcome.state_changed,
            "output": outcome.output,
            "cockpit": cockpit::browser_cockpit_view(context),
        }),
    )
}

fn operation_error_response(
    error: ActionFailure,
    context: &CommandContext<InMemoryRegistry>,
) -> Result<Response, WebError> {
    json_response(
        409,
        serde_json::json!({
            "ok": false,
            "error": error.message,
            "state_changed": error.state_changed,
            "cockpit": cockpit::browser_cockpit_view(context),
        }),
    )
}

fn unsupported_operate_action(action: &str) -> Option<ActionFailure> {
    if action == SYNC_ACTION {
        return None;
    }
    let operator_action = OperatorAction::from_label(action)?;
    if supported_web_action(operator_action) {
        return None;
    }
    let message = match operator_action {
        OperatorAction::Resume => "resume requires native cockpit; use sync instead".to_string(),
        OperatorAction::Start => {
            "start uses the dedicated Web Cockpit new-task operation".to_string()
        }
        _ => format!("unsupported action: {action}"),
    };
    Some(ActionFailure {
        message,
        state_changed: false,
    })
}

fn handle_start_task_request<C: CommandRunner>(
    body: &str,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    let request: crate::slices::operate::StartTaskRequest = serde_json::from_str(body)
        .map_err(|error| WebError::JsonSerialization(error.to_string()))?;
    match bridge.execute_start_task(request, context, runner) {
        Ok(outcome) => operation_success_response(outcome, context),
        Err(error) => operation_error_response(error, context),
    }
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
    let subscription = match parse_push_subscription(body) {
        Ok(subscription) => subscription,
        Err(error) => {
            return json_response(
                400,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            );
        }
    };
    push::add_subscription(state_dir, subscription)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

fn parse_push_subscription(body: &str) -> Result<push::PushSubscription, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    if let Some(subscription) = value.get("subscription") {
        serde_json::from_value(subscription.clone())
    } else {
        serde_json::from_value(value)
    }
}

fn handle_push_unsubscribe(body: &str, state_dir: &Path) -> Result<Response, WebError> {
    let request: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let Some(endpoint) = request.get("endpoint").and_then(serde_json::Value::as_str) else {
        return json_response(
            400,
            serde_json::json!({ "ok": false, "error": "endpoint is required" }),
        );
    };
    push::remove_subscription(state_dir, endpoint)?;
    json_response(200, serde_json::json!({ "ok": true }))
}

const ATTENTION_POLL_INTERVAL: Duration = Duration::from_secs(15);

fn attention_handles(view: &CockpitView) -> BTreeSet<String> {
    view.inbox
        .items
        .iter()
        .map(|item| item.task_handle.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{route, route_with_bridge, ActionFailure, Request, RuntimeBridge};
    use crate::slices::operate::{OperateOutcome, OperateRequest};
    use ajax_core::{
        adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
        commands::CommandContext,
        config::Config,
        registry::InMemoryRegistry,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{Request as AxumRequest, StatusCode},
    };
    use std::time::Duration;
    use tower::ServiceExt;

    struct TestBridge {
        refreshed: bool,
        operate: Option<OperateRequest>,
        operate_count: usize,
        operate_delay: Duration,
        operate_result: Result<OperateOutcome, ActionFailure>,
        start: Option<crate::slices::operate::StartTaskRequest>,
        start_result: Result<OperateOutcome, ActionFailure>,
    }

    impl Default for TestBridge {
        fn default() -> Self {
            Self {
                refreshed: false,
                operate: None,
                operate_count: 0,
                operate_delay: Duration::ZERO,
                operate_result: Ok(OperateOutcome {
                    state_changed: true,
                    output: String::new(),
                }),
                start: None,
                start_result: Ok(OperateOutcome {
                    state_changed: true,
                    output: String::new(),
                }),
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

        fn execute_operate(
            &mut self,
            request: OperateRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.operate_count += 1;
            std::thread::sleep(self.operate_delay);
            self.operate = Some(request);
            self.operate_result.clone()
        }

        fn execute_start_task(
            &mut self,
            request: crate::slices::operate::StartTaskRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<OperateOutcome, ActionFailure> {
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

    #[tokio::test]
    async fn axum_router_serves_static_shell_and_cockpit_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-static"),
        );
        let app = super::axum_app(state);

        let shell = app
            .clone()
            .oneshot(AxumRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
        let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&shell_body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(
            cockpit.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let cockpit_body = to_bytes(cockpit.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit_body).unwrap()["cards"],
            serde_json::json!([])
        );

        let missing_api = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/missing")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_api.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            missing_api.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let missing_api_body = to_bytes(missing_api.into_body(), usize::MAX).await.unwrap();
        assert!(!std::str::from_utf8(&missing_api_body)
            .unwrap()
            .contains("Ajax Cockpit"));
    }

    #[tokio::test]
    async fn axum_operations_are_idempotent_by_request_id() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-idempotency"),
        );
        let app = super::axum_app(state.clone());

        let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
        let first = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/operations")
                    .header("content-type", "application/json")
                    .body(Body::from(operation))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let first_body = to_bytes(first.into_body(), usize::MAX).await.unwrap();
        let first_json: serde_json::Value = serde_json::from_slice(&first_body).unwrap();
        assert_eq!(first_json["ok"], true);
        assert_eq!(first_json["request_id"], "req-1");

        let second = app
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/operations")
                    .header("content-type", "application/json")
                    .body(Body::from(operation))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::OK);
        let second_body = to_bytes(second.into_body(), usize::MAX).await.unwrap();
        let second_json: serde_json::Value = serde_json::from_slice(&second_body).unwrap();
        assert_eq!(second_json, first_json);

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.operate_count, 1);
    }

    #[tokio::test]
    async fn axum_operation_parse_errors_are_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-json-error"),
        );
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/operations")
                    .header("content-type", "application/json")
                    .body(Body::from("{not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert!(json["error"].as_str().unwrap_or_default().contains("json"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_blocks_conflicting_task_operations() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let bridge = TestBridge {
            operate_delay: Duration::from_millis(150),
            ..TestBridge::default()
        };
        let state =
            super::WebAppState::new(context, OkRunner, bridge, scratch_dir("axum-conflict"));
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    AxumRequest::builder()
                        .method("POST")
                        .uri("/api/operations")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let conflict = app
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/operations")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"req-b","task_handle":"web/fix-login","action":"ship"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = to_bytes(conflict.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "req-b");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already in progress"));

        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.operate_count, 1);
    }

    #[test]
    fn production_server_uses_axum_instead_of_manual_http_loop() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime.rs"),
        )
        .unwrap();
        let server_fn = source
            .split("pub fn serve_axum_web")
            .nth(1)
            .and_then(|tail| tail.split("fn resolve_bind_address").next())
            .unwrap();

        assert!(server_fn.contains("axum::serve"));
        assert!(server_fn.contains("TlsListener"));
        assert!(server_fn.contains("axum_app"));
        assert!(!server_fn.contains("listener.incoming()"));
        assert!(!server_fn.contains("serve_tls_connection"));
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
                body: r#"{"task_handle":"web/fix-login","action":"review"}"#,
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
        assert_eq!(body["state_changed"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            bridge.operate,
            Some(OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "review".to_string(),
            })
        );
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
    fn post_tasks_endpoint_delegates_to_start_bridge_method() {
        let mut context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let mut runner = OkRunner;
        let mut bridge = TestBridge::default();
        let dir = scratch_dir("start");

        let response = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/tasks",
                body: r#"{"repo":"web","title":"Fix login","agent":"codex"}"#,
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
                request_id: String::new(),
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
                body: r#"{"task_handle":"web/fix-login","action":"resume"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();

        assert_eq!(response.status_code, 409);
        assert_eq!(bridge.operate, None);
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
                body: r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(subscribe.status_code, 200);
        assert_eq!(crate::adapters::push::load_subscriptions(&dir).len(), 1);

        let wrapped = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/push/subscribe",
                body: r#"{"subscription":{"endpoint":"https://push.example/y","keys":{"p256dh":"k2","auth":"a2"}}}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(wrapped.status_code, 200);
        assert_eq!(crate::adapters::push::load_subscriptions(&dir).len(), 2);

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
        assert_eq!(crate::adapters::push::load_subscriptions(&dir).len(), 1);

        let unsubscribe_wrapped = route_with_bridge(
            Request {
                method: "POST",
                path: "/api/push/unsubscribe",
                body: r#"{"endpoint":"https://push.example/y"}"#,
            },
            &mut context,
            &mut runner,
            &mut bridge,
            &dir,
        )
        .unwrap();
        assert_eq!(unsubscribe_wrapped.status_code, 200);
        assert!(crate::adapters::push::load_subscriptions(&dir).is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn runtime_keeps_custom_connection_serving_out_of_production() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/runtime.rs"),
        )
        .unwrap();

        assert!(!source.contains(&["pub fn ", "serve_connection"].concat()));
        assert!(!source.contains(&["fn ", "serve_tls_connection"].concat()));
        assert!(!source.contains(&["fn ", "write_http_response"].concat()));
    }
}
