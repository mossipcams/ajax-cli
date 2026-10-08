//! Unit tests for [`super::pi_rpc_map`], replaying trimmed real Pi RPC-mode
//! transcripts from `tests/fixtures/pi_rpc/`.

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate, ToolCallStatus};
use serde_json::Value;

use super::client::AcpClientEvent;
use super::pi_rpc_map::map_record;

const SESSION_ID: &str = "acp-session-1";

/// The result of folding [`map_record`] over one whole transcript.
struct Replay {
    events: Vec<AcpClientEvent>,
    finished: bool,
    ended_aborted: bool,
}

/// Load and parse one trimmed transcript fixture.
fn fixture(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pi_rpc")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read fixture {path:?}: {error}");
    });
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line must be valid JSON"))
        .collect()
}

fn replay(records: &[Value]) -> Replay {
    let mut events = Vec::new();
    let mut finished = false;
    let mut ended_aborted = false;
    for record in records {
        let mapping = map_record(record, SESSION_ID);
        events.extend(mapping.events);
        finished |= mapping.finished;
        ended_aborted |= mapping.ended_aborted;
    }
    Replay {
        events,
        finished,
        ended_aborted,
    }
}

/// The session update of an event that must be a `SessionUpdate`, failing on
/// anything else (an `AcpClientEvent::Error` would fail the test here).
fn session_update(event: &AcpClientEvent) -> &SessionUpdate {
    match event {
        AcpClientEvent::SessionUpdate(notification) => &notification.update,
        other => panic!("expected a SessionUpdate event, got {other:?}"),
    }
}

/// The streamed text of a message/thought chunk update.
fn chunk_text(update: &SessionUpdate) -> &str {
    let chunk = match update {
        SessionUpdate::AgentMessageChunk(chunk) | SessionUpdate::AgentThoughtChunk(chunk) => chunk,
        other => panic!("expected a chunk update, got {other:?}"),
    };
    match &chunk.content {
        ContentBlock::Text(text) => &text.text,
        other => panic!("expected a text content block, got {other:?}"),
    }
}

/// Every event's notification must carry the ACP session id.
fn assert_events_carry_session_id(events: &[AcpClientEvent]) {
    for event in events {
        match event {
            AcpClientEvent::SessionUpdate(notification) => {
                assert_eq!(notification.session_id.to_string(), SESSION_ID);
            }
            other => panic!("expected only SessionUpdate events, got {other:?}"),
        }
    }
}

#[test]
fn prompt_transcript_yields_one_message_chunk_and_settles() {
    let replay = replay(&fixture("prompt.jsonl"));

    // The trimmed prompt transcript streams exactly one text delta ("ok").
    // `setStatus` / `setWidget` / `response` records yield no events.
    assert_events_carry_session_id(&replay.events);
    let texts: Vec<&str> = replay
        .events
        .iter()
        .map(|event| chunk_text(session_update(event)))
        .collect();
    assert_eq!(texts, ["ok"]);
    assert!(matches!(
        session_update(&replay.events[0]),
        SessionUpdate::AgentMessageChunk(_)
    ));
    assert!(replay.finished, "agent_settled must finish the run");
    assert!(!replay.ended_aborted);
}

#[test]
fn toolcall_transcript_yields_tool_call_and_updates() {
    let replay = replay(&fixture("toolcall.jsonl"));

    assert_events_carry_session_id(&replay.events);
    let updates: Vec<&SessionUpdate> = replay.events.iter().map(session_update).collect();

    // Four thought deltas, the tool call, one partial update, the completed
    // update, then the final "done" message chunk.
    assert_eq!(updates.len(), 8);

    let thought_text: String = updates[..4]
        .iter()
        .map(|update| chunk_text(update).to_string())
        .collect();
    assert_eq!(thought_text, "The user wants me");
    for update in &updates[..4] {
        assert!(matches!(update, SessionUpdate::AgentThoughtChunk(_)));
    }

    let SessionUpdate::ToolCall(tool_call) = updates[4] else {
        panic!("expected a tool call, got {:?}", updates[4]);
    };
    assert_eq!(
        tool_call.tool_call_id.to_string(),
        "chatcmpl-tool-be93830d9b11882f"
    );
    assert_eq!(tool_call.title, "echo hi");
    assert_eq!(tool_call.status, ToolCallStatus::InProgress);
    assert_eq!(
        tool_call
            .raw_input
            .as_ref()
            .map(|value| value.get("command").and_then(Value::as_str)),
        Some(Some("echo hi"))
    );

    // Partial result update: still in progress, carrying the partial text.
    let SessionUpdate::ToolCallUpdate(partial) = updates[5] else {
        panic!("expected a tool call update, got {:?}", updates[5]);
    };
    assert_eq!(
        partial.tool_call_id.to_string(),
        "chatcmpl-tool-be93830d9b11882f"
    );
    assert_eq!(partial.fields.status, Some(ToolCallStatus::InProgress));
    let content = partial.fields.content.as_deref().unwrap_or_default();
    assert_eq!(content.len(), 1);
    assert_eq!(tool_content_text(&content[0]), "hi\n");

    // Final update: completed with the tool result text.
    let SessionUpdate::ToolCallUpdate(finished) = updates[6] else {
        panic!("expected a tool call update, got {:?}", updates[6]);
    };
    assert_eq!(finished.fields.status, Some(ToolCallStatus::Completed));
    let content = finished.fields.content.as_deref().unwrap_or_default();
    assert_eq!(tool_content_text(&content[0]), "hi\n");

    assert!(matches!(
        session_update(&replay.events[7]),
        SessionUpdate::AgentMessageChunk(_)
    ));
    assert_eq!(chunk_text(updates[7]), "done");
    assert!(replay.finished);
    assert!(!replay.ended_aborted);
}

#[test]
fn failed_tool_execution_end_maps_to_failed_status() {
    // Same shape as the fixture's tool_execution_end record with isError true.
    let record = serde_json::json!({
        "type": "tool_execution_end",
        "toolCallId": "chatcmpl-tool-be93830d9b11882f",
        "toolName": "bash",
        "result": { "content": [{ "type": "text", "text": "boom\n" }] },
        "isError": true,
    });
    let replay = replay(&[record]);

    assert_eq!(replay.events.len(), 1);
    let SessionUpdate::ToolCallUpdate(failed) = session_update(&replay.events[0]) else {
        panic!("expected a tool call update");
    };
    assert_eq!(failed.fields.status, Some(ToolCallStatus::Failed));
    let content = failed.fields.content.as_deref().unwrap_or_default();
    assert_eq!(tool_content_text(&content[0]), "boom\n");
    assert!(!replay.finished);
}

#[test]
fn abort_transcript_reports_finished_and_aborted() {
    let replay = replay(&fixture("abort.jsonl"));

    assert_events_carry_session_id(&replay.events);
    let updates: Vec<&SessionUpdate> = replay.events.iter().map(session_update).collect();
    // Three thought deltas then three text deltas.
    assert_eq!(updates.len(), 6);
    let thought_text: String = updates[..3]
        .iter()
        .map(|update| chunk_text(update).to_string())
        .collect();
    assert_eq!(thought_text, "A writing task");
    let message_text: String = updates[3..]
        .iter()
        .map(|update| chunk_text(update).to_string())
        .collect();
    assert_eq!(message_text, "# Ocean T");
    assert!(replay.finished, "agent_settled must finish the aborted run");
    assert!(replay.ended_aborted, "stopReason aborted must be reported");
}

#[test]
fn aborted_message_end_alone_reports_abort_without_finishing() {
    let aborted_message_end = fixture("abort.jsonl")
        .into_iter()
        .find(|record| {
            record.get("type").and_then(Value::as_str) == Some("message_end")
                && record
                    .get("message")
                    .and_then(|message| message.get("stopReason"))
                    .and_then(Value::as_str)
                    == Some("aborted")
        })
        .expect("abort fixture must contain the aborted message_end");
    let replay = replay(&[aborted_message_end]);

    assert!(replay.ended_aborted);
    assert!(
        !replay.finished,
        "no agent_settled means the run is not finished"
    );
    assert!(replay.events.is_empty());
}

#[test]
fn agent_settled_alone_finishes_without_abort() {
    let settled = serde_json::json!({ "type": "agent_settled" });
    let replay = replay(&[settled]);

    assert!(replay.finished);
    assert!(!replay.ended_aborted);
    assert!(replay.events.is_empty());
}

#[test]
fn non_adapter_records_yield_no_events() {
    let records = fixture("prompt.jsonl");
    let non_adapter: Vec<Value> = records
        .iter()
        .filter(|record| {
            matches!(
                record.get("type").and_then(Value::as_str),
                Some("response") | Some("extension_ui_request")
            )
        })
        .cloned()
        .collect();
    // The trimmed prompt transcript must still exercise these record types.
    assert!(
        !non_adapter.is_empty(),
        "fixture lost its non-adapter records"
    );
    for record in &non_adapter {
        let mapping = map_record(record, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "record must yield no events: {record}"
        );
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }

    // Unknown record types and synthetic noise behave the same.
    for record in [
        serde_json::json!({ "type": "something_new", "data": 1 }),
        serde_json::json!({ "type": "message_start", "message": { "role": "system" } }),
        serde_json::json!({ "type": "turn_start" }),
    ] {
        let mapping = map_record(&record, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "record must yield no events: {record}"
        );
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn malformed_records_never_panic() {
    for record in [
        Value::Null,
        serde_json::json!({}),
        serde_json::json!({ "type": 42 }),
        serde_json::json!({ "type": "message_update" }),
        serde_json::json!({ "type": "message_update", "assistantMessageEvent": "nope" }),
        serde_json::json!({ "type": "message_update", "assistantMessageEvent": { "type": 7, "delta": 1 } }),
        serde_json::json!({ "type": "tool_execution_start", "toolCallId": null }),
        serde_json::json!({ "type": "tool_execution_update", "toolCallId": 9, "partialResult": "x" }),
        serde_json::json!({ "type": "tool_execution_end" }),
        serde_json::json!({ "type": "tool_execution_end", "toolCallId": [1], "result": { "content": 3 } }),
        serde_json::json!({ "type": "message_end", "message": { "stopReason": [] } }),
        serde_json::json!("just a string"),
        serde_json::json!([1, 2, 3]),
    ] {
        // No panics; whatever comes back must be a total mapping.
        let mapping = map_record(&record, SESSION_ID);
        assert!(
            !mapping.finished
                || record.get("type").and_then(Value::as_str) == Some("agent_settled")
        );
    }
}

/// The text of a tool-call content entry, failing on non-text content.
fn tool_content_text(content: &agent_client_protocol::schema::v1::ToolCallContent) -> String {
    match content {
        agent_client_protocol::schema::v1::ToolCallContent::Content(inner) => {
            match &inner.content {
                ContentBlock::Text(text) => text.text.clone(),
                other => panic!("expected a text content block, got {other:?}"),
            }
        }
        other => panic!("expected tool call content, got {other:?}"),
    }
}
