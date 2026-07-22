//! Web companion runtime wiring.

use crate::{
    adapters::{
        browser_session::BrowserSession,
        cloudflare_access::{CloudflareAccessConfig, CloudflareAccessError},
        server, tls,
    },
    slices::actions::supported_web_action,
    slices::{cockpit, dev_deploy, install},
    WebError,
};
use ajax_core::{
    adapters::CommandRunner, commands::CommandContext, config::NotifyConfig,
    models::OperatorAction, registry::InMemoryRegistry, runtime_refresh::RefreshTier,
};
use axum::{
    body::Bytes,
    extract::{
        ws::WebSocketUpgrade, FromRequestParts, Path as AxumPath, Request as AxumRequest, State,
    },
    http::{header, HeaderMap, Uri},
    middleware::{from_fn_with_state, Next},
    response::Response as AxumResponse,
    routing::{get, post},
    serve::Listener,
    Json, Router,
};
use serde::Deserialize;
use std::{
    collections::{BTreeSet, HashMap, VecDeque},
    io::{BufRead, BufReader},
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    process::{Command as ProcessCommand, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tower_http::compression::CompressionLayer;

pub use crate::adapters::http::Response;

use crate::adapters::http::{
    bytes_axum_response, html_response, json_response, json_value_response,
    operation_response_with_request_id, response_from_web_error, text_axum_response,
    web_error_response,
};

const COCKPIT_REFRESH_CACHE_TTL: Duration = Duration::from_millis(750);
const DEFAULT_NOTIFY_POLL_SECONDS: u64 = 30;
const BROWSER_CONNECTED_TTL: Duration = Duration::from_secs(90);
const TLS_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_COMPLETED_OPERATIONS: usize = 128;

pub struct WebAppState<C, B> {
    shared: Arc<Mutex<WebSharedState<C, B>>>,
    operations: Arc<Mutex<OperationCoordinator>>,
    control_lane: Arc<tokio::sync::Mutex<()>>,
    state_dir: Arc<PathBuf>,
    browser_session: Arc<BrowserSession>,
    cloudflare_access: Arc<Option<CloudflareAccessConfig>>,
    last_browser_cockpit_at: Arc<Mutex<Option<Instant>>>,
    dev_deploy: Arc<dev_deploy::SharedDevDeploySlot>,
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
            control_lane: Arc::clone(&self.control_lane),
            state_dir: Arc::clone(&self.state_dir),
            browser_session: Arc::clone(&self.browser_session),
            cloudflare_access: Arc::clone(&self.cloudflare_access),
            last_browser_cockpit_at: Arc::clone(&self.last_browser_cockpit_at),
            dev_deploy: Arc::clone(&self.dev_deploy),
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
            let guard = self.shared();
            (
                guard.context.clone(),
                guard.runner.clone(),
                guard.bridge.clone(),
                guard.revision,
            )
        };
        let response = operate(&mut context, &mut runner, &mut bridge);
        let mut guard = self.shared();
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
    fn shared(&self) -> std::sync::MutexGuard<'_, WebSharedState<C, B>> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn operations(&self) -> std::sync::MutexGuard<'_, OperationCoordinator> {
        self.operations
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

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
            control_lane: Arc::new(tokio::sync::Mutex::new(())),
            state_dir: Arc::new(state_dir),
            browser_session: Arc::new(BrowserSession::test_default()),
            cloudflare_access: Arc::new(None),
            last_browser_cockpit_at: Arc::new(Mutex::new(None)),
            dev_deploy: Arc::new(Mutex::new(dev_deploy::DevDeploySlot::default())),
        }
    }

    pub fn mark_browser_cockpit_seen(&self) {
        *self
            .last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Instant::now());
    }

    pub fn browser_connected(&self) -> bool {
        self.last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some_and(|at| at.elapsed() < BROWSER_CONNECTED_TTL)
    }

    #[cfg(test)]
    fn set_browser_cockpit_seen_at_for_test(&self, at: Instant) {
        *self
            .last_browser_cockpit_at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(at);
    }

    pub fn load_or_create(
        context: CommandContext<InMemoryRegistry>,
        runner: C,
        bridge: B,
        state_dir: PathBuf,
    ) -> Result<Self, WebError> {
        let browser_session = BrowserSession::load_or_create(&state_dir)?;
        let cloudflare_access = CloudflareAccessConfig::from_env()?;
        Ok(Self {
            shared: Arc::new(Mutex::new(WebSharedState {
                context,
                runner,
                bridge,
                revision: 0,
                cockpit_cache: None,
            })),
            operations: Arc::new(Mutex::new(OperationCoordinator::default())),
            control_lane: Arc::new(tokio::sync::Mutex::new(())),
            state_dir: Arc::new(state_dir),
            browser_session: Arc::new(browser_session),
            cloudflare_access: Arc::new(cloudflare_access),
            last_browser_cockpit_at: Arc::new(Mutex::new(None)),
            dev_deploy: Arc::new(Mutex::new(dev_deploy::DevDeploySlot::default())),
        })
    }

    #[cfg(test)]
    fn with_cloudflare_access_for_test(self, config: CloudflareAccessConfig) -> Self {
        Self {
            cloudflare_access: Arc::new(Some(config)),
            ..self
        }
    }

    fn cached_cockpit_response(&self) -> Option<Response> {
        let guard = self.shared();
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

/// Construct the per-attach terminal input acknowledgment sink. The sink
/// locks shared state, split-borrows `context` and `bridge`, and calls
/// `bridge.acknowledge_operator_input(context, task_handle)`. On `Ok(true)`
/// it bumps `revision` (saturating) and clears `cockpit_cache` so the next
/// cockpit fetch observes the acknowledgment. On `Ok(false)` or error,
/// revision and cache are left untouched (errors are dropped: the terminal
/// adapter must not propagate core failures back into the wire loop).
pub fn operator_input_sink<C, B>(
    state: &WebAppState<C, B>,
    task_handle: String,
) -> Arc<dyn Fn() + Send + Sync>
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let state = state.clone();
    Arc::new(move || {
        // Typing in the PWA terminal is active presence; refresh the notify
        // suppress TTL even when cockpit polls have stalled.
        state.mark_browser_cockpit_seen();
        let mut guard = state.shared();
        let acknowledged = {
            let WebSharedState {
                context, bridge, ..
            } = &mut *guard;
            bridge
                .acknowledge_operator_input(context, &task_handle)
                .unwrap_or(false)
        };
        if acknowledged {
            guard.revision = guard.revision.saturating_add(1);
            guard.cockpit_cache = None;
        }
    })
}

#[derive(Default)]
struct OperationCoordinator {
    completed: HashMap<String, Response>,
    completed_request_ids: VecDeque<String>,
    in_flight_requests: BTreeSet<String>,
    in_flight_tasks: BTreeSet<String>,
}

/// Why a mutation could not enter the in-flight gate.
enum GateRejection {
    /// The request id already completed; replay its stored response.
    Replay(Response),
    /// Another mutation holds the gate.
    Conflict,
}

impl OperationCoordinator {
    fn completed_response(&self, request_id: &str) -> Option<Response> {
        self.completed.get(request_id).cloned()
    }

    fn has_in_flight_mutation(&self) -> bool {
        !self.in_flight_requests.is_empty() || !self.in_flight_tasks.is_empty()
    }

    /// Claim the single-mutation gate for this request/task pair, or explain
    /// why the caller must stop: idempotent replay or a 409 conflict.
    fn try_begin(&mut self, request_id: Option<&str>, task_key: &str) -> Result<(), GateRejection> {
        if let Some(request_id) = request_id {
            if let Some(response) = self.completed_response(request_id) {
                return Err(GateRejection::Replay(response));
            }
        }
        if self.has_in_flight_mutation() {
            return Err(GateRejection::Conflict);
        }
        if let Some(request_id) = request_id {
            if !self.in_flight_requests.insert(request_id.to_string()) {
                return Err(GateRejection::Conflict);
            }
        }
        if !self.in_flight_tasks.insert(task_key.to_string()) {
            if let Some(request_id) = request_id {
                self.in_flight_requests.remove(request_id);
            }
            return Err(GateRejection::Conflict);
        }
        Ok(())
    }

    /// Release the gate and record the response for idempotent replay.
    fn finish(&mut self, request_id: Option<&str>, task_key: &str, response: &Response) {
        self.in_flight_tasks.remove(task_key);
        if let Some(request_id) = request_id {
            self.in_flight_requests.remove(request_id);
            self.store_completed_response(request_id.to_string(), response.clone());
        }
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
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let session_state = state.clone();
    Router::new()
        .route("/", get(axum_browser_shell::<C, B>))
        .route("/index.html", get(axum_browser_shell::<C, B>))
        .route("/app.css", get(axum_app_css))
        .route("/app.js", get(axum_app_js))
        .route("/terminal.js", get(axum_terminal_js))
        .route("/api/health", get(axum_health))
        .route("/api/session", post(axum_browser_session::<C, B>))
        .route("/api/version", get(axum_version))
        .route("/api/server/restart", post(axum_server_restart))
        .route(
            "/api/server/test-in-stable",
            post(axum_server_test_in_stable),
        )
        .route(
            "/api/dev-deploy",
            get(axum_dev_deploy_status::<C, B>).post(axum_dev_deploy_start::<C, B>),
        )
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
        .layer(CompressionLayer::new())
        .with_state(state)
}

pub(crate) fn log_web_listening(host: &str, port: u16) {
    tracing::info!(target: "ajax_web", host = %host, port, "listening");
}

pub fn serve_axum_web<C, B>(host: &str, port: u16, state: WebAppState<C, B>) -> Result<(), WebError>
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    let identity = tls::load_or_create_identity(&state.state_dir)?;
    let address = resolve_bind_address(host, port)?;
    log_web_listening(host, port);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| WebError::CommandFailed(format!("web runtime failed: {error}")))?;

    // Kill any ephemeral per-client terminal sessions left behind by a bridge
    // that crashed before it could tear its own session down.
    crate::adapters::terminal_pty::reap_orphan_terminal_sessions();

    runtime.block_on(async move {
        spawn_notify_tick(&state);
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

fn notify_poll_interval(notify: Option<&NotifyConfig>) -> Option<Duration> {
    match notify?.poll_seconds.unwrap_or(DEFAULT_NOTIFY_POLL_SECONDS) {
        0 => None,
        seconds => Some(Duration::from_secs(seconds)),
    }
}

/// Background attention poll: keeps webhook notifications firing while no
/// browser is polling `/api/cockpit`. Webhooks stay quiet while a browser is
/// connected.
fn spawn_notify_tick<C, B>(state: &WebAppState<C, B>)
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let period = {
        let guard = state.shared();
        notify_poll_interval(guard.context.config.notify.as_ref())
    };
    let Some(period) = period else {
        return;
    };
    let tick_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.tick().await; // consume the immediate first tick
        loop {
            interval.tick().await;
            if tick_state.browser_connected() {
                continue;
            }
            let _ = refresh_cockpit_and_cache(&tick_state, true).await;
        }
    });
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
    if let Some(config) = state.cloudflare_access.as_ref() {
        if let Err(error) = config.verify_headers(request.headers()) {
            return cloudflare_access_error_response(error);
        }
    }
    if state.browser_session.is_present(request.headers()) {
        return next.run(request).await;
    }
    json_value_response(
        401,
        serde_json::json!({ "ok": false, "error": "browser session required" }),
    )
}

fn cloudflare_access_error_response(error: CloudflareAccessError) -> AxumResponse {
    json_value_response(
        error.status_code(),
        serde_json::json!({ "ok": false, "error": error.client_message() }),
    )
}

async fn axum_app_css() -> AxumResponse {
    static_asset_response("/app.css")
}

async fn axum_app_js() -> AxumResponse {
    static_asset_response("/app.js")
}

async fn axum_terminal_js() -> AxumResponse {
    static_asset_response("/terminal.js")
}

async fn axum_health() -> AxumResponse {
    json_value_response(200, serde_json::json!({ "ok": true }))
}

async fn axum_version() -> AxumResponse {
    json_value_response(
        200,
        serde_json::json!({
            "version": install::app_version(),
            "test_in_stable": server::test_in_stable_enabled_from_env(),
        }),
    )
}

async fn axum_server_restart() -> AxumResponse {
    handle_server_restart().into_axum_response()
}

async fn axum_server_test_in_stable() -> AxumResponse {
    handle_server_test_in_stable().into_axum_response()
}

fn handle_server_restart() -> Response {
    server::schedule_process_restart();
    Response {
        status_code: 200,
        content_type: "application/json; charset=utf-8",
        body: br#"{"ok":true,"restarting":true}"#.to_vec(),
    }
}

fn handle_server_test_in_stable() -> Response {
    if !server::test_in_stable_enabled_from_env() {
        return Response {
            status_code: 404,
            content_type: "application/json; charset=utf-8",
            body: br#"{"ok":false,"error":"test in stable is not available"}"#.to_vec(),
        };
    }
    server::schedule_test_in_stable();
    Response {
        status_code: 200,
        content_type: "application/json; charset=utf-8",
        body: br#"{"ok":true,"restarting":true}"#.to_vec(),
    }
}

#[derive(Deserialize)]
struct DevDeployRequest {
    task_handle: String,
}

async fn axum_dev_deploy_status<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let status = dev_deploy::lock_slot(&state.dev_deploy).status();
    json_value_response(200, serde_json::json!({ "ok": true, "deploy": status }))
}

async fn axum_dev_deploy_start<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    handle_dev_deploy_start(&state, &body).into_axum_response()
}

fn handle_dev_deploy_start<C, B>(state: &WebAppState<C, B>, body: &[u8]) -> Response
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let request: DevDeployRequest = match serde_json::from_slice(body) {
        Ok(request) => request,
        Err(_) => {
            return json_response(
                400,
                serde_json::json!({ "ok": false, "error": "invalid JSON body" }),
            )
            .unwrap_or_else(|error| response_from_web_error(error, None));
        }
    };

    let source = {
        let guard = state.shared();
        match dev_deploy::resolve_ajax_dev_deploy_source(&guard.context, &request.task_handle) {
            Ok(source) => source,
            Err(error) => {
                let status = match &error {
                    dev_deploy::DevDeployError::Busy => 409,
                    dev_deploy::DevDeployError::TaskNotFound(_) => 404,
                    _ => 400,
                };
                return json_response(
                    status,
                    serde_json::json!({ "ok": false, "error": error.to_string() }),
                )
                .unwrap_or_else(|error| response_from_web_error(error, None));
            }
        }
    };

    let script = match dev_deploy::resolve_restart_script(&source.worktree_path) {
        Ok(script) => script,
        Err(error) => {
            return json_response(
                500,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            )
            .unwrap_or_else(|error| response_from_web_error(error, None));
        }
    };

    {
        let mut slot = dev_deploy::lock_slot(&state.dev_deploy);
        if let Err(error) = slot.begin(&source) {
            let status = if matches!(error, dev_deploy::DevDeployError::Busy) {
                409
            } else {
                400
            };
            return json_response(
                status,
                serde_json::json!({ "ok": false, "error": error.to_string() }),
            )
            .unwrap_or_else(|error| response_from_web_error(error, None));
        }
    }

    let slot = Arc::clone(&state.dev_deploy);
    let worktree = source.worktree_path.clone();
    thread::spawn(move || {
        run_test_in_dev_job(slot, script, source, worktree);
    });

    let status = dev_deploy::lock_slot(&state.dev_deploy).status();
    json_response(
        202,
        serde_json::json!({
            "ok": true,
            "deploy": status,
            "message": "Test in Dev started for the shared Ajax Dev slot"
        }),
    )
    .unwrap_or_else(|error| response_from_web_error(error, None))
}

fn run_test_in_dev_job(
    slot: Arc<dev_deploy::SharedDevDeploySlot>,
    script: PathBuf,
    source: dev_deploy::DevDeploySource,
    worktree: PathBuf,
) {
    let mut child = match ProcessCommand::new(&script)
        .args(dev_deploy::test_in_dev_command_args(&worktree))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            dev_deploy::lock_slot(&slot)
                .set_failed(format!("could not spawn restart script: {error}"));
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let slot_for_stdout = Arc::clone(&slot);
    let stdout_thread = stdout.map(|stdout| {
        thread::spawn(move || {
            let mut log = String::new();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if line.contains("AJAX_DEV_DEPLOY_PHASE=restarting") {
                    dev_deploy::lock_slot(&slot_for_stdout).set_restarting();
                }
                log.push_str(&line);
                log.push('\n');
            }
            log
        })
    });
    let stderr_thread = stderr.map(|stderr| {
        thread::spawn(move || {
            let mut log = String::new();
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log.push_str(&line);
                log.push('\n');
            }
            log
        })
    });

    let status = child.wait();
    let stdout_log = stdout_thread
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr_log = stderr_thread
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let combined = format!("{stdout_log}{stderr_log}");

    match status {
        Ok(status) if status.success() => {
            dev_deploy::lock_slot(&slot).set_ready(&source);
        }
        Ok(status) => {
            let tail = combined
                .lines()
                .rev()
                .take(12)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            let message = if tail.is_empty() {
                format!("dev deploy failed with status {status}")
            } else {
                format!("dev deploy failed with status {status}\n{tail}")
            };
            // Build/restart failure leaves the previous running instance (script
            // restores the prior slot binary when restart fails after install).
            dev_deploy::lock_slot(&slot).set_failed(message);
        }
        Err(error) => {
            dev_deploy::lock_slot(&slot).set_failed(format!("dev deploy wait failed: {error}"));
        }
    }
}

async fn axum_cockpit<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    state.mark_browser_cockpit_seen();
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    if let Ok(_refresh_guard) = state.control_lane.try_lock() {
        return refresh_cockpit_and_cache_locked(&state, false);
    }

    let guard = state.shared();
    match serde_json::to_value(cockpit::browser_cockpit_view(&guard.context)) {
        Ok(value) => json_value_response(200, value),
        Err(error) => web_error_response(WebError::JsonSerialization(error.to_string())),
    }
}

/// Refresh the cockpit projection and cache the response, firing attention
/// notifications through the bridge as a side effect. The cockpit handler,
/// the background notify tick, and task mutations/task starts all serialize on
/// the same control lane, so a mutation cannot race an in-flight refresh and
/// discard its committed state.
async fn refresh_cockpit_and_cache<C, B>(
    state: &WebAppState<C, B>,
    deliver_notifications: bool,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let _refresh_guard = state.control_lane.lock().await;
    refresh_cockpit_and_cache_locked(state, deliver_notifications)
}

fn refresh_cockpit_and_cache_locked<C, B>(
    state: &WebAppState<C, B>,
    deliver_notifications: bool,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    if let Some(response) = state.cached_cockpit_response() {
        return response.into_axum_response();
    }

    let (mut context, mut runner, mut bridge, base_revision) = {
        let guard = state.shared();
        (
            guard.context.clone(),
            guard.runner.clone(),
            guard.bridge.clone(),
            guard.revision,
        )
    };
    let result = handle_refreshed_cockpit_request(
        &mut context,
        &mut runner,
        &mut bridge,
        deliver_notifications,
    );
    let cached_response = match &result {
        Ok(response) => Some(response.clone()),
        Err(_) => None,
    };
    {
        let mut guard = state.shared();
        if guard.revision == base_revision {
            guard.context = context;
            guard.bridge = bridge;
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

async fn axum_task_detail<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let guard = state.shared();
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
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if req.uri().path().ends_with("/terminal") {
        let Some(task_handle) = handle.strip_suffix("/terminal") else {
            return json_value_response(
                404,
                serde_json::json!({ "ok": false, "error": "not found" }),
            );
        };
        return axum_task_terminal(State(state), task_handle.to_string(), req).await;
    }
    if handle.ends_with("/snapshot") {
        return json_value_response(
            404,
            serde_json::json!({ "ok": false, "error": "not found" }),
        );
    }
    axum_task_detail::<C, B>(State(state), handle).await
}

async fn axum_task_terminal<C, B>(
    State(state): State<WebAppState<C, B>>,
    handle: String,
    req: AxumRequest,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + Sync + 'static,
    B: RuntimeBridge<C> + Clone + Send + Sync + 'static,
{
    if !req
        .headers()
        .get(header::UPGRADE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"))
    {
        return text_axum_response(400, "websocket upgrade required");
    }
    if !websocket_origin_allowed(req.headers()) {
        return text_axum_response(403, "websocket origin forbidden");
    }
    // A same-origin browser client reached the terminal socket; refresh
    // cockpit presence so the notify tick stays suppressed while it is open.
    state.mark_browser_cockpit_seen();

    let plan = {
        let guard = state.shared();
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

    let seed_history = crate::adapters::terminal_pty::seed_history_from_query(req.uri().query());
    let on_operator_input = operator_input_sink(&state, plan.qualified_handle.clone());
    let (mut parts, body) = req.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &state).await {
        Ok(upgrade) => upgrade,
        Err(_) => return text_axum_response(400, "websocket upgrade required"),
    };
    let _ = body;
    upgrade.on_upgrade(move |socket| async move {
        crate::adapters::terminal_pty::bridge_task_terminal_socket(
            socket,
            plan,
            seed_history,
            on_operator_input,
        )
        .await;
    })
}

fn websocket_origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    origin_authority(origin).is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

fn origin_authority(origin: &str) -> Option<&str> {
    let (scheme, rest) = origin.split_once("://")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let authority = rest.split('/').next()?;
    if authority.is_empty() || authority.contains('@') || authority.contains('\\') {
        return None;
    }
    Some(authority)
}

async fn axum_task_post<C, B>(
    State(_state): State<WebAppState<C, B>>,
    AxumPath(_handle): AxumPath<String>,
    _body: Bytes,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    json_value_response(
        404,
        serde_json::json!({ "ok": false, "error": "not found" }),
    )
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
    if !crate::slices::operate::supported_start_agent(&request.agent) {
        return json_value_response(
            400,
            serde_json::json!({
                "ok": false,
                "request_id": request_id,
                "error": format!("unsupported agent: {}", request.agent),
            }),
        );
    }
    let task_key = ajax_core::commands::start_task_identity(&request.repo, &request.title)
        .as_str()
        .to_string();
    if let Err(rejection) = state.operations().try_begin(Some(&request_id), &task_key) {
        return gate_rejection_response(rejection, Some(&request_id), &task_key, "task start");
    }
    let _lane = state.control_lane.lock().await;
    let response = state.run_optimistic(
        Some(&request_id),
        "cockpit state changed while task start was running",
        |context, runner, bridge| {
            let result = match bridge.execute_start_task(request, context, runner) {
                Ok(outcome) => operation_success_response(outcome, context),
                Err(error) => operation_error_response(error, context),
            };
            match result {
                Ok(response) => operation_response_with_request_id(response, Some(&request_id)),
                Err(error) => response_from_web_error(error, Some(&request_id)),
            }
        },
    );
    state
        .operations()
        .finish(Some(&request_id), &task_key, &response);
    response.into_axum_response()
}

/// Turn a gate rejection into the route response: replay the completed
/// response or report that a `{noun} already in progress` conflict.
fn gate_rejection_response(
    rejection: GateRejection,
    request_id: Option<&str>,
    task: &str,
    noun: &str,
) -> AxumResponse {
    match rejection {
        GateRejection::Replay(response) => {
            tracing::warn!(
                target: "ajax_web",
                request_id = ?request_id,
                task = %task,
                outcome = "replay",
                "operate gate"
            );
            response.into_axum_response()
        }
        GateRejection::Conflict => {
            tracing::warn!(
                target: "ajax_web",
                request_id = ?request_id,
                task = %task,
                outcome = "conflict",
                "operate gate"
            );
            json_value_response(
                409,
                serde_json::json!({
                    "ok": false,
                    "request_id": request_id,
                    "error": format!("{noun} already in progress"),
                }),
            )
        }
    }
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
    // The browser is actively driving an operate/action; refresh cockpit
    // presence so the background notify tick stays suppressed while it works.
    state.mark_browser_cockpit_seen();
    let request_id = request.request_id.clone();
    let task_key = request.task_handle.clone();
    let action = request.action.clone();
    if let Err(rejection) = state
        .operations()
        .try_begin(request_id.as_deref(), &task_key)
    {
        return gate_rejection_response(rejection, request_id.as_deref(), &task_key, "operation");
    }

    tracing::info!(
        target: "ajax_web",
        request_id = ?request_id,
        task = %task_key,
        action = %action,
        "operate begin"
    );

    let _lane = state.control_lane.lock().await;
    let response = state.run_optimistic(
        request_id.as_deref(),
        "cockpit state changed while operation was running",
        |context, runner, bridge| match handle_action_request(request, context, runner, bridge) {
            Ok(response) => operation_response_with_request_id(response, request_id.as_deref()),
            Err(error) => response_from_web_error(error, request_id.as_deref()),
        },
    );

    state
        .operations()
        .finish(request_id.as_deref(), &task_key, &response);

    if response.status_code >= 400 {
        tracing::warn!(
            target: "ajax_web",
            request_id = ?request_id,
            task = %task_key,
            action = %action,
            status = response.status_code,
            outcome = "err",
            "operate end"
        );
    } else {
        tracing::info!(
            target: "ajax_web",
            request_id = ?request_id,
            task = %task_key,
            action = %action,
            status = response.status_code,
            outcome = "ok",
            "operate end"
        );
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
        deliver_notifications: bool,
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

    /// Acknowledge operator attention for `task_handle` (e.g. the operator typed
    /// in the Web Cockpit terminal). Returns `true` when the acknowledgment
    /// advanced the task state and so callers should invalidate any cached
    /// cockpit projection; `Ok(false)` means no-ack (recently acknowledged, no
    /// newer live evidence, etc.). Errors are dropped by the sink caller; the
    /// default body preserves existing (pre-ack) behavior.
    fn acknowledge_operator_input(
        &mut self,
        _context: &mut CommandContext<InMemoryRegistry>,
        _task_handle: &str,
    ) -> Result<bool, WebError> {
        Ok(false)
    }
}

#[derive(Clone, Deserialize, serde::Serialize)]
struct MobileActionRequest {
    #[serde(default)]
    request_id: Option<String>,
    task_handle: String,
    action: String,
    #[serde(default)]
    confirmed: bool,
    #[serde(default)]
    branch_adoption: Option<ajax_core::commands::BranchAdoptionPlan>,
}

fn handle_refreshed_cockpit_request<C: CommandRunner>(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut C,
    bridge: &mut impl RuntimeBridge<C>,
    deliver_notifications: bool,
) -> Result<Response, WebError> {
    let _state_changed =
        bridge.refresh_cockpit(context, runner, RefreshTier::Live, deliver_notifications)?;
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
            confirmed: request.confirmed,
            branch_adoption: request.branch_adoption,
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

#[cfg(test)]
mod tests {
    use super::{ActionFailure, RefreshTier, RuntimeBridge};
    use crate::slices::operate::{operate, OperateError, OperateOutcome, OperateRequest};
    use ajax_core::{
        adapters::{
            CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
        },
        commands::CommandContext,
        config::Config,
        registry::InMemoryRegistry,
    };
    use axum::{
        body::{to_bytes, Body},
        http::{Request as AxumRequest, StatusCode},
    };
    use std::{
        collections::BTreeSet,
        io::{Read, Write},
        sync::atomic::{AtomicUsize, Ordering},
        sync::{Arc, Condvar, Mutex},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use tokio::sync::Notify;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct TestBridge {
        refreshed: bool,
        refresh_tier: Option<RefreshTier>,
        deliver_notifications_flags: Vec<bool>,
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
        refresh_calls: Arc<AtomicUsize>,
        refresh_entered: Option<Arc<Notify>>,
        refresh_release: Option<Arc<(Mutex<bool>, Condvar)>>,
        acknowledge_calls: Arc<AtomicUsize>,
        acknowledge_result: Result<bool, crate::WebError>,
    }

    impl Default for TestBridge {
        fn default() -> Self {
            Self {
                refreshed: false,
                refresh_tier: None,
                deliver_notifications_flags: Vec::new(),
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
                refresh_calls: Arc::new(AtomicUsize::new(0)),
                refresh_entered: None,
                refresh_release: None,
                acknowledge_calls: Arc::new(AtomicUsize::new(0)),
                acknowledge_result: Ok(false),
            }
        }
    }

    /// Block the first bridge call until the test releases the gate; later
    /// calls pass straight through.
    fn wait_for_release(release: &Option<Arc<(Mutex<bool>, Condvar)>>, call_index: usize) {
        if call_index != 0 {
            return;
        }
        if let Some(release) = release.as_ref() {
            let (lock, cvar) = &**release;
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

    fn release_gate(release: &Arc<(Mutex<bool>, Condvar)>) {
        let (lock, cvar) = &**release;
        let mut released = lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *released = true;
        cvar.notify_all();
    }

    impl<R: CommandRunner> RuntimeBridge<R> for TestBridge {
        fn refresh_cockpit(
            &mut self,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut R,
            tier: RefreshTier,
            deliver_notifications: bool,
        ) -> Result<bool, crate::WebError> {
            if self.refresh_delay > Duration::ZERO {
                std::thread::sleep(self.refresh_delay);
            }
            let call_index = self.refresh_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.refresh_entered.as_ref() {
                entered.notify_one();
            }
            wait_for_release(&self.refresh_release, call_index);
            self.refreshed = true;
            self.refresh_tier = Some(tier);
            self.deliver_notifications_flags.push(deliver_notifications);
            self.refresh_count += 1;
            Ok(false)
        }

        fn execute_operate(
            &mut self,
            request: OperateRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut R,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.operate_count += 1;
            let call_index = self.operate_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.operate_entered.as_ref() {
                entered.notify_one();
            }
            wait_for_release(&self.operate_release, call_index);
            std::thread::sleep(self.operate_delay);
            self.operate = Some(request);
            self.operate_result.clone()
        }

        fn execute_start_task(
            &mut self,
            request: crate::slices::operate::StartTaskRequest,
            _context: &mut CommandContext<InMemoryRegistry>,
            _runner: &mut R,
        ) -> Result<OperateOutcome, ActionFailure> {
            self.start_count += 1;
            let call_index = self.start_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(entered) = self.start_entered.as_ref() {
                entered.notify_one();
            }
            wait_for_release(&self.start_release, call_index);
            self.start = Some(request);
            self.start_result.clone()
        }

        fn acknowledge_operator_input(
            &mut self,
            _context: &mut CommandContext<InMemoryRegistry>,
            _task_handle: &str,
        ) -> Result<bool, crate::WebError> {
            self.acknowledge_calls.fetch_add(1, Ordering::SeqCst);
            self.acknowledge_result.clone()
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
        crate::test_support::context_with_fix_login_task()
    }

    fn context_with_web_repo() -> CommandContext<InMemoryRegistry> {
        crate::test_support::context_with_tasks(&["web"], vec![])
    }

    fn context_with_two_tasks() -> CommandContext<InMemoryRegistry> {
        crate::test_support::context_with_tasks(
            &["web", "api"],
            vec![
                crate::test_support::fix_login_task(),
                crate::test_support::task_in("api", "fix-auth", "Fix auth"),
            ],
        )
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

    const TEST_CF_ACCESS_ISSUER: &str = "https://test.cloudflareaccess.com";
    const TEST_CF_ACCESS_AUD: &str = "test-audience";
    const TEST_CF_ACCESS_SECRET: &[u8] = b"ajax-test-cloudflare-access-secret";

    fn cloudflare_access_config_for_test(
        allowed_emails: Option<&[&str]>,
    ) -> super::CloudflareAccessConfig {
        let allowed_emails = allowed_emails.map(|emails| {
            emails
                .iter()
                .map(|email| email.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
        });
        super::CloudflareAccessConfig::hmac_for_test(
            TEST_CF_ACCESS_ISSUER,
            TEST_CF_ACCESS_AUD,
            TEST_CF_ACCESS_SECRET,
            allowed_emails,
        )
    }

    #[derive(serde::Serialize)]
    struct TestCloudflareAccessClaims {
        aud: Vec<String>,
        iss: String,
        exp: u64,
        nbf: u64,
        iat: u64,
        email: String,
        #[serde(rename = "type")]
        token_type: String,
    }

    fn cloudflare_access_token_for_test(email: &str) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let claims = TestCloudflareAccessClaims {
            aud: vec![TEST_CF_ACCESS_AUD.to_string()],
            iss: TEST_CF_ACCESS_ISSUER.to_string(),
            exp: now + 300,
            nbf: now.saturating_sub(60),
            iat: now,
            email: email.to_string(),
            token_type: "app".to_string(),
        };
        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
        header.kid = Some("test-key".to_string());
        jsonwebtoken::encode(
            &header,
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(TEST_CF_ACCESS_SECRET),
        )
        .unwrap()
    }

    fn browser_session_cookie<C, B>(state: &super::WebAppState<C, B>) -> String {
        state.browser_session.cookie_pair()
    }

    fn authenticated_request(cookie: &str, uri: &str) -> axum::http::request::Builder {
        AxumRequest::builder().uri(uri).header("cookie", cookie)
    }

    /// State + session cookie + router for an `OkRunner`-backed test app.
    fn app_with(
        context: CommandContext<InMemoryRegistry>,
        bridge: TestBridge,
        tag: &str,
    ) -> (
        super::WebAppState<OkRunner, TestBridge>,
        String,
        axum::Router,
    ) {
        let state = super::WebAppState::new(context, OkRunner, bridge, scratch_dir(tag));
        let cookie = browser_session_cookie(&state);
        let app = super::axum_app(state.clone());
        (state, cookie, app)
    }

    /// GET without a browser-session cookie (public shell/asset routes and
    /// 401 checks).
    async fn get_public(app: &axum::Router, path: &str) -> axum::response::Response {
        app.clone()
            .oneshot(
                AxumRequest::builder()
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    /// The `name=value` pair of the browser-session cookie a response set.
    fn set_cookie_pair(response: &axum::response::Response) -> String {
        response
            .headers()
            .get("set-cookie")
            .expect("response should set browser session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    async fn get(app: &axum::Router, cookie: &str, path: &str) -> axum::response::Response {
        app.clone()
            .oneshot(
                authenticated_request(cookie, path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn get_with_access(
        app: &axum::Router,
        cookie: &str,
        path: &str,
        token: &str,
    ) -> axum::response::Response {
        app.clone()
            .oneshot(
                authenticated_request(cookie, path)
                    .header("cf-access-jwt-assertion", token)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn websocket_get(
        app: &axum::Router,
        cookie: &str,
        path: &str,
        origin: Option<&str>,
    ) -> axum::response::Response {
        let mut request = authenticated_request(cookie, path)
            .header("host", "localhost")
            .header("connection", "upgrade")
            .header("upgrade", "websocket")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==");
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        app.clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn post_json(
        app: &axum::Router,
        cookie: &str,
        path: &str,
        body: &str,
    ) -> axum::response::Response {
        app.clone()
            .oneshot(
                authenticated_request(cookie, path)
                    .method("POST")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn json_of(response: axum::response::Response) -> serde_json::Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[test]
    fn axum_api_access_policy_classifies_public_and_protected_routes() {
        use super::ApiAccess;

        for (method, path) in [
            ("GET", "/"),
            ("GET", "/index.html"),
            ("GET", "/app.js"),
            ("GET", "/terminal.js"),
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
            ("POST", "/api/server/test-in-stable"),
            ("GET", "/api/dev-deploy"),
            ("POST", "/api/dev-deploy"),
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

        let shell = get_public(&app, "/").await;
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
        assert_eq!(shell.headers()["cache-control"], "no-store");
        let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&shell_body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = get(&app, &session_cookie, "/api/cockpit").await;
        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(
            cockpit.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        assert_eq!(cockpit.headers()["cache-control"], "no-store");
        assert_eq!(json_of(cockpit).await["cards"], serde_json::json!([]));

        let missing_api = get(&app, &session_cookie, "/api/missing").await;
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
            let retired_asset = get_public(&app, path).await;
            assert_eq!(retired_asset.status(), StatusCode::NOT_FOUND, "{path}");
            assert_eq!(
                retired_asset.headers()["content-type"],
                "text/plain; charset=utf-8",
                "{path}"
            );
        }

        let missing_asset = get_public(&app, "/does-not-exist.css").await;
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
    async fn static_shell_assets_are_no_store_and_gzipped() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) =
            app_with(context, TestBridge::default(), "axum-static-cache-gzip");

        let version = crate::slices::install::app_version();
        for path in ["/app.js", "/app.css", "/terminal.js"] {
            for request_path in [path, &format!("{path}?v={version}")] {
                let response = get_public(&app, request_path).await;
                assert_eq!(response.status(), StatusCode::OK, "{request_path}");
                let cache_control = response.headers()["cache-control"]
                    .to_str()
                    .unwrap_or_default();
                assert_eq!(cache_control, "no-store", "{request_path} must not cache");
                assert!(
                    response.headers().get("etag").is_none(),
                    "{request_path} must not carry an ETag"
                );
                assert!(
                    !cache_control.contains("immutable"),
                    "{request_path} must not claim immutability"
                );
            }
        }

        // HTML shell and API remain no-store (do not get the immutable cache).
        let shell = get_public(&app, "/").await;
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()["cache-control"], "no-store");

        let cockpit = get(&app, &cookie, "/api/cockpit").await;
        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(cockpit.headers()["cache-control"], "no-store");

        // Negotiated gzip applies to compressible static JS when requested.
        let app_js_gz = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/app.js")
                    .header("accept-encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(app_js_gz.status(), StatusCode::OK);
        assert_eq!(app_js_gz.headers()["content-encoding"], "gzip");
    }

    #[tokio::test]
    async fn static_shell_assets_ignore_if_none_match() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, _cookie, app) = app_with(context, TestBridge::default(), "axum-static-etag");

        let etag = format!("W/\"{}\"", crate::slices::install::app_version());

        for path in ["/app.js", "/app.css", "/terminal.js"] {
            let baseline = get_public(&app, path).await;
            assert_eq!(baseline.status(), StatusCode::OK, "{path}");
            assert_eq!(
                baseline.headers()["cache-control"],
                "no-store",
                "{path} cache-control"
            );
            assert!(
                baseline.headers().get("etag").is_none(),
                "{path} must not carry an ETag"
            );
            let baseline_body = to_bytes(baseline.into_body(), usize::MAX).await.unwrap();

            // Matching If-None-Match must still return 200 with a body, never 304.
            let matched = app
                .clone()
                .oneshot(
                    AxumRequest::builder()
                        .uri(path)
                        .header("if-none-match", &etag)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(matched.status(), StatusCode::OK, "{path} if-none-match");
            assert_eq!(
                matched.headers()["cache-control"],
                "no-store",
                "{path} if-none-match cache-control"
            );
            assert!(
                matched.headers().get("etag").is_none(),
                "{path} if-none-match must not carry an ETag"
            );
            let matched_body = to_bytes(matched.into_body(), usize::MAX).await.unwrap();
            assert_eq!(matched_body, baseline_body, "{path} if-none-match body");

            // Stale If-None-Match: same no-store 200 with a non-empty body.
            let stale = app
                .clone()
                .oneshot(
                    AxumRequest::builder()
                        .uri(path)
                        .header("if-none-match", "W/\"stale\"")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(stale.status(), StatusCode::OK, "{path} stale");
            assert_eq!(
                stale.headers()["cache-control"],
                "no-store",
                "{path} stale cache-control"
            );
            assert!(
                stale.headers().get("etag").is_none(),
                "{path} stale must not carry an ETag"
            );
            let stale_body = to_bytes(stale.into_body(), usize::MAX).await.unwrap();
            assert_eq!(stale_body, baseline_body, "{path} stale body");
        }

        // gzip + If-None-Match must still return 200 with a body, never 304.
        let gz_matched = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/app.js")
                    .header("accept-encoding", "gzip")
                    .header("if-none-match", &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            gz_matched.status(),
            StatusCode::OK,
            "/app.js gzip+if-none-match"
        );
        assert_eq!(
            gz_matched.headers()["cache-control"],
            "no-store",
            "/app.js gzip+if-none-match cache-control"
        );
        assert!(
            gz_matched.headers().get("etag").is_none(),
            "/app.js gzip+if-none-match must not carry an ETag"
        );
        assert_eq!(gz_matched.headers()["content-encoding"], "gzip");
        let gz_matched_body = to_bytes(gz_matched.into_body(), usize::MAX).await.unwrap();
        assert!(
            !gz_matched_body.is_empty(),
            "/app.js gzip+if-none-match body must not be empty"
        );

        let gz_ok = app
            .clone()
            .oneshot(
                AxumRequest::builder()
                    .uri("/app.js")
                    .header("accept-encoding", "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(gz_ok.status(), StatusCode::OK, "/app.js gzip 200");
        assert_eq!(gz_ok.headers()["cache-control"], "no-store");
        assert!(
            gz_ok.headers().get("etag").is_none(),
            "/app.js gzip 200 must not carry an ETag"
        );

        // The HTML shell keeps no-store and gains no ETag.
        let shell = get_public(&app, "/").await;
        assert_eq!(shell.status(), StatusCode::OK);
        assert!(
            shell.headers().get("etag").is_none(),
            "shell must not carry an ETag"
        );
        assert_eq!(shell.headers()["cache-control"], "no-store");
    }

    #[tokio::test]
    async fn shell_uses_bare_static_asset_urls() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, _cookie, app) =
            app_with(context, TestBridge::default(), "axum-static-version-urls");

        let shell_body = to_bytes(get_public(&app, "/").await.into_body(), usize::MAX)
            .await
            .unwrap();
        let shell = std::str::from_utf8(&shell_body).unwrap();
        assert!(
            shell.contains("src=\"/app.js\""),
            "shell must load app.js at a bare URL"
        );
        assert!(
            shell.contains("href=\"/app.css\""),
            "shell must load app.css at a bare URL"
        );
        assert!(
            !shell.contains("src=\"/app.js?"),
            "shell must not cache-bust app.js with a query string"
        );
        assert!(
            !shell.contains("href=\"/app.css?"),
            "shell must not cache-bust app.css with a query string"
        );

        let app_js_body = to_bytes(get_public(&app, "/app.js").await.into_body(), usize::MAX)
            .await
            .unwrap();
        let app_js = std::str::from_utf8(&app_js_body).unwrap();
        assert!(
            app_js.contains("import(\"./terminal.js\")"),
            "app.js must keep the deferred terminal.js import at a bare URL"
        );
        for versioned_edge in [
            "\"./app.js?v=",
            "\"./terminal.js?v=",
            "import(\"./terminal.js?v=",
        ] {
            assert!(
                !app_js.contains(versioned_edge),
                "served app.js must not rewrite module edges with {versioned_edge}"
            );
        }
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

        let shell = get_public(&app, "/").await;
        let session_cookie = set_cookie_pair(&shell);
        assert!(session_cookie.starts_with("ajax_browser_session="));

        assert_eq!(
            get_public(&app, "/api/health").await.status(),
            StatusCode::OK
        );
        assert_eq!(
            get_public(&app, "/api/cockpit").await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            get(&app, &session_cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );
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

        assert_eq!(
            get_public(&app, "/api/cockpit").await.status(),
            StatusCode::UNAUTHORIZED
        );

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
        let session_cookie = set_cookie_pair(&renewal);
        assert!(session_cookie.starts_with("ajax_browser_session="));

        assert_eq!(
            get(&app, &session_cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );
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
        let guard = state.shared();
        assert_eq!(guard.revision, 0);
        assert!(!guard.bridge.refreshed);
        assert_eq!(guard.bridge.operate_count, 0);
        assert_eq!(guard.bridge.start_count, 0);
    }

    #[tokio::test]
    async fn cloudflare_access_enabled_rejects_missing_jwt_on_protected_routes() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("cf-access-missing"),
        )
        .with_cloudflare_access_for_test(cloudflare_access_config_for_test(None));
        let cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);

        let response = get(&app, &cookie, "/api/cockpit").await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = json_of(response).await;
        assert_eq!(body["ok"], false);
        assert!(body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("Cloudflare Access"));
    }

    #[tokio::test]
    async fn cloudflare_access_enabled_accepts_valid_jwt_on_protected_routes() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let state = super::WebAppState::new(
            context,
            OkRunner,
            TestBridge::default(),
            scratch_dir("cf-access-valid"),
        )
        .with_cloudflare_access_for_test(cloudflare_access_config_for_test(None));
        let cookie = browser_session_cookie(&state);
        let app = super::axum_app(state);
        let token = cloudflare_access_token_for_test("operator@example.com");

        let response = get_with_access(&app, &cookie, "/api/cockpit", &token).await;

        assert_eq!(response.status(), StatusCode::OK);
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
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "axum-cockpit-cache",
        );

        for _ in 0..2 {
            assert_eq!(
                get(&app, &cookie, "/api/cockpit").await.status(),
                StatusCode::OK
            );
        }

        assert_eq!(state.shared().bridge.refresh_count, 1);
    }

    #[test]
    fn browser_connected_is_false_until_marked_and_expires_after_ttl() {
        let state = super::WebAppState::new(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            OkRunner,
            TestBridge::default(),
            scratch_dir("browser-connected-ttl"),
        );

        assert!(!state.browser_connected());
        state.mark_browser_cockpit_seen();
        assert!(state.browser_connected());

        let aged = Instant::now() - super::BROWSER_CONNECTED_TTL - Duration::from_secs(1);
        state.set_browser_cockpit_seen_at_for_test(aged);
        assert!(!state.browser_connected());
    }

    #[tokio::test]
    async fn axum_cockpit_marks_browser_connected_even_on_cache_hit() {
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "axum-cockpit-browser-connected",
        );

        for _ in 0..2 {
            assert_eq!(
                get(&app, &cookie, "/api/cockpit").await.status(),
                StatusCode::OK
            );
        }

        assert!(state.browser_connected());
    }

    #[tokio::test]
    async fn axum_operations_marks_browser_connected() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(
            context,
            TestBridge::default(),
            "axum-operations-browser-connected",
        );

        assert!(!state.browser_connected());

        let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
        let response = post_json(&app, &cookie, "/api/operations", operation).await;
        assert_eq!(response.status(), StatusCode::OK);

        assert!(state.browser_connected());
    }

    #[tokio::test]
    async fn refresh_cockpit_and_cache_passes_deliver_notifications_flag() {
        let (state, _cookie, _app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "deliver-notifications-flag",
        );

        super::refresh_cockpit_and_cache(&state, false).await;
        assert_eq!(
            state.shared().bridge.deliver_notifications_flags,
            vec![false]
        );

        tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

        super::refresh_cockpit_and_cache(&state, true).await;
        assert_eq!(
            state.shared().bridge.deliver_notifications_flags,
            vec![false, true]
        );
    }

    #[test]
    fn notify_poll_interval_maps_config() {
        use ajax_core::config::NotifyConfig;

        assert_eq!(super::notify_poll_interval(None), None);

        let base = NotifyConfig {
            webhook_url: "https://ntfy.sh/topic".to_string(),
            poll_seconds: None,
        };
        assert_eq!(
            super::notify_poll_interval(Some(&base)),
            Some(Duration::from_secs(30))
        );

        let disabled = NotifyConfig {
            poll_seconds: Some(0),
            ..base.clone()
        };
        assert_eq!(super::notify_poll_interval(Some(&disabled)), None);

        let custom = NotifyConfig {
            poll_seconds: Some(90),
            ..base
        };
        assert_eq!(
            super::notify_poll_interval(Some(&custom)),
            Some(Duration::from_secs(90))
        );
    }

    #[tokio::test]
    async fn refresh_cockpit_and_cache_refreshes_once_and_caches() {
        let (state, _cookie, _app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "tick-refresh-cache",
        );

        super::refresh_cockpit_and_cache(&state, true).await;
        assert_eq!(state.shared().bridge.refresh_count, 1);

        // Within the cache TTL the tick shares the handler's cached response.
        super::refresh_cockpit_and_cache(&state, true).await;
        assert_eq!(state.shared().bridge.refresh_count, 1);
    }

    #[tokio::test]
    async fn axum_cockpit_refreshes_again_after_ttl_expires() {
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "axum-cockpit-ttl",
        );

        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );

        tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );

        assert_eq!(state.shared().bridge.refresh_count, 2);
    }

    #[tokio::test]
    async fn axum_operation_invalidates_cockpit_refresh_cache() {
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "axum-cockpit-invalidate",
        );

        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );

        let operation = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"request_id":"invalidate-1","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await;
        assert_eq!(operation.status(), StatusCode::OK);

        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );

        assert_eq!(state.shared().bridge.refresh_count, 2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_cockpit_polls_share_one_refresh() {
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                refresh_delay: Duration::from_millis(200),
                ..TestBridge::default()
            },
            "axum-cockpit-single-flight",
        );

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first =
            tokio::spawn(async move { get(&first_app, &first_cookie, "/api/cockpit").await });
        tokio::time::sleep(Duration::from_millis(25)).await;
        let second = get(&app, &cookie, "/api/cockpit").await;

        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
        assert_eq!(second.status(), StatusCode::OK);

        assert_eq!(state.shared().bridge.refresh_count, 1);
    }

    #[tokio::test]
    async fn axum_router_reports_shell_version() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "axum-version");

        let response = get(&app, &cookie, "/api/version").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let value = json_of(response).await;
        let version = value["version"].as_str().expect("version string");
        assert!(version.starts_with(env!("CARGO_PKG_VERSION")));
        assert_eq!(version, crate::slices::install::app_version());
        assert_eq!(value["test_in_stable"], false);
    }

    #[tokio::test]
    async fn axum_operation_preserves_branch_adoption_confirmation() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "adoption-confirm");

        let response = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"task_handle":"web/fix-login","action":"repair","confirmed":true,"branch_adoption":{"expected_branch":"ajax/fix-login","observed_branch":"fix/pane-stuck"}}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            state.shared().bridge.operate,
            Some(OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "repair".to_string(),
                confirmed: true,
                branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                    expected_branch: "ajax/fix-login".to_string(),
                    observed_branch: "fix/pane-stuck".to_string(),
                }),
            })
        );
    }

    #[tokio::test]
    async fn axum_operations_are_idempotent_by_request_id() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "axum-idempotency");

        let operation = r#"{"request_id":"req-1","task_handle":"web/fix-login","action":"review"}"#;
        let first = post_json(&app, &cookie, "/api/operations", operation).await;
        assert_eq!(first.status(), StatusCode::OK);
        let first_json = json_of(first).await;
        assert_eq!(first_json["ok"], true);
        assert_eq!(first_json["request_id"], "req-1");
        assert!(first_json["cockpit"].is_object());

        let second = post_json(&app, &cookie, "/api/operations", operation).await;
        assert_eq!(second.status(), StatusCode::OK);
        assert_eq!(json_of(second).await, first_json);

        assert_eq!(state.shared().bridge.operate_count, 1);
    }

    #[tokio::test]
    async fn axum_task_starts_are_idempotent_by_request_id() {
        let (state, cookie, app) = app_with(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            TestBridge::default(),
            "axum-start-idempotency",
        );
        let request =
            r#"{"request_id":"start-1","repo":"web","title":"Fix login","agent":"codex"}"#;

        for _ in 0..2 {
            let response = post_json(&app, &cookie, "/api/tasks", request).await;
            assert_eq!(response.status(), StatusCode::OK);
            let json = json_of(response).await;
            assert_eq!(json["ok"], true);
            assert_eq!(json["request_id"], "start-1");
            assert!(json["cockpit"].is_object());
        }

        assert_eq!(state.shared().bridge.start_count, 1);
    }

    #[tokio::test]
    async fn axum_task_start_rejects_unsupported_agent_before_bridge() {
        let (state, cookie, app) = app_with(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            TestBridge::default(),
            "axum-start-agent-allowlist",
        );

        let response = post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-shell","repo":"web","title":"Fix login","agent":"/bin/sh"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let json = json_of(response).await;
        assert_eq!(json["ok"], false);
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unsupported agent"));
        assert_eq!(state.shared().bridge.start_count, 0);
    }

    #[tokio::test]
    async fn axum_operation_parse_errors_are_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "axum-json-error");

        let response = post_json(&app, &cookie, "/api/operations", "{not-json").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let json = json_of(response).await;
        assert_eq!(json["ok"], false);
        assert!(json["error"].as_str().unwrap_or_default().contains("json"));
    }

    #[tokio::test]
    async fn operation_endpoint_returns_refreshed_cockpit_on_bridge_error() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                operate_result: Err(ActionFailure {
                    message: "bridge failed".to_string(),
                    state_changed: true,
                }),
                ..TestBridge::default()
            },
            "axum-operation-error",
        );

        let response = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"request_id":"op-error-1","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let json = json_of(response).await;
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
        let (state, cookie, app) = app_with(
            context_with_web_repo(),
            TestBridge {
                start_entered: Some(Arc::clone(&entered)),
                start_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-start-collision",
        );

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first = tokio::spawn(async move {
            post_json(
                &first_app,
                &first_cookie,
                "/api/tasks",
                r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first start request never entered the bridge");

        let conflict = tokio::time::timeout(
            Duration::from_secs(5),
            post_json(
                &app,
                &cookie,
                "/api/tasks",
                r#"{"request_id":"start-b","repo":"web","title":"Fix login!","agent":"codex"}"#,
            ),
        )
        .await
        .expect("second start request timed out");

        release_gate(&release);

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let json = json_of(conflict).await;
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
        assert_eq!(state.shared().bridge.start_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_start_task_rejects_when_action_operation_is_in_flight_before_bridge_side_effects()
    {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_two_tasks(),
            TestBridge {
                operate_entered: Some(Arc::clone(&entered)),
                operate_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-start-blocked-by-action",
        );

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first = tokio::spawn(async move {
            post_json(
                &first_app,
                &first_cookie,
                "/api/operations",
                r#"{"request_id":"op-a","task_handle":"web/fix-login","action":"review"}"#,
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first operation request never entered the bridge");

        let conflict = tokio::time::timeout(
            Duration::from_secs(5),
            post_json(
                &app,
                &cookie,
                "/api/tasks",
                r#"{"request_id":"start-a","repo":"web","title":"Start while action runs","agent":"codex"}"#,
            ),
        )
        .await
        .expect("start request timed out");

        release_gate(&release);

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let json = json_of(conflict).await;
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
        let guard = state.shared();
        assert_eq!(guard.bridge.start_calls.load(Ordering::SeqCst), 0);
        assert_eq!(guard.bridge.operate_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_start_task_duplicate_request_id_does_not_clear_original_in_flight_marker() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_web_repo(),
            TestBridge {
                start_entered: Some(Arc::clone(&entered)),
                start_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-start-duplicate-request-id",
        );

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first = tokio::spawn(async move {
            post_json(
                &first_app,
                &first_cookie,
                "/api/tasks",
                r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first start request never entered the bridge");

        let duplicate_same_task = post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-a","repo":"web","title":"Fix login","agent":"codex"}"#,
        )
        .await;
        assert_eq!(duplicate_same_task.status(), StatusCode::CONFLICT);

        let duplicate_different_task = tokio::time::timeout(
            Duration::from_secs(5),
            post_json(
                &app,
                &cookie,
                "/api/tasks",
                r#"{"request_id":"start-a","repo":"web","title":"Different task","agent":"codex"}"#,
            ),
        )
        .await
        .expect("duplicate start request timed out");

        release_gate(&release);

        assert_eq!(duplicate_different_task.status(), StatusCode::CONFLICT);
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), first)
                .await
                .expect("first start request timed out")
                .unwrap()
                .status(),
            StatusCode::OK
        );
        assert_eq!(state.shared().bridge.start_calls.load(Ordering::SeqCst), 1);
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
        let (_state, cookie, app) = app_with(
            CommandContext::new(Config::default(), InMemoryRegistry::default()),
            TestBridge {
                start_result: Err(ActionFailure {
                    message: "start failed".to_string(),
                    state_changed: true,
                }),
                ..TestBridge::default()
            },
            "axum-start-error",
        );

        let response = post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"request_id":"start-error-1","repo":"web","title":"Fix login","agent":"codex"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::CONFLICT);
        let json = json_of(response).await;
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
        let (state, cookie, app) = app_with(context, bridge, "axum-conflict");

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first = tokio::spawn(async move {
            post_json(
                &first_app,
                &first_cookie,
                "/api/operations",
                r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
            )
            .await
        });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let conflict = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"request_id":"req-b","task_handle":"web/fix-login","action":"ship"}"#,
        )
        .await;

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let json = json_of(conflict).await;
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "req-b");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already in progress"));

        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
        assert_eq!(
            state.shared().bridge.operate_calls.load(Ordering::SeqCst),
            1
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_rejects_concurrent_different_task_operations_before_bridge_side_effects() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_two_tasks(),
            TestBridge {
                operate_entered: Some(Arc::clone(&entered)),
                operate_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-concurrent-different-tasks",
        );

        let first_app = app.clone();
        let first_cookie = cookie.clone();
        let first = tokio::spawn(async move {
            post_json(
                &first_app,
                &first_cookie,
                "/api/operations",
                r#"{"request_id":"req-a","task_handle":"web/fix-login","action":"review"}"#,
            )
            .await
        });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("first request never entered the bridge");

        let conflict = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"request_id":"req-b","task_handle":"api/fix-auth","action":"ship"}"#,
        )
        .await;

        release_gate(&release);

        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let json = json_of(conflict).await;
        assert_eq!(json["ok"], false);
        assert_eq!(json["request_id"], "req-b");
        assert!(json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("already in progress"));

        assert_eq!(first.await.unwrap().status(), StatusCode::OK);
        assert_eq!(state.shared().bridge.operate_count, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn axum_health_stays_responsive_during_slow_cockpit_refresh() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                refresh_delay: Duration::from_millis(400),
                ..TestBridge::default()
            },
            "axum-health-cockpit",
        );

        let slow_app = app.clone();
        let slow_cookie = cookie.clone();
        let cockpit =
            tokio::spawn(async move { get(&slow_app, &slow_cookie, "/api/cockpit").await });

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
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                refresh_delay: Duration::from_millis(250),
                ..TestBridge::default()
            },
            "axum-refresh-operation-race",
        );

        let refresh_app = app.clone();
        let refresh_cookie = cookie.clone();
        let cockpit =
            tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });
        tokio::time::sleep(Duration::from_millis(25)).await;

        let operation = post_json(
            &app,
            &cookie,
            "/api/operations",
            r#"{"request_id":"req-race","task_handle":"web/fix-login","action":"review"}"#,
        )
        .await;

        assert_eq!(operation.status(), StatusCode::OK);
        assert_eq!(cockpit.await.unwrap().status(), StatusCode::OK);

        let guard = state.shared();
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

    #[tokio::test]
    async fn runtime_routes_to_vertical_slices() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "routes");

        let shell = get_public(&app, "/").await;
        assert_eq!(shell.status(), StatusCode::OK);
        assert_eq!(shell.headers()["content-type"], "text/html; charset=utf-8");
        let shell_body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
        assert!(std::str::from_utf8(&shell_body)
            .unwrap()
            .contains("Ajax Cockpit"));

        let cockpit = get(&app, &cookie, "/api/cockpit").await;
        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(
            cockpit.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        assert_eq!(json_of(cockpit).await["cards"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn cockpit_api_refreshes_before_rendering() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "refresh");

        let response = get(&app, &cookie, "/api/cockpit").await;

        assert_eq!(response.status(), StatusCode::OK);
        let bridge = &state.shared().bridge;
        assert!(bridge.refreshed);
        assert_eq!(bridge.refresh_tier, Some(RefreshTier::Live));
    }

    #[tokio::test]
    async fn server_restart_endpoint_returns_restarting_json() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "restart");

        let response = post_json(&app, &cookie, "/api/server/restart", "").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_of(response).await;
        assert_eq!(body["ok"], true);
        assert_eq!(body["restarting"], true);
    }

    struct EnvVarGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: ajax-web runtime tests are not run in parallel with other
            // env-mutating tests in this module.
            unsafe { std::env::set_var(key, value) };
            Self { key, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[tokio::test]
    async fn test_in_stable_endpoint_returns_not_found_when_disabled() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) =
            app_with(context, TestBridge::default(), "test-in-stable-disabled");

        let response = post_json(&app, &cookie, "/api/server/test-in-stable", "").await;

        assert_json_not_found(response, "test in stable is not available").await;
    }

    #[tokio::test]
    async fn test_in_stable_endpoint_returns_restarting_when_enabled() {
        let _script = EnvVarGuard::set(
            "AJAX_WEB_RESTART_SCRIPT",
            "/repo/scripts/dev-web-restart.sh",
        );
        let _profile = EnvVarGuard::set("AJAX_WEB_RESTART_PROFILE", "stable");
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) =
            app_with(context, TestBridge::default(), "test-in-stable-enabled");

        let response = post_json(&app, &cookie, "/api/server/test-in-stable", "").await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_of(response).await;
        assert_eq!(body["ok"], true);
        assert_eq!(body["restarting"], true);
    }

    #[tokio::test]
    async fn dev_deploy_status_and_reject_non_ajax_paths() {
        use ajax_core::{
            config::ManagedRepo,
            models::{AgentClient, Task, TaskId},
            registry::Registry as _,
        };

        let mut registry = InMemoryRegistry::default();
        registry
            .create_task(Task::new(
                TaskId::new("autosnooze/other"),
                "autosnooze",
                "other",
                "Other",
                "feat/other",
                "main",
                "/tmp/other",
                "ajax-autosnooze-other",
                "task",
                AgentClient::Codex,
            ))
            .unwrap();
        let context = CommandContext::new(
            Config {
                repos: vec![
                    ManagedRepo::new("ajax-cli", "/Users/matt/Desktop/Projects/ajax-cli", "main"),
                    ManagedRepo::new("autosnooze", "/tmp/autosnooze", "main"),
                ],
                ..Config::default()
            },
            registry,
        );
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "dev-deploy");

        let status = get(&app, &cookie, "/api/dev-deploy").await;
        assert_eq!(status.status(), StatusCode::OK);
        let status_body = json_of(status).await;
        assert_eq!(status_body["ok"], true);
        assert_eq!(status_body["deploy"]["shared_slot"], true);
        assert!(status_body["deploy"].get("open_url").is_none());
        assert_eq!(status_body["deploy"]["phase"], "ready_to_deploy");

        let rejected = post_json(
            &app,
            &cookie,
            "/api/dev-deploy",
            r#"{"task_handle":"autosnooze/other"}"#,
        )
        .await;
        assert_eq!(rejected.status(), StatusCode::BAD_REQUEST);
        let rejected_body = json_of(rejected).await;
        assert_eq!(rejected_body["ok"], false);
        assert!(rejected_body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("ajax-cli"));
    }

    #[tokio::test]
    async fn action_endpoint_executes_bridge_action_and_returns_cockpit() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "action");

        let response = post_json(
            &app,
            &cookie,
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"review"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_of(response).await;
        assert_eq!(body["ok"], true);
        assert_eq!(body["state_changed"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            state.shared().bridge.operate,
            Some(OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "review".to_string(),
                confirmed: false,
                branch_adoption: None,
            })
        );
    }

    #[tokio::test]
    async fn get_task_detail_returns_json_for_existing_handle() {
        let (_state, cookie, app) = app_with(context_with_task(), TestBridge::default(), "detail");

        let response = get(&app, &cookie, "/api/tasks/web/fix-login").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let body = json_of(response).await;
        assert_eq!(body["qualified_handle"], "web/fix-login");
        assert_eq!(body["title"], "Fix login");
        assert_eq!(body["branch"], "ajax/fix-login");
    }

    #[tokio::test]
    async fn get_task_detail_allows_encoded_handle_named_terminal() {
        let task = crate::test_support::task_in("ajax-cli", "terminal", "Terminal");
        let context = crate::test_support::context_with_tasks(&["ajax-cli"], vec![task]);
        let (_state, cookie, app) =
            app_with(context, TestBridge::default(), "detail-terminal-handle");

        let response = get(&app, &cookie, "/api/tasks/ajax-cli%2Fterminal").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let body = json_of(response).await;
        assert_eq!(body["qualified_handle"], "ajax-cli/terminal");
        assert_eq!(body["title"], "Terminal");
        assert_eq!(body["branch"], "ajax/terminal");
    }

    #[tokio::test]
    async fn axum_task_terminal_requires_browser_session_cookie() {
        let state = super::WebAppState::new(
            context_with_task(),
            OkRunner,
            TestBridge::default(),
            scratch_dir("terminal-auth"),
        );
        let app = super::axum_app(state);

        let response = get_public(&app, "/api/tasks/web%2Ffix-login/terminal").await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn axum_task_terminal_rejects_non_upgrade_requests() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "terminal-upgrade",
        );

        let response = get(&app, &cookie, "/api/tasks/web%2Ffix-login/terminal").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "websocket upgrade required"
        );
    }

    #[tokio::test]
    async fn axum_task_terminal_rejects_cross_site_websocket_origin() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "terminal-cross-origin",
        );

        let response = websocket_get(
            &app,
            &cookie,
            "/api/tasks/web%2Ffix-login/terminal",
            Some("https://evil.example"),
        )
        .await;

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(
            std::str::from_utf8(&body).unwrap(),
            "websocket origin forbidden"
        );
    }

    #[tokio::test]
    async fn axum_task_terminal_marks_browser_connected_after_origin_ok() {
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "terminal-same-origin-browser-connected",
        );

        assert!(!state.browser_connected());

        let response = websocket_get(
            &app,
            &cookie,
            "/api/tasks/web%2Ffix-login/terminal",
            Some("https://localhost"),
        )
        .await;

        // The same-origin request passed the websocket origin gate; the exact
        // upgrade outcome (101/400) is irrelevant once the handler ran past it.
        assert_ne!(response.status(), StatusCode::FORBIDDEN);

        assert!(state.browser_connected());
    }

    #[test]
    fn websocket_origin_policy_accepts_same_origin_host() {
        let request = AxumRequest::builder()
            .header("host", "localhost")
            .header("origin", "https://localhost")
            .body(Body::empty())
            .unwrap();

        assert!(super::websocket_origin_allowed(request.headers()));
    }

    /// Assert a JSON 404 body with the expected `error` string.
    async fn assert_json_not_found(response: axum::response::Response, error: &str) {
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers()["content-type"],
            "application/json; charset=utf-8"
        );
        let body = json_of(response).await;
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], error);
    }

    #[tokio::test]
    async fn axum_task_keys_route_is_not_supported() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "terminal-keys-removed",
        );
        let response = post_json(&app, &cookie, "/api/tasks/web%2Ffix-login/keys", "{}").await;

        assert_json_not_found(response, "not found").await;
    }

    #[tokio::test]
    async fn axum_task_snapshot_route_is_not_supported() {
        let (_state, cookie, app) = app_with(
            context_with_task(),
            TestBridge::default(),
            "terminal-snapshot-removed",
        );
        let response = get(&app, &cookie, "/api/tasks/web%2Ffix-login/snapshot").await;

        assert_json_not_found(response, "not found").await;
    }

    #[tokio::test]
    async fn get_task_detail_returns_text_404_for_unknown_handle() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "detail-missing");

        let response = get(&app, &cookie, "/api/tasks/web/missing").await;

        assert_json_not_found(response, "task not found").await;
    }

    #[tokio::test]
    async fn unknown_in_memory_api_path_stays_generic_404() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "missing-api");

        let response = get(&app, &cookie, "/api/missing").await;

        assert_json_not_found(response, "not found").await;
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

    #[tokio::test]
    async fn post_tasks_endpoint_delegates_to_start_bridge_method() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "start");

        let response = post_json(
            &app,
            &cookie,
            "/api/tasks",
            r#"{"repo":"web","title":"Fix login","agent":"codex","request_id":"req-1"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_of(response).await;
        assert_eq!(body["ok"], true);
        assert!(body["cockpit"].is_object());
        assert_eq!(
            state.shared().bridge.start,
            Some(crate::slices::operate::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                request_id: "req-1".to_string(),
            })
        );
    }

    #[tokio::test]
    async fn action_endpoint_keeps_start_out_of_bridge() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (state, cookie, app) = app_with(context, TestBridge::default(), "native-action");

        let response = post_json(
            &app,
            &cookie,
            "/api/actions",
            r#"{"task_handle":"web/fix-login","action":"start"}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(state.shared().bridge.operate, None);
    }

    #[tokio::test]
    async fn push_path_does_not_start_nested_runtime() {
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
        let (_state, cookie, app) = app_with(context, TestBridge::default(), "push-runtime");

        let config = get(&app, &cookie, "/api/push/config").await;
        assert_eq!(config.status(), StatusCode::NOT_FOUND);

        let subscribe = post_json(
            &app,
            &cookie,
            "/api/push/subscribe",
            r#"{"endpoint":"https://push.example/x","keys":{"p256dh":"k","auth":"a"}}"#,
        )
        .await;
        assert_eq!(subscribe.status(), StatusCode::NOT_FOUND);

        let unsubscribe = post_json(
            &app,
            &cookie,
            "/api/push/unsubscribe",
            r#"{"endpoint":"https://push.example/x"}"#,
        )
        .await;
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_cockpit_returns_current_projection_while_control_lane_is_busy() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                refresh_entered: Some(Arc::clone(&entered)),
                refresh_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-cockpit-busy-lane",
        );

        let refresh_app = app.clone();
        let refresh_cookie = cookie.clone();
        let first_cockpit =
            tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("cockpit refresh never entered the bridge");

        let concurrent_app = app.clone();
        let concurrent_cookie = cookie.clone();
        let second_cockpit =
            tokio::spawn(
                async move { get(&concurrent_app, &concurrent_cookie, "/api/cockpit").await },
            );

        let second_result = tokio::time::timeout(Duration::from_millis(150), second_cockpit).await;

        release_gate(&release);

        let second = second_result
            .expect("second cockpit GET should complete promptly while refresh holds the lane")
            .unwrap();

        assert_eq!(second.status(), StatusCode::OK);
        let body = json_of(second).await;
        assert!(
            body["cards"]
                .as_array()
                .expect("cockpit body should include cards")
                .iter()
                .any(|card| card["qualified_handle"] == "web/fix-login"),
            "fallback cockpit read should include the existing web/fix-login card"
        );
        assert_eq!(
            state.shared().bridge.refresh_calls.load(Ordering::SeqCst),
            1,
            "busy-lane cockpit read must not start another refresh"
        );

        let first = tokio::time::timeout(Duration::from_secs(5), first_cockpit)
            .await
            .expect("first cockpit refresh response timed out")
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        tokio::time::sleep(super::COCKPIT_REFRESH_CACHE_TTL + Duration::from_millis(50)).await;

        assert_eq!(
            get(&app, &cookie, "/api/cockpit").await.status(),
            StatusCode::OK
        );
        assert_eq!(
            state.shared().bridge.refresh_count,
            2,
            "later polls should still use the normal refresh path after TTL expires"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_operation_waits_for_slow_cockpit_refresh_and_preserves_refresh_state() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_task(),
            TestBridge {
                refresh_entered: Some(Arc::clone(&entered)),
                refresh_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-op-waits-refresh",
        );

        let refresh_app = app.clone();
        let refresh_cookie = cookie.clone();
        let cockpit =
            tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("cockpit refresh never entered the bridge");

        let mutate_app = app.clone();
        let mutate_cookie = cookie.clone();
        let mutation = tokio::spawn(async move {
            post_json(
                &mutate_app,
                &mutate_cookie,
                "/api/operations",
                r#"{"request_id":"op-wait-1","task_handle":"web/fix-login","action":"review"}"#,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        let operate_calls_during_refresh =
            state.shared().bridge.operate_calls.load(Ordering::SeqCst);

        release_gate(&release);

        let cockpit = tokio::time::timeout(Duration::from_secs(5), cockpit)
            .await
            .expect("cockpit refresh response timed out")
            .unwrap();
        let mutation = tokio::time::timeout(Duration::from_secs(5), mutation)
            .await
            .expect("operation response timed out")
            .unwrap();

        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(mutation.status(), StatusCode::OK);
        assert_eq!(
            operate_calls_during_refresh, 0,
            "operation must wait for the in-flight cockpit refresh (control lane)"
        );
        assert_eq!(
            state.shared().bridge.refresh_count,
            1,
            "refresh state must be committed, not discarded by the racing mutation"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn axum_task_start_waits_for_slow_cockpit_refresh_and_preserves_refresh_state() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let (state, cookie, app) = app_with(
            context_with_web_repo(),
            TestBridge {
                refresh_entered: Some(Arc::clone(&entered)),
                refresh_release: Some(Arc::clone(&release)),
                ..TestBridge::default()
            },
            "axum-start-waits-refresh",
        );

        let refresh_app = app.clone();
        let refresh_cookie = cookie.clone();
        let cockpit =
            tokio::spawn(async move { get(&refresh_app, &refresh_cookie, "/api/cockpit").await });

        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .expect("cockpit refresh never entered the bridge");

        let mutate_app = app.clone();
        let mutate_cookie = cookie.clone();
        let mutation = tokio::spawn(async move {
            post_json(
                &mutate_app,
                &mutate_cookie,
                "/api/tasks",
                r#"{"request_id":"start-wait-1","repo":"web","title":"Fix login","agent":"codex"}"#,
            )
            .await
        });

        tokio::time::sleep(Duration::from_millis(200)).await;
        let start_calls_during_refresh = state.shared().bridge.start_calls.load(Ordering::SeqCst);

        release_gate(&release);

        let cockpit = tokio::time::timeout(Duration::from_secs(5), cockpit)
            .await
            .expect("cockpit refresh response timed out")
            .unwrap();
        let mutation = tokio::time::timeout(Duration::from_secs(5), mutation)
            .await
            .expect("start task response timed out")
            .unwrap();

        assert_eq!(cockpit.status(), StatusCode::OK);
        assert_eq!(mutation.status(), StatusCode::OK);
        assert_eq!(
            start_calls_during_refresh, 0,
            "task start must wait for the in-flight cockpit refresh (control lane)"
        );
        assert_eq!(
            state.shared().bridge.refresh_count,
            1,
            "refresh state must be committed, not discarded by the racing mutation"
        );
    }

    fn state_with_bridge_and_task(bridge: TestBridge) -> super::WebAppState<OkRunner, TestBridge> {
        super::WebAppState::new(
            context_with_task(),
            OkRunner,
            bridge,
            scratch_dir("operator-input-sink"),
        )
    }

    #[test]
    fn operator_input_sink_calls_bridge_once_and_bumps_revision_and_clears_cache_when_acknowledged()
    {
        let bridge = TestBridge {
            acknowledge_result: Ok(true),
            ..TestBridge::default()
        };
        let acknowledge_calls = Arc::clone(&bridge.acknowledge_calls);
        let state = state_with_bridge_and_task(bridge);
        {
            let mut guard = state.shared();
            guard.cockpit_cache = Some(super::CockpitCacheEntry {
                response: super::Response {
                    status_code: 200,
                    content_type: "application/json; charset=utf-8",
                    body: Vec::new(),
                },
                cached_at: Instant::now(),
                revision: 0,
            });
        }
        let base_revision = state.shared().revision;

        let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
        sink();
        sink();

        assert_eq!(acknowledge_calls.load(Ordering::SeqCst), 2);
        let guard = state.shared();
        assert_eq!(guard.revision, base_revision + 2);
        assert!(guard.cockpit_cache.is_none());
    }

    #[test]
    fn operator_input_sink_leaves_revision_and_cache_untouched_when_bridge_returns_false() {
        let bridge = TestBridge {
            acknowledge_result: Ok(false),
            ..TestBridge::default()
        };
        let acknowledge_calls = Arc::clone(&bridge.acknowledge_calls);
        let state = state_with_bridge_and_task(bridge);
        {
            let mut guard = state.shared();
            guard.cockpit_cache = Some(super::CockpitCacheEntry {
                response: super::Response {
                    status_code: 200,
                    content_type: "application/json; charset=utf-8",
                    body: Vec::new(),
                },
                cached_at: Instant::now(),
                revision: 0,
            });
        }
        let base_revision = state.shared().revision;
        let cache_was = state.shared().cockpit_cache.is_some();

        let sink = super::operator_input_sink(&state, "web/fix-login".to_string());
        sink();

        assert_eq!(acknowledge_calls.load(Ordering::SeqCst), 1);
        let guard = state.shared();
        assert_eq!(guard.revision, base_revision);
        assert_eq!(guard.cockpit_cache.is_some(), cache_was);
    }

    #[test]
    fn logging_web_listening_writes_to_ajax_log() {
        let logs_dir = logging_test_logs_dir();
        super::log_web_listening("127.0.0.1", 9443);

        let contents = read_logging_test_log(logs_dir);
        assert!(
            contents.contains("listening"),
            "expected listening in log: {contents}"
        );
        assert!(
            contents.contains("127.0.0.1"),
            "expected host in log: {contents}"
        );
        assert!(
            contents.contains("9443"),
            "expected port in log: {contents}"
        );
    }

    #[test]
    fn logging_operate_unknown_action_includes_action_field() {
        let logs_dir = logging_test_logs_dir();
        let mut context = context_with_task();
        let mut runner = RecordingCommandRunner::default();

        let error = operate(
            &mut context,
            &mut runner,
            OperateRequest {
                task_handle: "web/fix-login".to_string(),
                action: "not-a-real-action".to_string(),
                confirmed: false,
                branch_adoption: None,
            },
        )
        .unwrap_err();

        assert!(matches!(error, OperateError::UnknownAction(_)));

        let contents = read_logging_test_log(logs_dir);
        assert!(
            contents.contains("action=") && contents.contains("not-a-real-action"),
            "expected action field in log: {contents}"
        );
        assert!(
            contents.contains("outcome=\"err\"") || contents.contains("outcome=err"),
            "expected err outcome in log: {contents}"
        );
    }

    fn logging_test_logs_dir() -> &'static std::path::Path {
        use std::{
            path::PathBuf,
            sync::{Mutex, OnceLock},
        };

        static LOGS_DIR: OnceLock<PathBuf> = OnceLock::new();
        static INIT: Mutex<()> = Mutex::new(());

        let _guard = INIT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        LOGS_DIR.get_or_init(|| {
            let logs_dir =
                std::env::temp_dir().join(format!("ajax_web_logging_tests_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&logs_dir);
            ajax_core::logging::init_to_logs_dir(&logs_dir);
            logs_dir
        })
    }

    fn read_logging_test_log(logs_dir: &std::path::Path) -> String {
        std::fs::read_to_string(logs_dir.join("ajax.log")).expect("ajax.log should exist")
    }
}
