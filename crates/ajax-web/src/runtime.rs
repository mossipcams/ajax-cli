//! Web companion runtime wiring.

use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, models::OperatorAction,
    registry::InMemoryRegistry, runtime_refresh::RefreshTier,
};
use axum::{
    body::Bytes,
    extract::{
        ws::WebSocketUpgrade, FromRequestParts, Path as AxumPath, Request as AxumRequest, State,
    },
    http::{header, Uri},
    middleware::{from_fn_with_state, Next},
    response::Response as AxumResponse,
    routing::{get, post},
    serve::Listener,
    Json, Router,
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tower_http::trace::TraceLayer;

use crate::{
    action_vocabulary::supported_web_action,
    adapters::{browser_session::BrowserSession, server, tls},
    slices::{cockpit, install},
    WebError,
};

pub use crate::adapters::http::{Request, Response};

use crate::adapters::http::{
    bytes_axum_response, html_response, json_response, json_value_response,
    operation_response_with_request_id, response_from_web_error, text_axum_response,
    web_error_response,
};

const COCKPIT_REFRESH_CACHE_TTL: Duration = Duration::from_millis(750);
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_COMPLETED_OPERATIONS: usize = 128;

pub struct WebAppState<C, B> {
    shared: Arc<Mutex<WebSharedState<C, B>>>,
    operations: Arc<Mutex<OperationCoordinator>>,
    cockpit_refresh_lock: Arc<tokio::sync::Mutex<()>>,
    state_dir: Arc<PathBuf>,
    browser_session: Arc<BrowserSession>,
}

struct WebSharedState<C, B> {
    context: CommandContext<InMemoryRegistry>,
    runner: C,
    bridge: B,
    revision: u64,
    cockpit_cache: Option<CockpitCacheEntry>,
}

#[derive(Clone)]
struct CockpitCacheEntry {
    response: Response,
    cached_at: Instant,
    revision: u64,
}

impl<C, B> Clone for WebAppState<C, B> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            operations: Arc::clone(&self.operations),
            cockpit_refresh_lock: Arc::clone(&self.cockpit_refresh_lock),
            state_dir: Arc::clone(&self.state_dir),
            browser_session: Arc::clone(&self.browser_session),
        }
    }
}

impl<C, B> WebAppState<C, B>
where
    C: Clone,
    B: Clone,
{
    /// Run `operate` against a clone of the shared state without holding the
    /// `shared` lock across the call, then commit the result only if no other
    /// request advanced the revision in the meantime. A losing writer leaves
    /// shared state untouched and returns a `409` conflict instead.
    fn run_optimistic(
        &self,
        request_id: Option<&str>,
        conflict_message: &str,
        operate: impl FnOnce(&mut CommandContext<InMemoryRegistry>, &mut C, &mut B) -> Response,
    ) -> Response {
        let (mut context, mut runner, mut bridge, base_revision) = {
            let guard = self
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        let response = operate(&mut context, &mut runner, &mut bridge);
        let mut guard = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.revision == base_revision {
            guard.context = context;
            guard.runner = runner;
            guard.bridge = bridge;
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
            response
        } else {
            operation_response_with_request_id(
                json_response(
                    409,
                    serde_json::json!({ "ok": false, "error": conflict_message }),
                )
                .unwrap_or_else(|error| response_from_web_error(error, request_id)),
                request_id,
            )
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
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            cockpit_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            state_dir: Arc::new(state_dir),
            browser_session: Arc::new(BrowserSession::test_default()),
        }
    }

    pub fn load_or_create(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        state_dir: PathBuf,
    ) -> Result<Self, WebError> {
        let browser_session = BrowserSession::load_or_create(&state_dir)?;
        Ok(Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            cockpit_refresh_lock: Arc::new(tokio::sync::Mutex::new(())),
            state_dir: Arc::new(state_dir),
            browser_session: Arc::new(browser_session),
        })
    }

    fn cached_cockpit_response(&self) -> Option<Response> {
        let guard = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cache = guard.cockpit_cache.as_ref()?;
        if cache.revision != guard.revision {
            return None;
        }
        if cache.cached_at.elapsed() > COCKPIT_REFRESH_CACHE_TTL {
            return None;
        }
        Some(cache.response.clone())
    }
}

#[derive(Default)]
struct OperationCoordinator {
    completed: HashMap<String, Response>,
    completed_request_ids: VecDeque<String>,
    in_flight_requests: BTreeSet<String>,
    in_flight_tasks: BTreeSet<String>,
}

impl OperationCoordinator {
    fn completed_response(&self, request_id: &str) -> Option<Response> {
        self.completed.get(request_id).cloned()
    }

    fn has_in_flight_mutation(&self) -> bool {
        !self.in_flight_requests.is_empty() || !self.in_flight_tasks.is_empty()
    }

    fn store_completed_response(&mut self, request_id: String, response: Response) {
        if self
            .completed
            .insert(request_id.clone(), response)
            .is_some()
        {
            self.completed_request_ids
                .retain(|completed_id| completed_id != &request_id);
        }
        self.completed_request_ids.push_back(request_id);
        while self.completed_request_ids.len() > MAX_COMPLETED_OPERATIONS {
            if let Some(oldest_request_id) = self.completed_request_ids.pop_front() {
                self.completed.remove(&oldest_request_id);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApiAccess {
    Public,
    BrowserSessionRequired,
}

fn api_access_policy(method: &str, path: &str) -> ApiAccess {
    if !path.starts_with("/api/") {
        return ApiAccess::Public;
    }
    match (method, path) {
        ("GET", "/api/health") | ("POST", "/api/session") => ApiAccess::Public,
        _ => ApiAccess::BrowserSessionRequired,
    }
}

pub fn axum_app<C, B>(state: WebAppState<C, B>) -> Router
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let session_state = state.clone();
    Router::new()
        .route("/", get(axum_browser_shell::<C, B>))
        .route("/index.html", get(axum_browser_shell::<C, B>))
        .route("/app.css", get(axum_app_css))
        .route("/app.js", get(axum_app_js))
        .route("/api/health", get(axum_health))
        .route("/api/session", post(axum_browser_session::<C, B>))
        .route("/api/version", get(axum_version))
        .route("/api/server/restart", post(axum_server_restart))
        .route("/api/cockpit", get(axum_cockpit::<C, B>))
        .route("/api/tasks", post(axum_start_task::<C, B>))
        .route(
            "/api/tasks/{*handle}",
            get(axum_task_get::<C, B>).post(axum_task_post::<C, B>),
        )
        .route("/api/actions", post(axum_action::<C, B>))
        .route("/api/operations", post(axum_action::<C, B>))
        .fallback(axum_fallback)
        .layer(from_fn_with_state(
            session_state,
            require_browser_session::<C, B>,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub fn serve_axum_web<C, B>(host: &str, port: u16, state: WebAppState<C, B>) -> Result<(), WebError>
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let identity = tls::load_or_create_identity(&state.state_dir)?;
    let address = resolve_bind_address(host, port)?;
    eprintln!("Ajax mobile web listening on https://{host}:{port}");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| WebError::CommandFailed(format!("web runtime failed: {error}")))?;

    // Kill any ephemeral per-client terminal sessions left behind by a bridge
    // that crashed before it could tear its own session down.
    crate::adapters::terminal_pty::reap_orphan_terminal_sessions();

    runtime.block_on(async move {
        let tls_config = tls::tls_server_config(&identity)?;
        let tcp_listener = tokio::net::TcpListener::bind(address)
            .await
            .map_err(|error| WebError::CommandFailed(format!("web bind failed: {error}")))?;
        let (accepted_tls_tx, accepted_tls_rx) = tokio::sync::mpsc::channel(1024);
        let tls_listener = TlsListener {
            listener: tcp_listener,
            acceptor: tokio_rustls::TlsAcceptor::from(tls_config),
            accepted_tls_tx,
            accepted_tls_rx,
        };
        axum::serve(tls_listener, axum_app(state))
            .await
            .map_err(|error| WebError::CommandFailed(format!("web server failed: {error}")))
    })
}

struct TlsListener {
    listener: tokio::net::TcpListener,
    acceptor: tokio_rustls::TlsAcceptor,
    accepted_tls_tx: tokio::sync::mpsc::Sender<(
        tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
        SocketAddr,
    )>,
    accepted_tls_rx: tokio::sync::mpsc::Receiver<(
        tokio_rustls::server::TlsStream<tokio::net::TcpStream>,
        SocketAddr,
    )>,
}

impl Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            tokio::select! {
                accepted = self.accepted_tls_rx.recv() => {
                    if let Some((stream, address)) = accepted {
                        return (stream, address);
                    }
                }
                accepted = self.listener.accept() => {
                    let (stream, address) = match accepted {
                        Ok(accepted) => accepted,
                        Err(error) => {
                            eprintln!("Ajax web accept error: {error}");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };
                    let acceptor = self.acceptor.clone();
                    let accepted_tls_tx = self.accepted_tls_tx.clone();
                    tokio::spawn(async move {
                        match tokio::time::timeout(TLS_HANDSHAKE_TIMEOUT, acceptor.accept(stream)).await {
                            Ok(Ok(stream)) => {
                                let _ = accepted_tls_tx.send((stream, address)).await;
                            }
                            Ok(Err(error)) => {
                                eprintln!("Ajax web TLS handshake error from {address}: {error}");
                            }
                            Err(_) => {
                                eprintln!("Ajax web TLS handshake timeout from {address}");
                            }
                        }
                    });
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

async fn axum_browser_shell<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse {
    let mut response = html_response(install::browser_shell().into_bytes());
    state
        .browser_session
        .apply_set_cookie(response.headers_mut());
    response
}

async fn axum_browser_session<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse {
    browser_session_json_response(&state.browser_session)
}

fn browser_session_json_response(session: &BrowserSession) -> AxumResponse {
    let mut response = json_value_response(200, serde_json::json!({ "ok": true }));
    session.apply_set_cookie(response.headers_mut());
    response
}

async fn require_browser_session<C, B>(
    State(state): State<WebAppState<C, B>>,
    request: AxumRequest,
    next: Next,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let path = request.uri().path();
    if api_access_policy(request.method().as_str(), path) == ApiAccess::Public {
        return next.run(request).await;
    }
    if state.browser_session.is_present(request.headers()) {
        return next.run(request).await;
    }
    json_value_response(
        401,
        serde_json::json!({ "ok": false, "error": "browser session required" }),
    )
}

async fn axum_app_css() -> AxumResponse {
    static_asset_response("/app.css")
}

async fn axum_app_js() -> AxumResponse {
    static_asset_response("/app.js")
}

async fn axum_health() -> AxumResponse {
    json_value_response(200, serde_json::json!({ "ok": true }))
}

async fn axum_version() -> AxumResponse {
    json_value_response(
        200,
        serde_json::json!({ "version": install::app_version() }),
    )
}

async fn axum_server_restart() -> AxumResponse {
    handle_server_restart().into_axum_response()
}

fn handle_server_restart() -> Response {
    server::schedule_process_restart();
    Response {
        status_code: 200,
        content_type: "application/json; charset=utf-8",
        body: br#"{"ok":true,"restarting":true}"#.to_vec(),
    }
}

async fn axum_cockpit<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    let _refresh_guard = state.cockpit_refresh_lock.lock().await;
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    let mut refresh_session = {
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        CockpitRefreshSession {
            context: guard.context.clone(),
            runner: guard.runner.clone(),
            bridge: guard.bridge.clone(),
            revision: guard.revision,
        }
    };
    let result = handle_refreshed_cockpit_request(
        &mut refresh_session.context,
        &mut refresh_session.runner,
        &mut refresh_session.bridge,
    );
    let cached_response = match &result {
        Ok(response) => Some(response.clone()),
        Err(_) => None,
    };
    {
        let mut guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.revision == refresh_session.revision {
            guard.context = refresh_session.context;
            guard.bridge = refresh_session.bridge;
            if let Some(response) = cached_response {
                guard.cockpit_cache = Some(CockpitCacheEntry {
                    response,
                    cached_at: Instant::now(),
                    revision: guard.revision,
                });
            }
        }
    }
    match result {
        Ok(response) => response.into_axum_response(),
        Err(error) => web_error_response(error),
    }
}

struct CockpitRefreshSession<C, B> {
    context: CommandContext<InMemoryRegistry>,
    runner: C,
    bridge: B,
    revision: u64,
}

async fn axum_task_detail<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
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

async fn axum_task_get<C, B>(
    State(state): State<WebAppState<C, B>>,
    AxumPath(handle): AxumPath<String>,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if let Some(task_handle) = handle.strip_suffix("/terminal") {
        return axum_task_terminal(State(state), task_handle.to_string(), req).await;
    }
    if let Some(task_handle) = handle.strip_suffix("/snapshot") {
        let since = req
            .uri()
            .query()
            .and_then(|query| query_param(query, "since"))
            .map(|value| value.to_string());
        return axum_task_snapshot(State(state), task_handle.to_string(), since).await;
    }
    axum_task_detail::<C, B>(State(state), handle).await
}

async fn axum_task_terminal<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if !req
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return text_axum_response(400, "websocket upgrade required");
    }

    let plan = {
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match crate::slices::terminal::prepare_task_terminal(&guard.context, &handle) {
            Ok(plan) => plan,
            Err(crate::slices::terminal::TerminalRouteError::TaskNotFound) => {
                return json_value_response(
                    404,
                    serde_json::json!({ "ok": false, "error": "task not found" }),
                );
            }
            Err(crate::slices::terminal::TerminalRouteError::SessionMissing) => {
                return json_value_response(
                    409,
                    serde_json::json!({ "ok": false, "error": "tmux session missing" }),
                );
            }
        }
    };

    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::adapters::terminal_pty::bridge_task_terminal_socket(socket, plan).await;
    })
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

async fn axum_task_snapshot<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    since: Option<String>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let (context, mut runner) = {
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (guard.context.clone(), guard.runner.clone())
    };

    match crate::slices::terminal::task_pane_snapshot(
        &context,
        &mut runner,
        &handle,
        since.as_deref(),
        crate::slices::terminal::PANE_SNAPSHOT_LIMIT,
    ) {
        Ok(view) => json_value_response(200, serde_json::to_value(view).unwrap_or_default()),
        Err(crate::slices::terminal::SnapshotRouteError::TaskNotFound) => json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "task not found" }),
        ),
        Err(crate::slices::terminal::SnapshotRouteError::SessionMissing) => json_value_response(
            409,
            serde_json::json!({ "ok": false, "error": "tmux session missing" }),
        ),
        Err(crate::slices::terminal::SnapshotRouteError::Command(message)) => json_value_response(
            502,
            serde_json::json!({ "ok": false, "error": format!("pane capture failed: {message}") }),
        ),
    }
}

#[derive(Debug, serde::Deserialize)]
struct SendKeysRequest {
    #[serde(default)]
    text: String,
    #[serde(default)]
    submit: bool,
}

async fn axum_task_post<C, B>(
    State(state): State<WebAppState<C, B>>,
    AxumPath(handle): AxumPath<String>,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if let Some(task_handle) = handle.strip_suffix("/keys") {
        return axum_task_keys(State(state), task_handle.to_string(), body).await;
    }
    json_value_response(
        404,
        serde_json::json!({ "ok": false, "error": "not found" }),
    )
}

async fn axum_task_keys<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let request: SendKeysRequest = match serde_json::from_slice(&body) {
        Ok(request) => request,
        Err(error) => {
            return json_value_response(
                400,
                serde_json::json!({ "ok": false, "error": format!("json parse failed: {error}") }),
            );
        }
    };

    // Clone the context and runner so the tmux shell-out doesn't hold the shared
    // lock (the runner is a stateless command executor).
    let (context, mut runner) = {
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (guard.context.clone(), guard.runner.clone())
    };

    let outcome = crate::slices::terminal::send_task_keys(
        &context,
        &mut runner,
        &handle,
        &request.text,
        request.submit,
    );

    match outcome {
        Ok(()) => json_value_response(200, serde_json::json!({ "ok": true })),
        Err(crate::slices::terminal::SendKeysRouteError::TaskNotFound) => json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "task not found" }),
        ),
        Err(crate::slices::terminal::SendKeysRouteError::SessionMissing) => json_value_response(
            409,
            serde_json::json!({ "ok": false, "error": "tmux session missing" }),
        ),
        Err(crate::slices::terminal::SendKeysRouteError::InvalidKeys(message)) => {
            json_value_response(400, serde_json::json!({ "ok": false, "error": message }))
        }
        Err(crate::slices::terminal::SendKeysRouteError::Command(message)) => json_value_response(
            502,
            serde_json::json!({ "ok": false, "error": format!("send-keys failed: {message}") }),
        ),
    }
}

async fn axum_start_task<C, B>(
    State(state): State<WebAppState<C, B>>,
    Json(request): Json<crate::slices::operate::StartTaskRequest>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let request_id = request.request_id.trim().to_string();
    if request_id.is_empty() {
        return json_value_response(
            400,
            serde_json::json!({ "ok": false, "error": "request_id is required" }),
        );
    }
    let task_key = ajax_core::commands::start_task_identity(&request.repo, &request.title)
        .as_str()
        .to_string();
    {
        let mut operations = state
            .operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(response) = operations.completed_response(&request_id) {
            return response.clone().into_axum_response();
        }
        if operations.has_in_flight_mutation() {
            return json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": "task start already in progress",
                }),
            );
        }
        if !operations.in_flight_requests.insert(request_id.clone()) {
            return json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": "task start already in progress",
                }),
            );
        }
        if !operations.in_flight_tasks.insert(task_key.clone()) {
            operations.in_flight_requests.remove(&request_id);
            return json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": "task start already in progress",
                }),
            );
        }
    }
    let response = state.run_optimistic(
        Some(&request_id),
        "cockpit state changed while task start was running",
        |context, runner, bridge| match handle_start_task_request(request, context, runner, bridge)
        {
            Ok(response) => operation_response_with_request_id(response, Some(&request_id)),
            Err(error) => response_from_web_error(error, Some(&request_id)),
        },
    );
    let mut operations = state
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations.in_flight_requests.remove(&request_id);
    operations.in_flight_tasks.remove(&task_key);
    operations.store_completed_response(request_id, response.clone());
    response.into_axum_response()
}

async fn axum_action<C, B>(State(state): State<WebAppState<C, B>>, body: Bytes) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
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
            if let Some(response) = operations.completed_response(request_id) {
                return response.clone().into_axum_response();
            }
        }
        if operations.has_in_flight_mutation() {
            return json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": "operation already in progress",
                }),
            );
        }
        if let Some(request_id) = request_id.as_ref() {
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
                    "error": "operation already in progress",
                }),
            );
        }
    }

    let response = state.run_optimistic(
        request_id.as_deref(),
        "cockpit state changed while operation was running",
        |context, runner, bridge| match handle_action_request(request, context, runner, bridge) {
            Ok(response) => operation_response_with_request_id(response, request_id.as_deref()),
            Err(error) => response_from_web_error(error, request_id.as_deref()),
        },
    );

    let mut operations = state
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    operations.in_flight_tasks.remove(&task_key);
    if let Some(request_id) = request_id.as_ref() {
        operations.in_flight_requests.remove(request_id);
        operations.store_completed_response(request_id.clone(), response.clone());
    }

    response.into_axum_response()
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
        tier: RefreshTier,
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

fn handle_refreshed_cockpit_request<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    let _state_changed = bridge.refresh_cockpit(context, runner, RefreshTier::Live)?;
    json_response(
        200,
        serde_json::to_value(cockpit::browser_cockpit_view(context))
            .map_err(|error| WebError::JsonSerialization(error.to_string()))?,
    )
}

fn handle_action_request<C: CommandRunner>(
    request: MobileActionRequest,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
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
    let operator_action = OperatorAction::from_label(action)?;
    if supported_web_action(operator_action) {
        return None;
    }
    let message = match operator_action {
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
    request: crate::slices::operate::StartTaskRequest,
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
) -> Result<Response, WebError> {
    match bridge.execute_start_task(request, context, runner) {
        Ok(outcome) => operation_success_response(outcome, context),
        Err(error) => operation_error_response(error, context),
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionFailure, RefreshTier, RuntimeBridge};
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
    use std::{
        io::{Read, Write},
        sync::atomic::{AtomicUsize, Ordering},
        sync::{Arc, Condvar, Mutex},
        time::Duration,
    };
    use tokio::sync::Notify;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct TestBridge {
        refreshed: bool,
        refresh_tier: Option<RefreshTier>,
        refresh_count: usize,
        operate: Option<OperateRequest>,
        operate_count: usize,
        operate_delay: Duration,
        refresh_delay: Duration,
        operate_result: Result<OperateOutcome, ActionFailure>,
        start: Option<crate::slices::operate::StartTaskRequest>,
        start_count: usize,
        start_result: Result<OperateOutcome, ActionFailure>,
        operate_calls: Arc<AtomicUsize>,
        operate_entered: Option<Arc<Notify>>,
        operate_release: Option<Arc<(Mutex<bool>, Condvar)>>,
        start_calls: Arc<AtomicUsize>,
        start_entered: Option<Arc<Notify>>,
        start_release: Option<Arc<(Mutex<bool>, Condvar)>>,
    }

    impl Default for TestBridge {
        fn default() -> Self {
            Self {
                refreshed: false,
                refresh_tier: None,
                refresh_count: 0,
                operate: None,
                operate_count: 0,
                operate_delay: Duration::ZERO,
                refresh_delay: Duration::ZERO,
                operate_result: Ok(OperateOutcome {
                    state_changed: true,
                    output: String::new(),
                }),
                start: None,
                start_count: 0,
                start_result: Ok(OperateOutcome {
                    state_changed: true,
                    output: String::new(),
                }),
                operate_calls: Arc::new(AtomicUsize::new(0)),
                operate_entered: None,
                operate_release: None,
                start_calls: Arc::new(AtomicUsize::new(0)),
                start_entered: None,
                start_release: None,
            }
        }
    }

    fn refresh_cockpit_with_optional_delay<C: CommandRunner>(
        bridge: &mut TestBridge,
        context: &mut CommandContext<InMemoryRegistry>,
        _runner: &mut C,
        tier: RefreshTier,
    ) -> Result<bool, crate::WebError> {
        if bridge.refresh_delay > Duration::ZERO {
            std::thread::sleep(bridge.refresh_delay);
        }
        bridge.refreshed = true;
        bridge.refresh_tier = Some(tier);
        bridge.refresh_count += 1;
        let _ = context;
        Ok(false)
    }

    impl RuntimeBridge<OkRunner> for TestBridge {
        fn refresh_cockpit(
            &mut self,
            context: &mut CommandContext<InMemoryRegistry>,
            runner: &mut OkRunner,
            tier: RefreshTier,
        ) -> Result<bool, crate::WebError> {
            refresh_cockpit_with_optional_delay(self, context, runner, tier)
        }

        fn execute_operate(
            &mut self,
            request: OperateRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut OkRunner,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.operate_count += 1;
            let operate_call_index = self.operate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.operate_entered.as_ref() {
                entered.notify_one();
            }
            if operate_call_index == 0 {
                if let Some(operate_release) = self.operate_release.as_ref() {
                    let (lock, cvar) = &**operate_release;
                    let mut released = lock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while !*released {
                        released = cvar
                            .wait(released)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                }
            }
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
            self.start_count += 1;
            let start_call_index = self.start_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.start_entered.as_ref() {
                entered.notify_one();
            }
            if start_call_index == 0 {
                if let Some(start_release) = self.start_release.as_ref() {
                    let (lock, cvar) = &**start_release;
                    let mut released = lock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while !*released {
                        released = cvar
                            .wait(released)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                }
            }
            self.start = Some(request);
            self.start_result.clone()
        }
    }

    impl RuntimeBridge<ajax_core::adapters::RecordingCommandRunner> for TestBridge {
        fn refresh_cockpit(
            &mut self,
            context: &mut CommandContext<InMemoryRegistry>,
            runner: &mut ajax_core::adapters::RecordingCommandRunner,
            tier: RefreshTier,
        ) -> Result<bool, crate::WebError> {
            refresh_cockpit_with_optional_delay(self, context, runner, tier)
        }

        fn execute_operate(
            &mut self,
            request: OperateRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut ajax_core::adapters::RecordingCommandRunner,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.operate_count += 1;
            let operate_call_index = self.operate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.operate_entered.as_ref() {
                entered.notify_one();
            }
            if operate_call_index == 0 {
                if let Some(operate_release) = self.operate_release.as_ref() {
                    let (lock, cvar) = &**operate_release;
                    let mut released = lock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while !*released {
                        released = cvar
                            .wait(released)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                }
            }
            std::thread::sleep(self.operate_delay);
            self.operate = Some(request);
            self.operate_result.clone()
        }

        fn execute_start_task(
            &mut self,
            request: crate::slices::operate::StartTaskRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut ajax_core::adapters::RecordingCommandRunner,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.start_count += 1;
            let start_call_index = self.start_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.start_entered.as_ref() {
                entered.notify_one();
            }
            if start_call_index == 0 {
                if let Some(start_release) = self.start_release.as_ref() {
                    let (lock, cvar) = &**start_release;
                    let mut released = lock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    while !*released {
                        released = cvar
                            .wait(released)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                }
            }
            self.start = Some(request);
            self.start_result.clone()
        }
    }

    #[derive(Clone, Copy, Default)]
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

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        use ajax_core::{
            config::ManagedRepo,
            models::{AgentClient, Task, TaskId},
            registry::Registry as _,
        };

        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
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
        CommandContext::new(config, registry)
    }

    fn context_with_web_repo() -> CommandContext<InMemoryRegistry> {
        use ajax_core::config::ManagedRepo;

        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        };
        CommandContext::new(config, InMemoryRegistry::default())
    }

    fn context_with_two_tasks() -> CommandContext<InMemoryRegistry> {
        use ajax_core::{
            config::ManagedRepo,
            models::{AgentClient, Task, TaskId},
            registry::Registry as _,
        };

        let config = Config {
            repos: vec![
                ManagedRepo::new("web", "/repo/web", "main"),
                ManagedRepo::new("api", "/repo/api", "main"),
            ],
            ..Config::default()
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
        registry
            .create_task(Task::new(
                TaskId::new("api/fix-auth"),
                "api",
                "fix-auth",
                "Fix auth",
                "ajax/fix-auth",
                "main",
                "/repo/api__worktrees/ajax-fix-auth",
                "ajax-api-fix-auth",
                "worktrunk",
                AgentClient::Codex,
            ))
            .unwrap();
        CommandContext::new(config, registry)
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

    fn browser_session_cookie<C, B>(state: &super::WebAppState<C, B>) -> String {
        state.browser_session.cookie_pair()
    }

    fn authenticated_request(cookie: &str, uri: &str) -> axum::http::request::Builder {
        AxumRequest::builder().uri(uri).header("cookie", cookie)
    }

    fn run_axum_request<C, B>(
        state: super::WebAppState<C, B>,
        method: &str,
        path: &str,
        body: &str,
    ) -> super::Response
    where
        C: CommandRunner + Clone + Send + 'static,
        B: RuntimeBridge<C> + Clone + Send + 'static,
    {
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let mut request = authenticated_request(&session_cookie, path).method(method);
                if !body.is_empty() {
                    request = request.header("content-type", "application/json");
                }
                let response = app
                    .oneshot(request.body(Body::from(body.to_string())).unwrap())
                    .await
                    .unwrap();
                let status_code = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|value| value.to_str().ok())
                    .map(http_content_type_to_static)
                    .unwrap_or("text/plain; charset=utf-8");
                let body = to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec();
                super::Response {
                    status_code,
                    content_type,
                    body,
                }
            })
    }

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

    #[test]
    fn axum_api_access_policy_classifies_public_and_protected_routes() {
        use super::ApiAccess;

        for (method, path) in [
            ("GET", "/"),
            ("GET", "/index.html"),
            ("GET", "/app.js"),
            ("GET", "/api/health"),
            ("POST", "/api/session"),
        ] {
            assert_eq!(
                super::api_access_policy(method, path),
                ApiAccess::Public,
                "{method} {path}"
            );
        }

        for (method, path) in [
            ("GET", "/api/session"),
            ("GET", "/api/cockpit"),
            ("GET", "/api/version"),
            ("POST", "/api/server/restart"),
            ("POST", "/api/operations"),
            ("POST", "/api/tasks"),
            ("GET", "/api/tasks/web%2Ffix-login"),
            ("GET", "/api/tasks/web%2Ffix-login/terminal"),
        ] {
            assert_eq!(
                super::api_access_policy(method, path),
                ApiAccess::BrowserSessionRequired,
                "{method} {path}"
            );
        }
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
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let shell = app
            .clone()
            .oneshot(AxumRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
        assert_eq!(shell.headers()["cache-control"], "no-store");
        let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&shell_body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
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
        assert_eq!(cockpit.headers()["cache-control"], "no-store");
        let cockpit_body = to_bytes(cockpit.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit_body).unwrap()["cards"],
            serde_json::json!([])
        );

        let missing_api = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/missing")
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
        assert_eq!(missing_api.headers()["cache-control"], "no-store");
        let missing_api_body = to_bytes(missing_api.into_body(), usize::MAX).await.unwrap();
        assert!(!std::str::from_utf8(&missing_api_body)
            .unwrap()
            .contains("Ajax Cockpit"));

        for path in [
            "/manifest.webmanifest",
            "/sw.js",
            "/icons/icon-192.png",
            "/icons/icon-512.png",
            "/icons/icon-maskable-512.png",
            "/icons/apple-touch-icon.png",
        ] {
            let retired_asset = app
                .clone()
                .oneshot(
                    AxumRequest::builder()
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(retired_asset.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(
                retired_asset.headers()["content-type"],
                "text/plain; charset=utf-8",
                "{path}"
            );
        }

        let missing_asset = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/does-not-exist.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            missing_asset.headers()["content-type"],
            "text/plain; charset=utf-8"
        );
        assert_eq!(missing_asset.headers()["cache-control"], "no-store");
        let missing_asset_body = to_bytes(missing_asset.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            std::str::from_utf8(&missing_asset_body).unwrap(),
            "not found"
        );
    }

    #[tokio::test]
    async fn axum_api_routes_require_browser_session_cookie_except_health() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-api-session"),
        );
        let app = super::axum_app(state);

        let shell = app
            .clone()
            .oneshot(AxumRequest::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let session_cookie = shell
            .headers()
            .get("set-cookie")
            .expect("shell should set browser session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        assert!(session_cookie.starts_with("ajax_browser_session="));

        let health = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);

        let unauthenticated = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let authenticated = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/cockpit")
                    .header("cookie", session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn axum_browser_session_renewal_bootstraps_api_access() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-session-renewal"),
        );
        let app = super::axum_app(state);

        let unauthenticated = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthenticated.status(), StatusCode::UNAUTHORIZED);

        let renewal = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .method("POST")
                    .uri("/api/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(renewal.status(), StatusCode::OK);
        let session_cookie = renewal
            .headers()
            .get("set-cookie")
            .expect("session renewal should set browser session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        assert!(session_cookie.starts_with("ajax_browser_session="));

        let authenticated = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/cockpit")
                    .header("cookie", session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authenticated.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn axum_session_renewal_response_is_cookie_json_without_shared_state() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-session-boundary"),
        );
        let response = super::browser_session_json_response(&state.browser_session);

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert!(response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .starts_with("ajax_browser_session="));
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({ "ok": true })
        );
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.revision, 0);
        assert!(!guard.bridge.refreshed);
        assert_eq!(guard.bridge.operate_count, 0);
        assert_eq!(guard.bridge.start_count, 0);
    }

    #[tokio::test]
    async fn tls_listener_idle_tcp_connection_does_not_block_health_request() {
        let state = super::WebAppState::new(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            OkRunner,
            TestBridge::default(),
            scratch_dir("tls-idle-health"),
        );
        let identity = crate::adapters::tls::load_or_create_identity(&state.state_dir).unwrap();
        let tls_config = crate::adapters::tls::tls_server_config(&identity).unwrap();
        let tcp_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = tcp_listener.local_addr().unwrap();
        let (accepted_tls_tx, accepted_tls_rx) = tokio::sync::mpsc::channel(1024);
        let tls_listener = super::TlsListener {
            listener: tcp_listener,
            acceptor: tokio_rustls::TlsAcceptor::from(tls_config),
            accepted_tls_tx,
            accepted_tls_rx,
        };
        let server = tokio::spawn(async move {
            axum::serve(tls_listener, super::axum_app(state))
                .await
                .unwrap();
        });

        let idle_connection = tokio::net::TcpStream::connect(address).await.unwrap();
        let health =
            tokio::time::timeout(Duration::from_millis(500), tls_get(address, "/api/health")).await;

        drop(idle_connection);
        server.abort();

        let response = health.expect("health request timed out").unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains(r#"{"ok":true}"#), "{response}");
    }

    #[derive(Debug)]
    struct AcceptAnyServerCert;

    impl rustls::client::danger::ServerCertVerifier for AcceptAnyServerCert {
        fn verify_server_cert(
            &self,
            _end_entity: &rustls::pki_types::CertificateDer<'_>,
            _intermediates: &[rustls::pki_types::CertificateDer<'_>],
            _server_name: &rustls::pki_types::ServerName<'_>,
            _ocsp_response: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &rustls::pki_types::CertificateDer<'_>,
            _dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::ED25519,
            ]
        }
    }

    async fn tls_get(address: std::net::SocketAddr, path: &str) -> std::io::Result<String> {
        let path = path.to_string();
        tokio::task::spawn_blocking(move || tls_get_blocking(address, &path))
            .await
            .unwrap()
    }

    fn tls_get_blocking(address: std::net::SocketAddr, path: &str) -> std::io::Result<String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert))
            .with_no_client_auth();
        let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
        let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
            .map_err(std::io::Error::other)?;
        let stream = std::net::TcpStream::connect(address)?;
        let mut stream = rustls::StreamOwned::new(connection, stream);
        stream.write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response)?;
        Ok(String::from_utf8_lossy(&response).into_owned())
    }

    #[tokio::test]
    async fn axum_cockpit_serves_cached_projection_within_refresh_ttl() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-cockpit-cache"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    authenticated_request(&session_cookie, "/api/cockpit")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.refresh_count, 1);
    }

    #[tokio::test]
    async fn axum_cockpit_refreshes_again_after_ttl_expires() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-cockpit-ttl"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

        let second = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::OK);

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.refresh_count, 2);
    }

    #[tokio::test]
    async fn axum_operation_invalidates_cockpit_refresh_cache() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-cockpit-invalidate"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let cockpit = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cockpit.status(), StatusCode::OK);

        let operation = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"invalidate-1","task_handle":"web/fix-login","action":"review"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(operation.status(), StatusCode::OK);

        let refreshed = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(refreshed.status(), StatusCode::OK);

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.refresh_count, 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_cockpit_polls_share_one_refresh() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge {
                refresh_delay: Duration::from_millis(200),
                ..TestBridge::default()
            },
            scratch_dir("axum-cockpit-single-flight"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/cockpit")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(25)).await;
        let second = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/cockpit")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.refresh_count, 1);
    }

    #[tokio::test]
    async fn axum_router_reports_shell_version() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-version"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/version")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let version = value["version"].as_str().expect("version string");
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
        assert_eq!(version, crate::slices::install::app_version());
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
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
        let first = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
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
        assert!(first_json["cockpit"].is_object());

        let second = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
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
    async fn axum_task_starts_are_idempotent_by_request_id() {
        let state = super::WebAppState::new(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            OkRunner,
            TestBridge::default(),
            scratch_dir("axum-start-idempotency"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());
        let request =
            r#"{"request_id":"start-1","repo":"web","title":"Fix login","agent":"codex"}"#;

        for _ in 0..2 {
            let response = app
                .clone()
                .oneshot(
                    authenticated_request(&session_cookie, "/api/tasks")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(request))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(json["ok"], true);
            assert_eq!(json["request_id"], "start-1");
            assert!(json["cockpit"].is_object());
        }

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.start_count, 1);
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
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
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

    #[tokio::test]
    async fn operation_endpoint_returns_refreshed_cockpit_on_bridge_error() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge {
                operate_result: Err(ActionFailure {
                    message: "bridge failed".to_string(),
                    state_changed: true,
                }),
                ..TestBridge::default()
            },
            scratch_dir("axum-operation-error"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"op-error-1","task_handle":"web/fix-login","action":"review"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "op-error-1");
        assert_eq!(json["state_changed"], true);
        assert_eq!(json["error"], "bridge failed");
        assert!(json["cockpit"].is_object());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_start_task_rejects_concurrent_colliding_normalized_identity() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let state = super::WebAppState::new(
            context_with_web_repo(),
            OkRunner,
            TestBridge {
                start_entered: Some(Arc::clone(&entered)),
                start_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            scratch_dir("axum-start-collision"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/tasks")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first start request never entered the bridge");

        let conflict = tokio::time::timeout(
            Duration::from_secs(5),
            async {
                app.oneshot(
                    authenticated_request(&session_cookie, "/api/tasks")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"start-b","repo":"web","title":"Fix login!","agent":"codex"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
            },
        )
        .await
        .expect("second start request timed out");

        {
            let (lock, cvar) = &*release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *released = true;
            cvar.notify_all();
        }

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = to_bytes(conflict.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "start-b");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already in progress"));

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), first)
                .await
                .expect("first start request timed out")
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.start_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_start_task_rejects_when_action_operation_is_in_flight_before_bridge_side_effects()
    {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let state = super::WebAppState::new(
            context_with_two_tasks(),
            OkRunner,
            TestBridge {
                operate_entered: Some(Arc::clone(&entered)),
                operate_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            scratch_dir("axum-start-blocked-by-action"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/operations")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"op-a","task_handle":"web/fix-login","action":"review"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first operation request never entered the bridge");

        let conflict = tokio::time::timeout(Duration::from_secs(5), async {
            app.oneshot(
                authenticated_request(&session_cookie, "/api/tasks")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"start-a","repo":"web","title":"Start while action runs","agent":"codex"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap()
        })
        .await
        .expect("start request timed out");

        {
            let (lock, cvar) = &*release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *released = true;
            cvar.notify_all();
        }

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = to_bytes(conflict.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "start-a");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already in progress"));

        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), first)
                .await
                .expect("first operation request timed out")
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.start_calls.load(Ordering::SeqCst), 0);
        assert_eq!(guard.bridge.operate_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_start_task_duplicate_request_id_does_not_clear_original_in_flight_marker() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let state = super::WebAppState::new(
            context_with_web_repo(),
            OkRunner,
            TestBridge {
                start_entered: Some(Arc::clone(&entered)),
                start_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            scratch_dir("axum-start-duplicate-request-id"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/tasks")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first start request never entered the bridge");

        let duplicate_same_task = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/tasks")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(duplicate_same_task.status(), StatusCode::CONFLICT);

        let duplicate_different_task = tokio::time::timeout(Duration::from_secs(5), async {
            app.oneshot(
                authenticated_request(&session_cookie, "/api/tasks")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"start-a","repo":"web","title":"Different task","agent":"codex"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap()
        })
        .await
        .expect("duplicate start request timed out");

        {
            let (lock, cvar) = &*release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *released = true;
            cvar.notify_all();
        }

        assert_eq!(duplicate_different_task.status(), StatusCode::CONFLICT);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), first)
                .await
                .expect("first start request timed out")
                .unwrap()
                .status(),
            StatusCode::OK
        );
        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.start_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn operation_coordinator_prunes_completed_request_ids() {
        let mut coordinator = super::OperationCoordinator::default();

        for index in 0..=128 {
            let request_id = format!("req-{index}");
            coordinator.store_completed_response(
                request_id.clone(),
                super::Response {
                    status_code: 200,
                    content_type: "application/json; charset=utf-8",
                    body: serde_json::to_vec(&serde_json::json!({
                        "ok": true,
                        "request_id": request_id,
                    }))
                    .unwrap(),
                },
            );
        }

        assert!(coordinator.completed_response("req-0").is_none());
        assert!(coordinator.completed_response("req-128").is_some());
        coordinator
            .in_flight_requests
            .insert("req-live".to_string());
        assert!(coordinator.has_in_flight_mutation());
    }

    #[test]
    fn committed_operation_fixture_matches_production_response_builder() {
        let context = crate::slices::cockpit::tests::browser_contract_context();
        let response = super::operation_success_response(
            OperateOutcome {
                state_changed: true,
                output: "Operation completed successfully.".to_string(),
            },
            &context,
        )
        .unwrap();
        let actual: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        let committed: serde_json::Value =
            serde_json::from_str(include_str!("../web/src/fixtures/operation.json")).unwrap();

        assert_eq!(committed, actual);
    }

    #[tokio::test]
    async fn start_task_endpoint_returns_refreshed_cockpit_on_bridge_error() {
        let state = super::WebAppState::new(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            OkRunner,
            TestBridge {
                start_result: Err(ActionFailure {
                    message: "start failed".to_string(),
                    state_changed: true,
                }),
                ..TestBridge::default()
            },
            scratch_dir("axum-start-error"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/tasks")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"start-error-1","repo":"web","title":"Fix login","agent":"codex"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "start-error-1");
        assert_eq!(json["state_changed"], true);
        assert_eq!(json["error"], "start failed");
        assert!(json["cockpit"].is_object());
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
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/operations")
                        .method("POST")
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
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
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
        assert_eq!(guard.bridge.operate_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_rejects_concurrent_different_task_operations_before_bridge_side_effects() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let state = super::WebAppState::new(
            context_with_two_tasks(),
            OkRunner,
            TestBridge {
                operate_entered: Some(Arc::clone(&entered)),
                operate_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            scratch_dir("axum-concurrent-different-tasks"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let first_app = app.clone();
        let first_cookie = session_cookie.clone();
        let first = tokio::spawn(async move {
            first_app
                .oneshot(
                    authenticated_request(&first_cookie, "/api/operations")
                        .method("POST")
                        .header("content-type", "application/json")
                        .body(Body::from(
                            r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first request never entered the bridge");

        let conflict = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"req-b","task_handle":"api/fix-auth","action":"ship"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        {
            let (lock, cvar) = &*release;
            let mut released = lock
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *released = true;
            cvar.notify_all();
        }

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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_health_stays_responsive_during_slow_cockpit_refresh() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge {
                refresh_delay: Duration::from_millis(400),
                ..TestBridge::default()
            },
            scratch_dir("axum-health-cockpit"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let slow_app = app.clone();
        let slow_cookie = session_cookie.clone();
        let cockpit = tokio::spawn(async move {
            slow_app
                .oneshot(
                    authenticated_request(&slow_cookie, "/api/cockpit")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        });

        let health_started = std::time::Instant::now();
        let health = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let health_elapsed = health_started.elapsed();

        assert_eq!(health.status(), StatusCode::OK);
        assert!(
            health_elapsed < Duration::from_millis(150),
            "health took {health_elapsed:?} while cockpit refresh was in flight"
        );
        assert_eq!(cockpit.await.unwrap().status(), StatusCode::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_cockpit_refresh_does_not_overwrite_concurrent_operation_state() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge {
                refresh_delay: Duration::from_millis(250),
                ..TestBridge::default()
            },
            scratch_dir("axum-refresh-operation-race"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());

        let refresh_app = app.clone();
        let refresh_cookie = session_cookie.clone();
        let cockpit = tokio::spawn(async move {
            refresh_app
                .oneshot(
                    authenticated_request(&refresh_cookie, "/api/cockpit")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
        });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let operation = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/operations")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"request_id":"req-race","task_handle":"web/fix-login","action":"review"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(operation.status(), StatusCode::OK);
        assert_eq!(cockpit.await.unwrap().status(), StatusCode::OK);

        let guard = state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(guard.bridge.operate_count, 1);
        assert_eq!(
            guard
                .bridge
                .operate
                .as_ref()
                .map(|request| request.action.as_str()),
            Some("review")
        );
    }

    #[test]
    fn runtime_routes_to_vertical_slices() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("routes"),
        );

        let shell = run_axum_request(state.clone(), "GET", "/", "");
        assert_eq!(shell.status_code, 200);
        assert_eq!(shell.content_type, "text/html; charset=utf-8");
        assert!(std::str::from_utf8(&shell.body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = run_axum_request(state, "GET", "/api/cockpit", "");
        assert_eq!(cockpit.status_code, 200);
        assert_eq!(cockpit.content_type, "application/json; charset=utf-8");
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&cockpit.body).unwrap()["cards"],
            serde_json::json!([])
        );
    }

    #[test]
    fn cockpit_api_refreshes_before_rendering() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("refresh");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let response = run_axum_request(state.clone(), "GET", "/api/cockpit", "");

        assert_eq!(response.status_code, 200);
        let bridge = &state
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bridge;
        assert!(bridge.refreshed);
        assert_eq!(bridge.refresh_tier, Some(RefreshTier::Live));
    }

    #[test]
    fn server_restart_endpoint_returns_restarting_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("restart");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let response = run_axum_request(state, "POST", "/api/server/restart", "");

        assert_eq!(response.status_code, 200);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["restarting"], true);
    }

    #[test]
    fn action_endpoint_executes_bridge_action_and_returns_cockpit() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("action");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let response = run_axum_request(
            state.clone(),
            "POST",
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"review"}"#,
        );

        assert_eq!(response.status_code, 200);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["ok"], true);
        assert_eq!(body["state_changed"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            state
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .bridge
                .operate,
            Some(OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "review".to_string(),
            })
        );
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

        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("detail"),
        );
        let response = run_axum_request(state, "GET", "/api/tasks/web/fix-login", "");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["qualified_handle"], "web/fix-login");
        assert_eq!(body["title"], "Fix login");
        assert_eq!(body["branch"], "ajax/fix-login");
    }

    #[test]
    fn axum_task_terminal_requires_browser_session_cookie() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("terminal-auth"),
        );
        let app = super::axum_app(state);

        let response = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                app.oneshot(
                    AxumRequest::builder()
                        .method("GET")
                        .uri("/api/tasks/web%2Ffix-login/terminal")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
            });

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn axum_task_terminal_rejects_non_upgrade_requests() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("terminal-upgrade"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/tasks/web%2Ffix-login/terminal")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "websocket upgrade required"
        );
    }

    #[test]
    fn get_task_detail_returns_text_404_for_unknown_handle() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("detail-missing"),
        );
        let response = run_axum_request(state, "GET", "/api/tasks/web/missing", "");

        assert_eq!(response.status_code, 404);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "task not found");
    }

    #[test]
    fn unknown_in_memory_api_path_stays_generic_404() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("missing-api"),
        );
        let response = run_axum_request(state, "GET", "/api/missing", "");

        assert_eq!(response.status_code, 404);
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "not found");
    }

    #[test]
    fn operation_helpers_accept_typed_requests_without_json_roundtrip() {
        let production_source = include_str!("runtime.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(
            !production_source.contains("serde_json::to_string(&request).unwrap_or_default()"),
            "operation routes should not serialize typed requests back to JSON for internal helpers"
        );
        assert!(
            !production_source
                .contains("fn handle_action_request<C: CommandRunner>(\n    body: &str,"),
            "handle_action_request should accept MobileActionRequest directly"
        );
        assert!(
            !production_source
                .contains("fn handle_start_task_request<C: CommandRunner>(\n    body: &str,"),
            "handle_start_task_request should accept StartTaskRequest directly"
        );
        assert!(
            !production_source
                .contains("let request: MobileActionRequest = serde_json::from_str(body)"),
            "action helper should not reparse MobileActionRequest from JSON"
        );
        assert!(
            !production_source.contains("let request: crate::slices::operate::StartTaskRequest = serde_json::from_str(body)"),
            "start helper should not reparse StartTaskRequest from JSON"
        );
    }

    #[test]
    fn runtime_tests_do_not_compare_axum_against_old_router() {
        let source = include_str!("runtime.rs");

        for marker in [
            concat!(
                "stale_answer_responses_match_between_axum_and_",
                "leg",
                "acy"
            ),
            concat!("leg", "acy_", "response"),
            concat!("leg", "acy_", "context"),
            concat!("leg", "acy_", "runner"),
            concat!("leg", "acy_", "bridge"),
            concat!("leg", "acy_", "dir"),
        ] {
            assert!(
                !source.contains(marker),
                "runtime tests should not preserve old-router marker {marker}"
            );
        }
    }

    #[test]
    fn runtime_module_does_not_define_legacy_manual_router_helpers() {
        let production_source = include_str!("runtime.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        for marker in [
            "pub fn route<R: Registry>",
            "pub fn route_with_bridge<C: CommandRunner>",
            "fn split_path_and_query(raw_path: &str) -> (&str, &str)",
            "fn percent_decode(value: &str) -> String",
        ] {
            assert!(
                !production_source.contains(marker),
                "runtime.rs should no longer define legacy router helper {marker}"
            );
        }
    }

    #[test]
    fn post_tasks_endpoint_delegates_to_start_bridge_method() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("start");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let response = run_axum_request(
            state.clone(),
            "POST",
            "/api/tasks",
            r#"{"repo":"web","title":"Fix login","agent":"codex","request_id":"req-1"}"#,
        );

        assert_eq!(response.status_code, 200);
        let body: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["ok"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            state
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .bridge
                .start,
            Some(crate::slices::operate::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: "req-1".to_string(),
            })
        );
    }

    #[test]
    fn action_endpoint_keeps_start_out_of_bridge() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("native-action");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let response = run_axum_request(
            state.clone(),
            "POST",
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"start"}"#,
        );

        assert_eq!(response.status_code, 409);
        assert_eq!(
            state
                .shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .bridge
                .operate,
            None
        );
    }

    #[test]
    fn push_endpoints_are_not_supported() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let runner = OkRunner;
        let bridge = TestBridge::default();
        let dir = scratch_dir("push");

        let state = super::WebAppState::new(context, runner, bridge, dir);
        let config = run_axum_request(state.clone(), "GET", "/api/push/config", "");
        assert_eq!(config.status_code, 404);

        let subscribe = run_axum_request(
            state.clone(),
            "POST",
            "/api/push/subscribe",
            r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
        );
        assert_eq!(subscribe.status_code, 404);

        let unsubscribe = run_axum_request(
            state,
            "POST",
            "/api/push/unsubscribe",
            r#"{"endpoint":"https://push.example/x"}"#,
        );
        assert_eq!(unsubscribe.status_code, 404);
    }

    #[tokio::test]
    async fn push_path_does_not_start_nested_runtime() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("push-runtime"),
        );
        let session_cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let config = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/push/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("push config request should complete");
        assert_eq!(config.status(), StatusCode::NOT_FOUND);

        let subscribe = app
            .clone()
            .oneshot(
                authenticated_request(&session_cookie, "/api/push/subscribe")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
                    ))
                    .unwrap(),
            )
            .await
            .expect("push subscribe request should complete");
        assert_eq!(subscribe.status(), StatusCode::NOT_FOUND);

        let unsubscribe = app
            .oneshot(
                authenticated_request(&session_cookie, "/api/push/unsubscribe")
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"endpoint":"https://push.example/x"}"#))
                    .unwrap(),
            )
            .await
            .expect("push unsubscribe request should complete");
        assert_eq!(unsubscribe.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn server_health_remains_responsive_after_terminal_disconnect_cleanup() {
        let state = super::WebAppState::new(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            OkRunner,
            TestBridge::default(),
            scratch_dir("terminal-cleanup-health"),
        );
        let app = super::axum_app(state);

        let cleanup = tokio::spawn(async move {
            crate::adapters::terminal_pty::simulate_terminal_disconnect_cleanup_for_tests(
                Duration::from_millis(50),
            )
            .await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let health_started = std::time::Instant::now();
        let health = app
            .oneshot(
                AxumRequest::builder()
                    .uri("/api/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("health request should complete");
        let health_elapsed = health_started.elapsed();

        assert_eq!(health.status(), StatusCode::OK);
        assert!(
            health_elapsed < Duration::from_millis(150),
            "health took {health_elapsed:?} while terminal cleanup was in flight"
        );

        cleanup.await.expect("terminal cleanup should finish");
    }
}
