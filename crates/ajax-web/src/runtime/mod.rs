//! Web companion runtime wiring.

use crate::{
    adapters::{
        browser_session::BrowserSession, cloudflare_access::CloudflareAccessError, server, tls,
    },
    slices::{dev_deploy, install},
    WebError,
};
use ajax_core::{adapters::CommandRunner, config::NotifyConfig};
use axum::{
    body::Bytes,
    extract::{Request as AxumRequest, State},
    http::Uri,
    middleware::{from_fn_with_state, Next},
    response::Response as AxumResponse,
    routing::{get, post},
    serve::Listener,
    Router,
};
use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    process::{Command as ProcessCommand, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};
use tower_http::compression::CompressionLayer;

pub use crate::adapters::http::Response;

use crate::adapters::http::{
    bytes_axum_response, html_response, json_response, json_value_response,
    response_from_web_error, text_axum_response,
};

mod bridge;
mod state;
mod task_routes;
use task_routes::{
    axum_action, axum_cockpit, axum_start_task, axum_task_get, axum_task_post,
    refresh_cockpit_and_cache,
};

pub use bridge::{ActionFailure, RuntimeBridge};
pub use state::{operator_input_sink, WebAppState};
use state::{DEFAULT_NOTIFY_POLL_SECONDS, TLS_HANDSHAKE_TIMEOUT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiAccess {
    Public,
    BrowserSessionRequired,
}

pub(crate) fn api_access_policy(method: &str, path: &str) -> ApiAccess {
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

pub(crate) fn notify_poll_interval(notify: Option<&NotifyConfig>) -> Option<Duration> {
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

pub(crate) struct TlsListener {
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

pub(crate) fn browser_session_json_response(session: &BrowserSession) -> AxumResponse {
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

#[cfg(test)]
pub(crate) use crate::adapters::cloudflare_access::CloudflareAccessConfig;
#[cfg(test)]
pub(crate) use ajax_core::runtime_refresh::RefreshTier;
#[cfg(test)]
pub(crate) use bridge::operation_success_response;
#[cfg(test)]
pub(crate) use state::{
    CockpitCacheEntry, OperationCoordinator, BROWSER_CONNECTED_TTL, COCKPIT_REFRESH_CACHE_TTL,
};
#[cfg(test)]
pub(crate) use task_routes::websocket_origin_allowed;

#[cfg(test)]
mod tests;
