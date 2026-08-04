// Re-export runtime items so suite_* grandchildren can `use super::*` / `super::X`.
pub(super) use super::{
    api_access_policy, axum_app, browser_session_json_response, log_web_listening,
    operation_success_response, operator_input_sink, refresh_cockpit_and_cache,
    websocket_origin_allowed, ActionFailure, ApiAccess, CockpitCacheEntry, OperationCoordinator,
    RefreshTier, Response, RuntimeBridge, TlsListener, WebAppState, BROWSER_CONNECTED_TTL,
    COCKPIT_REFRESH_CACHE_TTL,
};

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
pub(super) struct TestBridge {
    refreshed: bool,
    refresh_tier: Option<RefreshTier>,
    deliver_notifications_flags: Vec<bool>,
    refresh_count: usize,
    operate: Option<OperateRequest>,
    operate_count: usize,
    operate_delay: Duration,
    start_delay: Duration,
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
            start_delay: Duration::ZERO,
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
pub(super) fn wait_for_release(release: &Option<Arc<(Mutex<bool>, Condvar)>>, call_index: usize) {
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

pub(super) fn release_gate(release: &Arc<(Mutex<bool>, Condvar)>) {
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
        std::thread::sleep(self.start_delay);
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
pub(super) struct OkRunner;

impl CommandRunner for OkRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone)]
pub(super) struct SlowDiffRunner {
    delay: Duration,
    entered: Option<Arc<Notify>>,
}

impl CommandRunner for SlowDiffRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        if let Some(entered) = self.entered.as_ref() {
            entered.notify_one();
        }
        std::thread::sleep(self.delay);
        Ok(CommandOutput {
            status_code: 0,
            stdout: "[]".to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone, Copy)]
pub(super) struct PanickingDiffRunner;

impl CommandRunner for PanickingDiffRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        panic!("diff runner panic")
    }
}

pub(super) fn context_with_task() -> CommandContext<InMemoryRegistry> {
    crate::test_support::context_with_fix_login_task()
}

pub(super) fn context_with_web_repo() -> CommandContext<InMemoryRegistry> {
    crate::test_support::context_with_tasks(&["web"], vec![])
}

pub(super) fn context_with_two_tasks() -> CommandContext<InMemoryRegistry> {
    crate::test_support::context_with_tasks(
        &["web", "api"],
        vec![
            crate::test_support::fix_login_task(),
            crate::test_support::task_in("api", "fix-auth", "Fix auth"),
        ],
    )
}

pub(super) fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "ajax-web-runtime-{tag}-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) const TEST_CF_ACCESS_ISSUER: &str = "https://test.cloudflareaccess.com";
pub(super) const TEST_CF_ACCESS_AUD: &str = "test-audience";
pub(super) const TEST_CF_ACCESS_SECRET: &[u8] = b"ajax-test-cloudflare-access-secret";

pub(super) fn cloudflare_access_config_for_test(
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
pub(super) struct TestCloudflareAccessClaims {
    aud: Vec<String>,
    iss: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    email: String,
    #[serde(rename = "type")]
    token_type: String,
}

pub(super) fn cloudflare_access_token_for_test(email: &str) -> String {
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

pub(super) fn browser_session_cookie<C, B>(state: &super::WebAppState<C, B>) -> String {
    state.browser_session.cookie_pair()
}

pub(super) fn authenticated_request(cookie: &str, uri: &str) -> axum::http::request::Builder {
    AxumRequest::builder().uri(uri).header("cookie", cookie)
}

/// State + session cookie + router for an `OkRunner`-backed test app.
pub(super) fn app_with(
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
pub(super) async fn get_public(app: &axum::Router, path: &str) -> axum::response::Response {
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
pub(super) fn set_cookie_pair(response: &axum::response::Response) -> String {
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

pub(super) async fn get(app: &axum::Router, cookie: &str, path: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            authenticated_request(cookie, path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

pub(super) async fn get_with_access(
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

pub(super) async fn websocket_get(
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

pub(super) async fn post_json(
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

pub(super) async fn json_of(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

pub(super) async fn assert_json_not_found(response: axum::response::Response, error: &str) {
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers()["content-type"],
        "application/json; charset=utf-8"
    );
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], error);
}

pub(super) fn state_with_bridge_and_task(bridge: TestBridge) -> WebAppState<OkRunner, TestBridge> {
    WebAppState::new(
        context_with_task(),
        OkRunner,
        bridge,
        scratch_dir("operator-input-sink"),
    )
}

mod suite_1;
mod suite_2;
mod suite_3;
mod suite_4;
mod suite_5;
