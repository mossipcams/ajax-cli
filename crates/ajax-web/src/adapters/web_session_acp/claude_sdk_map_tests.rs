//! Unit tests for [`super::claude_sdk_map`], replaying trimmed real Claude
//! Agent SDK transcripts from `tests/fixtures/claude_sdk/`.

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{ContentBlock, SessionUpdate, ToolCallStatus};
use serde_json::Value;

use super::claude_sdk_map::map_sdk_message;
use super::client::AcpClientEvent;
use super::pi_rpc_map::PiRpcMapping;

const SESSION_ID: &str = "acp-session-1";

/// Load and parse one trimmed transcript fixture.
fn fixture(name: &str) -> Vec<Value> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude_sdk")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("failed to read fixture {path:?}: {error}");
    });
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("fixture line must be valid JSON"))
        .collect()
}

/// Map every record of one transcript, keeping the per-message mappings so a
/// test can reason about which message produced which events.
fn replay(records: &[Value]) -> Vec<PiRpcMapping> {
    records
        .iter()
        .map(|record| map_sdk_message(record, SESSION_ID))
        .collect()
}

/// All events of all mappings, flattened in order.
fn all_events(mappings: &[PiRpcMapping]) -> Vec<&AcpClientEvent> {
    mappings
        .iter()
        .flat_map(|mapping| mapping.events.iter())
        .collect()
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

/// The first tool-result text carried in one `ToolCallUpdate`'s content.
fn update_result_text(update: &SessionUpdate) -> String {
    let tool_call_update = match update {
        SessionUpdate::ToolCallUpdate(tool_call_update) => tool_call_update,
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    };
    match tool_call_update.fields.content.as_ref() {
        Some(contents) => match &contents[0] {
            agent_client_protocol::schema::v1::ToolCallContent::Content(content) => {
                match &content.content {
                    ContentBlock::Text(text) => text.text.clone(),
                    other => panic!("expected a text content block, got {other:?}"),
                }
            }
            other => panic!("expected a content item, got {other:?}"),
        },
        None => String::new(),
    }
}

#[test]
fn prompt_transcript_maps_text_usage_and_finishes_once() {
    let records = fixture("prompt.jsonl");
    let mappings = replay(&records);
    let events = all_events(&mappings);

    // Every streamed chunk event must be a message/thought chunk; the text is
    // emitted exactly once (the assistant message re-emits nothing), and no
    // chunk is empty.
    let mut message_text: Vec<&str> = Vec::new();
    for event in &events {
        match session_update(event) {
            SessionUpdate::AgentMessageChunk(_) => {
                message_text.push(chunk_text(session_update(event)))
            }
            SessionUpdate::AgentThoughtChunk(_) => {
                assert!(!chunk_text(session_update(event)).is_empty())
            }
            SessionUpdate::UsageUpdate(_) => {}
            other => panic!("unexpected update in prompt transcript: {other:?}"),
        }
    }
    let ok_chunks = message_text.iter().filter(|text| **text == "ok").count();
    assert_eq!(
        ok_chunks, 1,
        "expected exactly one 'ok' text chunk, got {message_text:?}"
    );

    // Exactly one usage event (25875 / 200000), placed before the events of the
    // finishing mapping.
    let usage_positions: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(session_update(event), SessionUpdate::UsageUpdate(_)))
        .map(|(position, _)| position)
        .collect();
    assert_eq!(usage_positions.len(), 1, "expected exactly one UsageUpdate");
    let usage = match session_update(events[usage_positions[0]]) {
        SessionUpdate::UsageUpdate(usage) => usage,
        other => panic!("expected a UsageUpdate, got {other:?}"),
    };
    assert_eq!(usage.used, 25875);
    assert_eq!(usage.size, 200000);

    let finished_positions: Vec<usize> = mappings
        .iter()
        .enumerate()
        .filter(|(_, mapping)| mapping.finished)
        .map(|(position, _)| position)
        .collect();
    assert_eq!(
        finished_positions.len(),
        1,
        "expected exactly one finished mapping"
    );
    let finished_index = finished_positions[0];
    // The usage update arrives with the finishing message (nothing is emitted
    // after it).
    let finish_mapping = &mappings[finished_index];
    let usage_in_finish = finish_mapping
        .events
        .iter()
        .filter(|event| matches!(session_update(event), SessionUpdate::UsageUpdate(_)))
        .count();
    assert_eq!(
        usage_in_finish, 1,
        "usage event must be part of the finishing message"
    );
    assert_eq!(
        finish_mapping.events.len(),
        1,
        "no events may follow the usage update"
    );

    assert!(
        mappings.iter().all(|mapping| !mapping.ended_aborted),
        "prompt run must not be aborted"
    );
}

#[test]
fn toolcall_transcript_opens_updates_and_completes_a_tool_call() {
    let records = fixture("toolcall.jsonl");
    let mappings = replay(&records);
    let events = all_events(&mappings);

    // The content_block_start opens an Edit tool call in progress.
    let tool_call_position = events
        .iter()
        .position(|event| matches!(session_update(event), SessionUpdate::ToolCall(_)))
        .expect("expected a ToolCall event");
    match session_update(events[tool_call_position]) {
        SessionUpdate::ToolCall(tool_call) => {
            assert_eq!(
                tool_call.kind,
                agent_client_protocol::schema::v1::ToolKind::Edit
            );
            assert_eq!(tool_call.status, ToolCallStatus::InProgress);
        }
        other => panic!("expected a ToolCall update, got {other:?}"),
    }

    // A later assistant tool_use update carries the complete raw input.
    let _raw_input_position: usize = events[tool_call_position + 1..]
        .iter()
        .position(|event| match session_update(event) {
            SessionUpdate::ToolCallUpdate(update) => matches!(
                &update.fields.raw_input,
                Some(Value::Object(object)) if object.contains_key("file_path")
            ),
            _ => false,
        })
        .map(|offset| tool_call_position + 1 + offset)
        .expect("expected a later ToolCallUpdate with a file_path raw_input");

    // The tool_result settles the call as completed with non-empty text.
    let completed_position = events[tool_call_position + 1..]
        .iter()
        .position(|event| match session_update(event) {
            SessionUpdate::ToolCallUpdate(update) => {
                update.fields.status == Some(ToolCallStatus::Completed)
            }
            _ => false,
        })
        .map(|offset| tool_call_position + 1 + offset)
        .expect("expected a Completed ToolCallUpdate");
    assert!(!update_result_text(session_update(events[completed_position])).is_empty());

    let usage = events
        .iter()
        .find_map(|event| match session_update(event) {
            SessionUpdate::UsageUpdate(usage) => Some(usage),
            _ => None,
        })
        .expect("expected a UsageUpdate");
    assert_eq!(usage.used, 55672);
    assert_eq!(usage.size, 200000);

    let finished_count = mappings.iter().filter(|mapping| mapping.finished).count();
    assert_eq!(finished_count, 1, "expected exactly one finished mapping");
}

#[test]
fn interrupt_transcript_aborts_then_restarts_with_usage() {
    let records = fixture("interrupt.jsonl");
    let mappings = replay(&records);
    let finished: Vec<&PiRpcMapping> = mappings.iter().filter(|mapping| mapping.finished).collect();
    assert_eq!(finished.len(), 2, "expected exactly two finished mappings");

    // The interrupted run is aborted and must not emit a zero usage update.
    assert!(finished[0].ended_aborted, "first run must be aborted");
    assert!(
        !finished[0]
            .events
            .iter()
            .any(|event| matches!(session_update(event), SessionUpdate::UsageUpdate(_))),
        "aborted zero-usage run must not emit a UsageUpdate"
    );

    // The restarted run finishes cleanly with real usage.
    assert!(!finished[1].ended_aborted, "second run must not be aborted");
    let usage = finished[1]
        .events
        .iter()
        .find_map(|event| match session_update(event) {
            SessionUpdate::UsageUpdate(usage) => Some(usage),
            _ => None,
        })
        .expect("expected a UsageUpdate in the second run");
    assert_eq!(usage.used, 26781);
    assert_eq!(usage.size, 200000);

    // The '[Request interrupted by user]' user message produces no events.
    let interrupt_records: Vec<&Value> = records
        .iter()
        .filter(|record| record.to_string().contains("[Request interrupted by user]"))
        .collect();
    assert!(
        !interrupt_records.is_empty(),
        "fixture must contain the interrupt marker"
    );
    for record in interrupt_records {
        let mapping = map_sdk_message(record, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "interrupt user message must produce no events"
        );
    }
}

#[test]
fn sub_agent_messages_map_to_empty() {
    let message = serde_json::json!({
        "type": "assistant",
        "parent_tool_use_id": "toolu_123",
        "message": {"role": "assistant", "content": [{"type": "text", "text": "hi"}]}
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.events.is_empty());
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn tool_result_status_and_content() {
    let failed = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t1", "is_error": true, "content": "boom"}
        ]}
    });
    let mapping = map_sdk_message(&failed, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match session_update(&mapping.events[0]) {
        SessionUpdate::ToolCallUpdate(update) => {
            assert_eq!(update.fields.status, Some(ToolCallStatus::Failed));
            assert_eq!(
                update_result_text(session_update(&mapping.events[0])),
                "boom"
            );
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }

    let joined = serde_json::json!({
        "type": "user",
        "message": {"role": "user", "content": [
            {"type": "tool_result", "tool_use_id": "t2",
             "content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}
        ]}
    });
    let mapping = map_sdk_message(&joined, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match session_update(&mapping.events[0]) {
        SessionUpdate::ToolCallUpdate(update) => {
            assert_eq!(update.fields.status, Some(ToolCallStatus::Completed));
            assert_eq!(
                update_result_text(session_update(&mapping.events[0])),
                "a\nb"
            );
        }
        other => panic!("expected a ToolCallUpdate, got {other:?}"),
    }
}

#[test]
fn meaningless_and_non_object_messages_map_to_empty() {
    let messages: Vec<Value> = vec![
        serde_json::json!({"type": "unknown_type"}),
        serde_json::json!({"type": "system", "subtype": "init", "cwd": "/work/path"}),
        serde_json::json!({"type": "rate_limit_event"}),
        Value::String("not an object".to_string()),
        Value::Number(42.into()),
        Value::Null,
    ];
    for message in messages {
        let mapping = map_sdk_message(&message, SESSION_ID);
        assert!(
            mapping.events.is_empty(),
            "expected no events for {message:?}"
        );
        assert!(!mapping.finished);
        assert!(!mapping.ended_aborted);
    }
}

#[test]
fn non_aborted_error_result_finishes_with_an_error_event() {
    let message = serde_json::json!({
        "type": "result",
        "subtype": "error_max_turns",
        "is_error": true,
        "stop_reason": null
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert_eq!(mapping.events.len(), 1, "expected exactly the error event");
    match &mapping.events[0] {
        AcpClientEvent::Error(text) => assert!(text.contains("error_max_turns")),
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn result_without_usage_finishes_silently() {
    let message = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": false,
        "stop_reason": "end_turn"
    });
    let mapping = map_sdk_message(&message, SESSION_ID);
    assert!(mapping.finished);
    assert!(!mapping.ended_aborted);
    assert!(mapping.events.is_empty());
}
