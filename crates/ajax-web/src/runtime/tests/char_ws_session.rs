//! WebSocket session orchestration-chat characterization tests.

use super::axum_app;
use super::char_support::{
    char_app, close_ws, collect_until, install_fake_agent, message_texts, prepare_worktree,
    read_jsonl, wait_for_type, ws_cancel, ws_permission, ws_prompt, ws_set_model,
};
use std::time::Duration;
use tokio::time::sleep;

type CharWs =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn sleep_until_busy(ws: &mut CharWs) {
    let _ = collect_until(ws, Duration::from_secs(5), |events| {
        message_texts(events, "agent")
            .iter()
            .any(|text| text == "hung")
    })
    .await;
}

#[tokio::test]
async fn oc_acp_ready() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-acp-ready");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-acp-ready");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    assert_eq!(ready["model"], "auto");
}

#[tokio::test]
async fn oc_acp_mcp_servers() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-acp-mcp");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-acp-mcp");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "ping").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
}

#[tokio::test]
async fn oc_prompt_host_transcript() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-prompt-host");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-prompt-host");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "hello").await;
    let events = collect_until(&mut ws, Duration::from_secs(5), |events| {
        message_texts(events, "user").contains(&"hello".to_string())
            && events.iter().any(|event| event["type"] == "turn_end")
    })
    .await;
    assert!(
        events
            .iter()
            .filter(|event| event["type"] == "message" && event["role"] == "user")
            .count()
            <= 1,
        "host records one user message for the turn"
    );
    assert!(
        message_texts(&events, "agent")
            .iter()
            .any(|text| text == "hello"),
        "fake agent echoes prompt: {events:#?}"
    );
}

#[tokio::test]
async fn oc_queue_fifo() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-queue-fifo");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-queue-fifo");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__DELAY__").await;
    sleep(Duration::from_millis(50)).await;
    ws_prompt(&mut ws, "second").await;
    ws_prompt(&mut ws, "third").await;
    let events = collect_until(&mut ws, Duration::from_secs(8), |events| {
        message_texts(events, "agent").contains(&"second".to_string())
            && message_texts(events, "agent").contains(&"third".to_string())
    })
    .await;
    let agent_order = message_texts(&events, "agent")
        .into_iter()
        .filter(|text| text == "second" || text == "third")
        .collect::<Vec<_>>();
    assert!(
        agent_order.first() == Some(&"second".to_string())
            && agent_order.last() == Some(&"third".to_string()),
        "queued prompts drain FIFO through ACP: {agent_order:?}"
    );
}

#[tokio::test]
async fn oc_queue_cap_8() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-queue-cap");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-queue-cap");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__HANG__").await;
    sleep_until_busy(&mut ws).await;
    for index in 0..10 {
        ws_prompt(&mut ws, &format!("q{index}")).await;
    }
    ws_cancel(&mut ws, true).await;
    let events = collect_until(&mut ws, Duration::from_secs(10), |events| {
        message_texts(events, "agent")
            .into_iter()
            .filter(|text| text.starts_with('q'))
            .count()
            >= 8
    })
    .await;
    let queued: Vec<_> = message_texts(&events, "agent")
        .into_iter()
        .filter(|text| text.starts_with('q'))
        .collect();
    assert!(
        !queued.contains(&"q0".to_string()) && !queued.contains(&"q1".to_string()),
        "silent cap drops oldest queued prompts from ACP execution: {queued:?}"
    );
    assert_eq!(
        queued.len(),
        8,
        "at most eight queued prompts execute after cancel: {queued:?}"
    );
    // conflict: docs do not mention the silent eight-prompt cap.
}

#[tokio::test]
async fn oc_cancel_stop_drops_queue() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-cancel-drop");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-cancel-drop");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__HANG__").await;
    sleep_until_busy(&mut ws).await;
    ws_prompt(&mut ws, "queued-follow-up").await;
    ws_cancel(&mut ws, false).await;
    let events = collect_until(&mut ws, Duration::from_secs(5), |events| {
        events.iter().any(|event| event["type"] == "turn_end")
    })
    .await;
    assert!(
        !message_texts(&events, "agent").contains(&"queued-follow-up".to_string()),
        "cancel without keepQueue clears the queue before ACP executes follow-up"
    );
}

#[tokio::test]
async fn oc_cancel_enter_again() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-cancel-enter");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-cancel-enter");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__HANG__").await;
    sleep_until_busy(&mut ws).await;
    ws_prompt(&mut ws, "follow-up").await;
    ws_cancel(&mut ws, true).await;
    let events = collect_until(&mut ws, Duration::from_secs(8), |events| {
        message_texts(events, "agent")
            .iter()
            .any(|text| text == "follow-up")
    })
    .await;
    assert!(
        message_texts(&events, "user").contains(&"follow-up".to_string()),
        "keepQueue true sends the queued follow-up after cancel: {events:#?}"
    );
}

#[tokio::test]
async fn oc_perm_request() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-perm-req");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-perm-req");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__PERM__").await;
    let perm = wait_for_type(&mut ws, "permission_request", Duration::from_secs(5)).await;
    assert_eq!(perm["requestId"], "42");
}

#[tokio::test]
async fn oc_perm_resolved_persisted() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-perm-resolved");
    let (state, cookie, app, _worktree) = char_app(worktree, "oc-perm-resolved");
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__PERM__").await;
    let perm = wait_for_type(&mut ws, "permission_request", Duration::from_secs(5)).await;
    let request_id = perm["requestId"].as_str().expect("request id");
    ws_permission(&mut ws, request_id, true).await;
    let _ = wait_for_type(&mut ws, "permission_resolved", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);
    sleep(Duration::from_millis(100)).await;

    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(state.clone())).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let events = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        events
            .iter()
            .any(|event| event["type"] == "permission_resolved")
    })
    .await;
    let resolved = events
        .iter()
        .filter(|event| event["type"] == "permission_resolved")
        .count();
    assert!(resolved >= 1, "resolved permission replays: {events:#?}");
    // Observed: append-only JSONL also replays permission_request. Docs say
    // reconnect must not resurrect a decided prompt — that is UI reducer duty.
}

#[tokio::test]
async fn oc_model_pin_at_spawn() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-model-pin");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-model-pin");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws = super::char_support::connect_session_ws(
        addr,
        &cookie,
        "web/fix-login",
        Some("composer-2.5"),
    )
    .await;
    let ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    assert_eq!(ready["model"], "composer-2.5");
}

#[tokio::test]
async fn oc_model_switch() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-model-switch");
    let (state, cookie, app, _worktree) = char_app(worktree, "oc-model-switch");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws_a =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let mut ws_b =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ = wait_for_type(&mut ws_a, "ready", Duration::from_secs(5)).await;
    let _ = wait_for_type(&mut ws_b, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws_a, "seed").await;
    let _ = wait_for_type(&mut ws_a, "turn_end", Duration::from_secs(5)).await;
    ws_set_model(&mut ws_a, "composer-2.5").await;
    let ready_a = wait_for_type(&mut ws_a, "ready", Duration::from_secs(5)).await;
    assert_eq!(ready_a["model"], "composer-2.5");
    let events_b = collect_until(&mut ws_b, Duration::from_secs(10), |events| {
        events
            .iter()
            .any(|event| event["type"] == "ready" && event["model"] == "composer-2.5")
    })
    .await;
    assert!(
        events_b.iter().any(|event| event["type"] == "message"),
        "peer socket replays transcript after model switch"
    );
    let jsonl = read_jsonl(&state.state_dir, "web/fix-login");
    assert!(
        jsonl.iter().any(|line| line.get("event").is_some()),
        "transcript persisted across model switch"
    );
}

#[tokio::test]
async fn oc_model_switch_during_turn() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-model-switch-busy");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-model-switch-busy");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__HANG__").await;
    sleep_until_busy(&mut ws).await;
    ws_set_model(&mut ws, "composer-2.5").await;
    let ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    assert_eq!(ready["model"], "composer-2.5");
    // conflict: docs say model changes when idle; server respawns while busy.
}
