//! Transcript persistence and reconnect characterization tests.

use super::axum_app;
use super::char_support::{
    char_app, close_ws, collect_until, events_of_type, install_fake_agent, jsonl_path,
    message_texts, prepare_worktree, read_jsonl, wait_for_type, write_fake_agent_args, ws_prompt,
};
use super::{browser_session_cookie, OkRunner, TestBridge, WebAppState};
use std::time::Duration;
use tokio::time::sleep;

const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

#[tokio::test]
async fn oc_transcript_jsonl() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-transcript-jsonl");
    let (state, cookie, app, _worktree) = char_app(worktree, "oc-transcript-jsonl");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "disk").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    let path = jsonl_path(&state.state_dir, "web/fix-login");
    assert!(path.is_file(), "jsonl file must exist at {path:?}");
    assert!(
        path.to_string_lossy().contains("web%2Ffix-login.jsonl"),
        "handle slash is percent-encoded in filename"
    );
    let lines = read_jsonl(&state.state_dir, "web/fix-login");
    assert!(
        lines
            .iter()
            .any(|line| line.get("kind") == Some(&serde_json::json!("meta"))),
        "meta line persisted"
    );
    assert!(
        lines.iter().any(|line| {
            line.get("event")
                .and_then(|event| event.get("type"))
                .and_then(|kind| kind.as_str())
                == Some("message")
        }),
        "event lines persisted"
    );
}

#[tokio::test]
async fn oc_transcript_grain() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-transcript-grain");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-transcript-grain");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__TOOL__").await;
    let tool_events = collect_until(&mut ws, Duration::from_secs(5), |events| {
        events.iter().any(|event| event["type"] == "tool_call")
    })
    .await;
    assert!(
        tool_events
            .iter()
            .any(|event| event["type"] == "tool_call" && event["title"] == "Fake tool"),
        "tool_call maps from ACP: {tool_events:#?}"
    );
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__UNKNOWN__").await;
    let unknown_events = collect_until(&mut ws, Duration::from_secs(5), |events| {
        events
            .iter()
            .any(|event| event["type"] == "artifact" && event["kind"] == "totally_unknown_kind")
            || events.iter().any(|event| event["type"] == "turn_end")
    })
    .await;
    let saw_artifact = unknown_events
        .iter()
        .any(|event| event["type"] == "artifact" && event["kind"] == "totally_unknown_kind");
    assert!(
        saw_artifact,
        // conflict: docs say unknown kinds are dropped; implementation emits artifact.
        "unknown sessionUpdate becomes artifact on the wire: {unknown_events:#?}"
    );
}

#[tokio::test]
async fn oc_reconnect_replay() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-reconnect-replay");
    let (state, cookie, app, _worktree) = char_app(worktree, "oc-reconnect-replay");
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "replay-me").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);
    sleep(Duration::from_millis(100)).await;

    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(state.clone())).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let replay = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        message_texts(events, "user").contains(&"replay-me".to_string())
            && message_texts(events, "agent").contains(&"replay-me".to_string())
    })
    .await;
    assert!(
        replay.iter().any(|event| event["type"] == "ready"),
        "reconnect sends ready before replay"
    );
}

#[tokio::test]
async fn oc_reconnect_idle_slot() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-reconnect-idle");
    let (state, cookie, app, _worktree) = char_app(worktree, "oc-reconnect-idle");
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "idle-slot").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);
    sleep(Duration::from_millis(300)).await;

    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(state.clone())).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let events = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        message_texts(events, "agent").contains(&"idle-slot".to_string())
    })
    .await;
    assert!(
        !message_texts(&events, "agent").contains(&CONTEXT_RESET_NOTE.to_string()),
        "idle slot keeps ACP child without context-reset note"
    );
}

#[tokio::test]
async fn oc_restart_jsonl_reload() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-restart-jsonl");
    let (state, cookie, app, worktree) = char_app(worktree, "oc-restart-jsonl");
    let state_dir = state.state_dir.to_path_buf();
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "persisted").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);

    let fresh = WebAppState::new(
        super::char_support::cursor_context(worktree),
        OkRunner,
        TestBridge::default(),
        state_dir,
    );
    let cookie = browser_session_cookie(&fresh);
    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(fresh)).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let replay = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        message_texts(events, "user").contains(&"persisted".to_string())
    })
    .await;
    assert!(
        replay.iter().any(|event| event["type"] == "ready"),
        "new hub reloads jsonl before replay: {replay:#?}"
    );
}

#[tokio::test]
async fn oc_restart_loadsession() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-restart-load");
    let (state, cookie, app, worktree) = char_app(worktree, "oc-restart-load");
    let state_dir = state.state_dir.to_path_buf();
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "before-restart").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);

    let context = super::char_support::cursor_context(worktree);
    let fresh = super::WebAppState::new(context, OkRunner, super::TestBridge::default(), state_dir);
    let cookie = super::browser_session_cookie(&fresh);
    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(fresh)).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let events = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        events.iter().any(|event| event["type"] == "turn_end")
            || message_texts(events, "user").len() >= 2
    })
    .await;
    assert!(
        !message_texts(&events, "agent").contains(&"replayed".to_string()),
        "session/load replay must not duplicate into transcript"
    );
    assert!(
        !message_texts(&events, "agent").contains(&CONTEXT_RESET_NOTE.to_string()),
        "successful load avoids context-reset note"
    );
}

#[tokio::test]
async fn oc_restart_load_fail_or_unsupported() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-restart-load-fail");
    let (state, cookie, app, worktree) = char_app(worktree, "oc-restart-load-fail");
    let state_dir = state.state_dir.to_path_buf();
    let (addr, shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "survive").await;
    let _ = wait_for_type(&mut ws, "turn_end", Duration::from_secs(5)).await;
    close_ws(ws).await;
    drop(shutdown);

    write_fake_agent_args(&worktree, &["--load-fail"]);
    let _agent2 = install_fake_agent(&[]);
    let context = super::char_support::cursor_context(worktree);
    let fresh = super::WebAppState::new(context, OkRunner, super::TestBridge::default(), state_dir);
    let cookie = super::browser_session_cookie(&fresh);
    let (addr2, _shutdown2) = super::char_support::serve_plain_http(axum_app(fresh)).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr2, &cookie, "web/fix-login", None).await;
    let events = collect_until(&mut ws2, Duration::from_secs(5), |events| {
        message_texts(events, "agent").contains(&CONTEXT_RESET_NOTE.to_string())
            && message_texts(events, "user").contains(&"survive".to_string())
    })
    .await;
    assert!(
        message_texts(&events, "agent").contains(&CONTEXT_RESET_NOTE.to_string()),
        "load failure appends context-reset note while keeping transcript"
    );
    ws_prompt(&mut ws2, "after-reset").await;
    let _ = wait_for_type(&mut ws2, "turn_end", Duration::from_secs(5)).await;
}

#[tokio::test]
async fn oc_proc_death_respawn() {
    let _agent = install_fake_agent(&[]);
    let worktree = prepare_worktree("oc-proc-death");
    let (_state, cookie, app, _worktree) = char_app(worktree, "oc-proc-death");
    let (addr, _shutdown) = super::char_support::serve_plain_http(app).await;
    let mut ws =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws, "__DIE__").await;
    let _ = collect_until(&mut ws, Duration::from_secs(5), |events| {
        events_of_type(events, "error")
            .iter()
            .any(|event| event["message"].as_str().unwrap_or("").contains("exited"))
    })
    .await;
    close_ws(ws).await;
    sleep(Duration::from_millis(200)).await;
    let mut ws2 =
        super::char_support::connect_session_ws(addr, &cookie, "web/fix-login", None).await;
    let _ready = wait_for_type(&mut ws2, "ready", Duration::from_secs(5)).await;
    ws_prompt(&mut ws2, "after-death").await;
    let events = collect_until(&mut ws2, Duration::from_secs(10), |events| {
        message_texts(events, "agent").contains(&"after-death".to_string())
    })
    .await;
    assert!(
        message_texts(&events, "agent").contains(&"after-death".to_string()),
        "reconnect after child death can prompt successfully: {events:#?}"
    );
}
