//! Web companion runtime wiring.

use crate::{
    adapters::{
        browser_session::BrowserSession, cloudflare_access::CloudflareAccessError, server, tls,
    },
    slices::{dev_deploy, install, push},
    WebError,
};
use ajax_core::adapters::CommandRunner;
pub(crate) use ajax_core::runtime_refresh::RefreshTier;
use axum::{
    body::Bytes,
    extract::{rejection::JsonRejection, Request as AxumRequest, State},
    http::{HeaderMap, StatusCode, Uri},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Response as AxumResponse},
    routing::{delete, get, post},
    serve::Listener,
    Json, Router,
};
use serde::Deserialize;
use std::{
    net::{SocketAddr, ToSocketAddrs},
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
mod server_routes;
mod state;
mod task_routes;
use task_routes::{
    axum_action, axum_cockpit, axum_start_task, axum_task_get, axum_task_post,
    refresh_cockpit_and_cache,
};

pub use bridge::{ActionFailure, RuntimeBridge};
use state::TLS_HANDSHAKE_TIMEOUT;
pub use state::{operator_input_sink, WebAppState};

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
        .route("/api/session/models", get(axum_session_models))
        .route("/api/push/vapid", get(axum_push_vapid::<C, B>))
        .route("/api/push/subscribe", post(axum_push_subscribe::<C, B>))
        .route("/api/push/subscribe", delete(axum_push_unsubscribe::<C, B>))
        .route("/api/push/test", post(axum_push_test::<C, B>))
        .route("/api/version", get(axum_version))
        .route(
            "/api/server/runtime",
            get(server_routes::axum_server_runtime),
        )
        .route(
            "/api/server/restart",
            post(server_routes::axum_server_restart),
        )
        .route(
            "/api/server/update",
            post(server_routes::axum_server_update),
        )
        .route(
            "/api/server/test-in-stable",
            post(server_routes::axum_server_test_in_stable),
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
        push::spawn_push_flusher(Arc::clone(&state.push));
        spawn_push_tick(&state);
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

/// Background Full refresh always advances low-frequency runtime and CI
/// evidence. Browser presence and push subscriptions gate only web push.
fn spawn_push_tick<C, B>(state: &WebAppState<C, B>)
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let period = Duration::from_secs(push::DEFAULT_PUSH_POLL_SECONDS);
    let tick_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.tick().await; // consume the immediate first tick
        loop {
            interval.tick().await;
            let (tier, deliver_push) = background_refresh_plan(
                tick_state.browser_connected(),
                tick_state.push.has_subscriptions(),
            );
            let _ = refresh_cockpit_and_cache(&tick_state, tier, deliver_push).await;
        }
    });
}

fn background_refresh_plan(
    browser_connected: bool,
    has_subscriptions: bool,
) -> (RefreshTier, bool) {
    (RefreshTier::Full, !browser_connected && has_subscriptions)
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
        serde_json::json!({ "ok": false, "error": "browser session required", "code": "stale_session" }),
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

async fn axum_session_models(uri: axum::http::Uri) -> AxumResponse {
    let agent = uri
        .query()
        .and_then(|query| {
            query.split('&').find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == "agent").then_some(value)
            })
        })
        .unwrap_or("cursor")
        .to_string();
    // Reading a bridge catalog spawns a short-lived process; keep it off the
    // async worker so the event loop is not blocked on stdio.
    match tokio::task::spawn_blocking(move || {
        crate::slices::session_models::list_session_models(&agent)
    })
    .await
    {
        Ok(response) => Json(response).into_response(),
        Err(_) => json_value_response(
            500,
            serde_json::json!({ "ok": false, "error": "model catalog worker failed" }),
        ),
    }
}

async fn axum_version() -> AxumResponse {
    json_value_response(
        200,
        serde_json::json!({
            "version": install::app_version(),
            "test_in_stable": server::test_in_stable_enabled_from_env(),
            "profile": server::resolved_web_profile_from_env(),
        }),
    )
}

async fn axum_push_vapid<C, B>(State(state): State<WebAppState<C, B>>) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    match state.push.vapid_public_key_base64() {
        Ok(public_key) => Json(serde_json::json!({ "public_key": public_key })).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": error })),
        )
            .into_response(),
    }
}

async fn axum_push_subscribe<C, B>(
    State(state): State<WebAppState<C, B>>,
    headers: HeaderMap,
    body: Result<Json<push::PushSubscription>, JsonRejection>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let Json(subscription) = match body {
        Ok(subscription) => subscription,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": error.body_text() })),
            )
                .into_response();
        }
    };
    let navigate = match push::navigation_url(&headers) {
        Ok(navigate) => navigate,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": error })),
            )
                .into_response();
        }
    };
    match state.push.upsert_subscription(subscription, &navigate) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(error) => {
            let status = if error.contains("subscription")
                || error.contains("endpoint")
                || error.contains("Origin")
                || error.contains("Host")
                || error.contains("navigate")
            {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(serde_json::json!({ "ok": false, "error": error })),
            )
                .into_response()
        }
    }
}

async fn axum_push_unsubscribe<C, B>(
    State(state): State<WebAppState<C, B>>,
    body: Result<Json<push::UnsubscribeRequest>, JsonRejection>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let Json(request) = match body {
        Ok(request) => request,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": error.body_text() })),
            )
                .into_response();
        }
    };
    match state.push.apply_unsubscribe(&request) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "ok": false, "error": error })),
        )
            .into_response(),
    }
}

async fn axum_push_test<C, B>(
    State(state): State<WebAppState<C, B>>,
    headers: HeaderMap,
    body: Result<Json<push::PushTestRequest>, JsonRejection>,
) -> AxumResponse
where
    C: CommandRunner + Clone + Send + 'static,
    B: RuntimeBridge<C> + Clone + Send + 'static,
{
    let Json(request) = match body {
        Ok(request) => request,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "ok": false, "error": error.body_text() })),
            )
                .into_response();
        }
    };
    match push::schedule_test_push(&state.push, &headers, request) {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "ok": true, "scheduled": true })),
        )
            .into_response(),
        Err(error) => {
            // Never return 502: Cloudflare surfaces origin 502 as a host error page.
            let status = if error.contains("subscription") || error.contains("endpoint") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(serde_json::json!({ "ok": false, "error": error })),
            )
                .into_response()
        }
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
        dev_deploy::run_test_in_dev_job(slot, script, source, worktree);
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
pub(crate) use bridge::operation_success_response;
#[cfg(test)]
pub(crate) use state::{
    CockpitCacheEntry, OperationCoordinator, BROWSER_CONNECTED_TTL, COCKPIT_REFRESH_CACHE_TTL,
};
#[cfg(test)]
pub(crate) use task_routes::websocket_origin_allowed;

#[cfg(test)]
mod tests;
