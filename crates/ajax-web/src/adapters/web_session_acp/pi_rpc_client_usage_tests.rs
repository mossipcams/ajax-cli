//! Tests for the context-usage update [`super::pi_rpc_client`] emits before
//! each finished prompt: the happy prompt flow reports `UsageUpdate` between
//! the last message chunk and `RequestFinished`, an aborted run reports it the
//! same way, and a silent-stats host or a rejected prompt skips it entirely.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate, TextContent};

use super::client::AcpClientEvent;
use super::pi_rpc_client::PiRpcClient;

/// Generous per-step bound: node startup plus one scripted record burst, and
/// the 2s stats timeout must fit inside a single wait when stats are silent.
const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

/// Spawn the fake pi RPC child directly (executable, node shebang), passing
/// `extra_args` as the fixture switches. Fails loudly when spawn fails.
fn spawn_client(extra_args: &[&str]) -> PiRpcClient {
    let args: Vec<String> = extra_args.iter().map(|arg| (*arg).to_string()).collect();
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    PiRpcClient::spawn(&fixture_path(), &args, cwd, None, TIMEOUT)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

fn text_blocks() -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new("hello"))]
}

/// The `(used, size)` pair when `event` is a `SessionUpdate` carrying a
/// `UsageUpdate`, otherwise `None`.
fn usage_update(event: &AcpClientEvent) -> Option<(u64, u64)> {
    match event {
        AcpClientEvent::SessionUpdate(notification) => match &notification.update {
            SessionUpdate::UsageUpdate(update) => Some((update.used, update.size)),
            _ => None,
        },
        _ => None,
    }
}

/// The stop reason when `event` is a successful `RequestFinished`, otherwise
/// `None`.
fn stop_reason(event: &AcpClientEvent) -> Option<&str> {
    match event {
        AcpClientEvent::RequestFinished {
            result: Ok(value), ..
        } => value.get("stopReason").and_then(|s| s.as_str()),
        _ => None,
    }
}

/// Collect events (with generous per-event waits) until a `RequestFinished`
/// arrives; it is kept as the last element.
fn collect_until_finished(client: &PiRpcClient) -> Vec<AcpClientEvent> {
    let mut events = Vec::new();
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(event, AcpClientEvent::RequestFinished { .. });
        events.push(event);
        if finished {
            return events;
        }
    }
}

#[test]
fn prompt_flow_reports_usage_between_message_and_finish() {
    let client = spawn_client(&[]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let events = collect_until_finished(&client);

    // Order: message chunk(s), then exactly one UsageUpdate, then the finish.
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AcpClientEvent::RequestFinished { .. }))
        .expect("RequestFinished must arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );

    let usage_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| usage_update(event).map(|_| i))
        .collect();
    assert_eq!(
        usage_indices,
        vec![finish_index - 1],
        "expected exactly one UsageUpdate right before the finish: {events:?}"
    );
    assert_eq!(
        usage_update(&events[finish_index - 1]),
        Some((10362, 1000000)),
        "usage numbers must come from get_session_stats: {events:?}"
    );

    let chunk_index = events
        .iter()
        .position(|event| {
            matches!(
                event,
                AcpClientEvent::SessionUpdate(notification)
                    if matches!(notification.update, SessionUpdate::AgentMessageChunk(_))
            )
        })
        .expect("an AgentMessageChunk must precede the usage update");
    assert!(
        chunk_index < usage_indices[0],
        "message chunk must come before the usage update: {events:?}"
    );

    assert_eq!(stop_reason(&events[finish_index]), Some("end_turn"));
    assert!(matches!(
        &events[finish_index],
        AcpClientEvent::RequestFinished { id: finish_id, method: "session/prompt", result: Ok(_) }
            if *finish_id == id
    ));
}

#[test]
fn silent_stats_still_finishes_without_usage() {
    let client = spawn_client(&["--silent-stats"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let events = collect_until_finished(&client);

    assert!(
        events.iter().all(|event| usage_update(event).is_none()),
        "no UsageUpdate may be reported when stats are silent: {events:?}"
    );
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AcpClientEvent::RequestFinished { .. }))
        .expect("RequestFinished must still arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );
    assert_eq!(stop_reason(&events[finish_index]), Some("end_turn"));
    assert!(matches!(
        &events[finish_index],
        AcpClientEvent::RequestFinished { id: finish_id, method: "session/prompt", result: Ok(_) }
            if *finish_id == id
    ));
}

#[test]
fn aborted_run_reports_usage_before_cancelled_finish() {
    let client = spawn_client(&["--hold-run"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let deadline = Instant::now() + TIMEOUT;
    while !client.prompt_in_flight() {
        assert!(
            Instant::now() < deadline,
            "held run never became active before cancel"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    client.cancel().expect("cancel");

    let events = collect_until_finished(&client);

    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AcpClientEvent::RequestFinished { .. }))
        .expect("RequestFinished must arrive after cancel");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );

    let usage_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| usage_update(event).map(|_| i))
        .collect();
    assert_eq!(
        usage_indices,
        vec![finish_index - 1],
        "an aborted run reports usage the same way, right before the finish: {events:?}"
    );
    assert_eq!(
        usage_update(&events[finish_index - 1]),
        Some((10362, 1000000)),
        "usage numbers must come from get_session_stats: {events:?}"
    );

    assert_eq!(stop_reason(&events[finish_index]), Some("cancelled"));
    assert!(matches!(
        &events[finish_index],
        AcpClientEvent::RequestFinished { id: finish_id, method: "session/prompt", result: Ok(_) }
            if *finish_id == id
    ));
}

#[test]
fn rejected_prompt_finishes_without_usage() {
    let client = spawn_client(&["--reject-prompt"]);
    let id = client.begin_prompt(&text_blocks()).expect("begin_prompt");

    let mut events = Vec::new();
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(event, AcpClientEvent::RequestFinished { .. });
        events.push(event);
        if finished {
            break;
        }
    }

    assert!(
        events.iter().all(|event| usage_update(event).is_none()),
        "a rejected prompt must not request stats: {events:?}"
    );
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AcpClientEvent::RequestFinished { .. }))
        .expect("RequestFinished must arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );
    assert!(matches!(
        &events[finish_index],
        AcpClientEvent::RequestFinished {
            id: finish_id,
            method: "session/prompt",
            result: Err(_),
        } if *finish_id == id
    ));
}
