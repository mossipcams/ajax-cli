//! Shared helpers for orchestration-chat black-box characterization tests.

use super::{browser_session_cookie, scratch_dir, OkRunner, TestBridge};
use ajax_core::{
    commands::CommandContext,
    models::{AgentClient, Task},
    registry::InMemoryRegistry,
};
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::{
    net::TcpListener,
    sync::oneshot,
    time::{sleep, timeout},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, http::HeaderValue, Message},
    MaybeTlsStream, WebSocketStream,
};

const FAKE_ACP_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/fake-cursor-acp.mjs");
const FAKE_ACP_ARGS_FILE: &str = ".ajax-fake-acp-args";

static FAKE_AGENT_BIN: OnceLock<PathBuf> = OnceLock::new();

/// Marker that PATH contains the fake `agent`. Extra ACP flags belong on the
/// worktree via [`write_fake_agent_args`] so concurrent tests do not deadlock
/// on a process-wide std Mutex held across `.await`.
pub(super) struct FakeAgentGuard;

/// Prepend a stable `bin/agent` wrapper to PATH once per process.
pub(super) fn install_fake_agent(extra_script_args: &[&str]) -> FakeAgentGuard {
    if !extra_script_args.is_empty() {
        panic!(
            "pass extra fake-ACP flags with write_fake_agent_args(worktree, …); \
             install_fake_agent must not hold a process lock across await"
        );
    }
    FAKE_AGENT_BIN.get_or_init(|| {
        let bin_dir = scratch_dir("char-fake-agent-bin");
        fs::create_dir_all(&bin_dir).expect("fake agent bin dir");
        let agent_path = bin_dir.join("agent");
        let wrapper = format!("#!/bin/sh\nexec node \"{FAKE_ACP_SCRIPT}\" \"$@\"\n");
        fs::write(&agent_path, wrapper).expect("fake agent wrapper");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&agent_path, fs::Permissions::from_mode(0o755))
                .expect("chmod agent");
        }
        let prev_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{prev_path}", bin_dir.display()));
        bin_dir
    });
    FakeAgentGuard
}

/// Flags read by `fake-cursor-acp.mjs` from the ACP spawn cwd (task worktree).
pub(super) fn write_fake_agent_args(worktree: &Path, extra_script_args: &[&str]) {
    fs::write(
        worktree.join(FAKE_ACP_ARGS_FILE),
        extra_script_args.join(" "),
    )
    .expect("write fake acp args");
}

pub(super) fn cursor_task_with_worktree(worktree: PathBuf) -> Task {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Cursor;
    task.worktree_path = worktree;
    task
}

pub(super) fn cursor_context(worktree: PathBuf) -> CommandContext<InMemoryRegistry> {
    crate::test_support::context_with_tasks(&["web"], vec![cursor_task_with_worktree(worktree)])
}

pub(super) fn prepare_worktree(tag: &str) -> PathBuf {
    let dir = scratch_dir(tag);
    fs::create_dir_all(&dir).expect("worktree dir");
    dir
}

pub(super) type CharApp = (
    super::WebAppState<OkRunner, TestBridge>,
    String,
    Router,
    PathBuf,
);

pub(super) fn char_app(worktree: PathBuf, tag: &str) -> CharApp {
    let state_dir = scratch_dir(&format!("char-state-{tag}"));
    let context = cursor_context(worktree.clone());
    let state = super::WebAppState::new(context, OkRunner, TestBridge::default(), state_dir);
    let cookie = browser_session_cookie(&state);
    let app = super::axum_app(state.clone());
    (state, cookie, app, worktree)
}

pub(super) async fn serve_plain_http(app: Router) -> (SocketAddr, oneshot::Sender<()>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind plain http");
    let addr = listener.local_addr().expect("local addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve");
    });
    sleep(Duration::from_millis(30)).await;
    (addr, shutdown_tx)
}

pub(super) fn session_ws_path(handle: &str, model: Option<&str>) -> String {
    let encoded = handle.replace('%', "%25").replace('/', "%2F");
    match model {
        Some(model) => format!("/api/tasks/{encoded}/session?model={model}"),
        None => format!("/api/tasks/{encoded}/session"),
    }
}

pub(super) async fn connect_session_ws(
    addr: SocketAddr,
    cookie: &str,
    handle: &str,
    model: Option<&str>,
) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
    let path = session_ws_path(handle, model);
    let url = format!("ws://127.0.0.1:{port}{path}", port = addr.port());
    let origin = format!("http://127.0.0.1:{port}", port = addr.port());
    let mut request = url.into_client_request().expect("ws request");
    request.headers_mut().insert(
        "cookie",
        HeaderValue::from_str(cookie).expect("cookie header"),
    );
    request.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&origin).expect("origin header"),
    );
    let (ws, _) = connect_async(request).await.expect("ws connect");
    ws
}

pub(super) async fn ws_send_json(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    value: &Value,
) {
    ws.send(Message::Text(value.to_string().into()))
        .await
        .expect("ws send");
}

pub(super) async fn ws_prompt(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    text: &str,
) {
    ws_send_json(ws, &json!({ "type": "prompt", "text": text })).await;
}

pub(super) async fn ws_cancel(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    keep_queue: bool,
) {
    ws_send_json(ws, &json!({ "type": "cancel", "keepQueue": keep_queue })).await;
}

pub(super) async fn ws_set_model(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    model: &str,
) {
    ws_send_json(ws, &json!({ "type": "set_model", "model": model })).await;
}

pub(super) async fn ws_permission(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    request_id: &str,
    approved: bool,
) {
    ws_send_json(
        ws,
        &json!({ "type": "permission", "requestId": request_id, "approved": approved }),
    )
    .await;
}

pub(super) async fn recv_json(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    wait: Duration,
) -> Option<Value> {
    let deadline = Instant::now() + wait;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match timeout(remaining, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                return Some(serde_json::from_str(text.as_ref()).unwrap());
            }
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return None,
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(error))) => panic!("ws recv error: {error}"),
            Err(_) => return None,
        }
    }
    None
}

pub(super) async fn collect_until<F>(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    max_wait: Duration,
    mut predicate: F,
) -> Vec<Value>
where
    F: FnMut(&[Value]) -> bool,
{
    let deadline = Instant::now() + max_wait;
    let mut events = Vec::new();
    while Instant::now() < deadline {
        if predicate(&events) {
            return events;
        }
        if let Some(event) = recv_json(ws, Duration::from_millis(100)).await {
            events.push(event);
        }
    }
    panic!("timed out collecting ws events; got {events:#?}");
}

pub(super) async fn wait_for_type(
    ws: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    event_type: &str,
    max_wait: Duration,
) -> Value {
    let events = collect_until(ws, max_wait, |events| {
        events.iter().any(|event| event["type"] == event_type)
    })
    .await;
    events
        .into_iter()
        .find(|event| event["type"] == event_type)
        .expect("event type present")
}

#[allow(dead_code)]
pub(super) fn normalize_value(mut value: Value) -> Value {
    normalize_in_place(&mut value);
    value
}

fn normalize_in_place(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if matches!(
                    key.as_str(),
                    "requestId" | "callId" | "sessionId" | "generation"
                ) {
                    *child = Value::String("<id>".to_string());
                } else if key == "model" && child.is_string() {
                    // keep model ids — tests assert on them
                } else if key == "worktree_path" || key == "path" {
                    *child = Value::String("<path>".to_string());
                } else {
                    normalize_in_place(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_in_place(item);
            }
        }
        Value::String(text) => {
            if text.contains("/repo/") || text.contains("ajax-web-runtime-") {
                *text = "<path>".to_string();
            } else if text.starts_with("127.0.0.1:") {
                *text = "<addr>".to_string();
            }
        }
        _ => {}
    }
}

pub(super) fn jsonl_path(state_dir: &Path, handle: &str) -> PathBuf {
    let encoded = handle.replace('%', "%25").replace('/', "%2F");
    state_dir
        .join("web-session")
        .join(format!("{encoded}.jsonl"))
}

pub(super) fn read_jsonl(state_dir: &Path, handle: &str) -> Vec<Value> {
    let path = jsonl_path(state_dir, handle);
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub(super) fn events_of_type<'a>(events: &'a [Value], event_type: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event.get("type").and_then(Value::as_str) == Some(event_type))
        .collect()
}

pub(super) fn message_texts(events: &[Value], role: &str) -> Vec<String> {
    events_of_type(events, "message")
        .into_iter()
        .filter(|event| event["role"] == role)
        .filter_map(|event| event["text"].as_str().map(str::to_string))
        .collect()
}

pub(super) async fn close_ws(mut ws: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>) {
    let _ = ws.close(None).await;
}

#[allow(dead_code)]
pub(super) fn percent_encode_handle(handle: &str) -> String {
    handle.replace('/', "%2F")
}
