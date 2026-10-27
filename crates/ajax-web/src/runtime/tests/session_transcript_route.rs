use super::{app_with, context_with_task, json_of, TestBridge};
use crate::adapters::web_session_store;
use crate::slices::web_session::SessionServerEvent;
use axum::{body::Body, http::Request as AxumRequest};
use tower::ServiceExt;

const HANDLE: &str = "web/fix-login";

fn message(role: &str, text: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: role.to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: format!("{role}-{text}"),
        message_id: None,
    }
}

async fn transcript_get(app: &axum::Router, cookie: &str, path: &str) -> axum::response::Response {
    send_transcript_request(app, cookie, path, Some("localhost"), None).await
}

async fn send_transcript_request(
    app: &axum::Router,
    cookie: &str,
    path: &str,
    host: Option<&str>,
    origin: Option<&str>,
) -> axum::response::Response {
    let mut builder = AxumRequest::builder().uri(path).header("cookie", cookie);
    if let Some(host) = host {
        builder = builder.header("host", host);
    }
    if let Some(origin) = origin {
        builder = builder.header("origin", origin);
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn seeded_state(tag: &str, events: &[SessionServerEvent]) -> (String, axum::Router) {
    let (state, cookie, app) = app_with(context_with_task(), TestBridge::default(), tag);
    web_session_store::append_events(&state.state_dir, HANDLE, events);
    (cookie, app)
}

fn cursors(body: &serde_json::Value) -> Vec<usize> {
    body["events"]
        .as_array()
        .expect("events array")
        .iter()
        .map(|event| event["cursor"].as_u64().expect("cursor") as usize)
        .collect()
}

#[tokio::test]
async fn session_transcript_returns_200_with_ascending_events_and_from_floor() {
    let events = vec![
        message("user", "one"),
        message("assistant", "two"),
        message("user", "three"),
        message("assistant", "four"),
        message("assistant", "five"),
    ];
    let (cookie, app) = seeded_state("session-transcript-basic", &events);

    let response = send_transcript_request(
        &app,
        &cookie,
        "/api/tasks/web/fix-login/session/transcript",
        Some("localhost"),
        Some("http://localhost"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = json_of(response).await;
    assert_eq!(body["protocolVersion"], 2);
    assert_eq!(cursors(&body), vec![0, 1, 2, 3, 4]);
    assert_eq!(body["from"], 0);
    assert_eq!(body["floor"], 0);
}

#[tokio::test]
async fn session_transcript_absent_origin_is_allowed_like_same_origin_fetch() {
    let events = vec![message("user", "one"), message("assistant", "two")];
    let (cookie, app) = seeded_state("session-transcript-no-origin", &events);

    let response =
        transcript_get(&app, &cookie, "/api/tasks/web/fix-login/session/transcript").await;

    assert_eq!(response.status(), 200);
    let body = json_of(response).await;
    assert_eq!(cursors(&body), vec![0, 1]);
}

#[tokio::test]
async fn session_transcript_origin_authority_matches_host_case_insensitively() {
    let events = vec![message("user", "one")];
    let (cookie, app) = seeded_state("session-transcript-origin-case", &events);

    let response = send_transcript_request(
        &app,
        &cookie,
        "/api/tasks/web/fix-login/session/transcript",
        Some("localhost"),
        Some("http://LOCALHOST"),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = json_of(response).await;
    assert_eq!(cursors(&body), vec![0]);
}

#[tokio::test]
async fn session_transcript_mismatched_origin_is_forbidden() {
    let events = vec![message("user", "one")];
    let (cookie, app) = seeded_state("session-transcript-mismatch", &events);

    let response = send_transcript_request(
        &app,
        &cookie,
        "/api/tasks/web/fix-login/session/transcript",
        Some("localhost"),
        Some("http://evil.example"),
    )
    .await;

    assert_eq!(response.status(), 403);
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "origin forbidden");
}

#[tokio::test]
async fn session_transcript_malformed_origin_or_missing_host_is_forbidden() {
    let events = vec![message("user", "one")];
    let (cookie, app) = seeded_state("session-transcript-malformed-origin", &events);

    for (host, origin) in [
        (Some("localhost"), Some("null")),
        (Some("localhost"), Some("ftp://localhost")),
        (Some("localhost"), Some("localhost")),
        (Some("localhost"), Some("http://localhost@evil.example")),
        (None, Some("http://localhost")),
    ] {
        let response = send_transcript_request(
            &app,
            &cookie,
            "/api/tasks/web/fix-login/session/transcript",
            host,
            origin,
        )
        .await;
        assert_eq!(
            response.status(),
            403,
            "expected 403 for host {host:?} origin {origin:?}"
        );
        let body = json_of(response).await;
        assert_eq!(body["ok"], false);
        assert_eq!(body["error"], "origin forbidden");
    }
}

#[tokio::test]
async fn session_transcript_honors_before_cursor() {
    let events = vec![
        message("user", "one"),
        message("assistant", "two"),
        message("user", "three"),
        message("assistant", "four"),
    ];
    let (cookie, app) = seeded_state("session-transcript-before", &events);

    let response = transcript_get(
        &app,
        &cookie,
        "/api/tasks/web/fix-login/session/transcript?before=3",
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = json_of(response).await;
    assert_eq!(cursors(&body), vec![0, 1, 2]);
    assert_eq!(body["from"], 0);
    assert_eq!(body["floor"], 0);
}

#[tokio::test]
async fn session_transcript_clamps_limit_to_at_most_1000() {
    let events = (0..7)
        .map(|i| {
            message(
                if i % 2 == 0 { "user" } else { "assistant" },
                &i.to_string(),
            )
        })
        .collect::<Vec<_>>();
    let (cookie, app) = seeded_state("session-transcript-limit", &events);

    let response = transcript_get(
        &app,
        &cookie,
        "/api/tasks/web/fix-login/session/transcript?limit=2000",
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = json_of(response).await;
    assert_eq!(cursors(&body), vec![0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(body["from"], 0);
}

#[tokio::test]
async fn session_transcript_rejects_non_numeric_query_values() {
    let events = vec![message("user", "one")];
    let (cookie, app) = seeded_state("session-transcript-bad-query", &events);

    for path in [
        "/api/tasks/web/fix-login/session/transcript?limit=abc",
        "/api/tasks/web/fix-login/session/transcript?before=nope",
    ] {
        let response = transcript_get(&app, &cookie, path).await;
        assert_eq!(response.status(), 400, "expected 400 for {path}");
        let body = json_of(response).await;
        assert_eq!(body["ok"], false);
        assert!(body["error"].is_string());
    }
}

#[tokio::test]
async fn session_transcript_unknown_task_is_404() {
    let events = vec![message("user", "one")];
    let (cookie, app) = seeded_state("session-transcript-unknown", &events);

    let response = transcript_get(
        &app,
        &cookie,
        "/api/tasks/web/does-not-exist/session/transcript",
    )
    .await;

    assert_eq!(response.status(), 404);
    let body = json_of(response).await;
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"], "task not found");
}

#[tokio::test]
async fn session_transcript_walk_back_with_from_reaches_floor() {
    let events = vec![
        message("user", "one"),
        message("assistant", "two"),
        message("user", "three"),
        message("assistant", "four"),
        message("user", "five"),
    ];
    let (cookie, app) = seeded_state("session-transcript-walk", &events);

    let first = json_of(
        transcript_get(
            &app,
            &cookie,
            "/api/tasks/web/fix-login/session/transcript?limit=2",
        )
        .await,
    )
    .await;
    assert_eq!(first["floor"], 0);
    let from = first["from"].as_u64().expect("from");

    let second = json_of(
        transcript_get(
            &app,
            &cookie,
            &format!("/api/tasks/web/fix-login/session/transcript?before={from}"),
        )
        .await,
    )
    .await;
    assert_eq!(cursors(&second), vec![0, 1]);
    assert_eq!(
        second["from"], second["floor"],
        "walk should stop at the floor"
    );

    let third = json_of(
        transcript_get(
            &app,
            &cookie,
            "/api/tasks/web/fix-login/session/transcript?before=0",
        )
        .await,
    )
    .await;
    assert_eq!(
        third["from"], third["floor"],
        "stopped walk stays at the floor"
    );
}
