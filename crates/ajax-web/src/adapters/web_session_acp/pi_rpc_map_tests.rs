//! Unit tests for [`super::pi_rpc_map`], replaying trimmed real Pi RPC-mode
//! transcripts from `tests/fixtures/pi_rpc/`.

use std::path::PathBuf;

use serde_json::Value;

use super::agent_event::{AgentEvent, NativeEvent};
use super::pi_rpc_map::map_record;

const SESSION_ID: &str = "acp-session-1";

/// The result of folding [`map_record`] over one whole transcript.
struct Replay {
    events: Vec<AgentEvent>,
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

/// The native event of an event that must be `AgentEvent::Native`, failing on
/// anything else (an ACP-wrapped event would fail the test here).
fn native_event(event: &AgentEvent) -> &NativeEvent {
    match event {
        AgentEvent::Native(native) => native,
        other => panic!("expected a Native event, got {other:?}"),
    }
}

#[test]
fn prompt_transcript_yields_one_message_chunk_and_settles() {
    let replay = replay(&fixture("prompt.jsonl"));

    // The trimmed prompt transcript streams exactly one text delta ("ok").
    // `setStatus` / `setWidget` / `response` records yield no events.
    assert_eq!(replay.events.len(), 1);
    match native_event(&replay.events[0]) {
        NativeEvent::AgentText(text) => assert_eq!(text, "ok"),
        other => panic!("expected an AgentText event, got {other:?}"),
    }
    assert!(replay.finished, "agent_settled must finish the run");
    assert!(!replay.ended_aborted);
}

#[test]
fn toolcall_transcript_yields_tool_call_and_updates() {
    let replay = replay(&fixture("toolcall.jsonl"));
    let events = &replay.events;

    // Four thought deltas, the tool call, one partial update, the completed
    // update, then the final "done" message chunk.
    assert_eq!(events.len(), 8);

    let thought_text: String = events[..4]
        .iter()
        .map(|event| match native_event(event) {
            NativeEvent::AgentThought(text) => text.as_str(),
            other => panic!("expected an AgentThought event, got {other:?}"),
        })
        .collect();
    assert_eq!(thought_text, "The user wants me");

    let NativeEvent::ToolCall {
        id,
        title,
        kind,
        status,
        raw_input,
    } = native_event(&events[4])
    else {
        panic!("expected a ToolCall event, got {:?}", events[4]);
    };
    assert_eq!(id.as_str(), "chatcmpl-tool-be93830d9b11882f");
    assert_eq!(title.as_str(), "bash: echo hi");
    assert_eq!(kind.as_str(), "execute");
    assert_eq!(status.as_str(), "in_progress");
    assert_eq!(
        raw_input
            .as_ref()
            .and_then(|raw| raw.get("command"))
            .and_then(Value::as_str),
        Some("echo hi")
    );

    // Partial result update: still in progress, carrying the partial text.
    let NativeEvent::ToolCallUpdate {
        id,
        status,
        content,
        ..
    } = native_event(&events[5])
    else {
        panic!("expected a ToolCallUpdate event, got {:?}", events[5]);
    };
    assert_eq!(id.as_str(), "chatcmpl-tool-be93830d9b11882f");
    assert_eq!(status.as_str(), "in_progress");
    assert_eq!(content.as_deref(), Some("hi\n"));

    // Final update: completed with the tool result text.
    let NativeEvent::ToolCallUpdate {
        id,
        status,
        content,
        ..
    } = native_event(&events[6])
    else {
        panic!("expected a ToolCallUpdate event, got {:?}", events[6]);
    };
    assert_eq!(id.as_str(), "chatcmpl-tool-be93830d9b11882f");
    assert_eq!(status.as_str(), "completed");
    assert_eq!(content.as_deref(), Some("hi\n"));

    match native_event(&events[7]) {
        NativeEvent::AgentText(text) => assert_eq!(text, "done"),
        other => panic!("expected an AgentText event, got {other:?}"),
    }
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
    match native_event(&replay.events[0]) {
        NativeEvent::ToolCallUpdate {
            status, content, ..
        } => {
            assert_eq!(status.as_str(), "failed");
            assert_eq!(content.as_deref(), Some("boom\n"));
        }
        other => panic!("expected a ToolCallUpdate event, got {other:?}"),
    }
    assert!(!replay.finished);
}

#[test]
fn abort_transcript_reports_finished_and_aborted() {
    let replay = replay(&fixture("abort.jsonl"));
    let events = &replay.events;

    // Three thought deltas then three text deltas.
    assert_eq!(events.len(), 6);
    let thought_text: String = events[..3]
        .iter()
        .map(|event| match native_event(event) {
            NativeEvent::AgentThought(text) => text.as_str(),
            other => panic!("expected an AgentThought event, got {other:?}"),
        })
        .collect();
    assert_eq!(thought_text, "A writing task");
    let message_text: String = events[3..]
        .iter()
        .map(|event| match native_event(event) {
            NativeEvent::AgentText(text) => text.as_str(),
            other => panic!("expected an AgentText event, got {other:?}"),
        })
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

#[test]
fn confirm_dialog_maps_to_elicitation_request() {
    let mapping = map_record(
        &serde_json::json!({
            "type": "extension_ui_request",
            "id": "ui-1",
            "method": "confirm",
            "message": "Delete branch?"
        }),
        SESSION_ID,
    );
    assert_eq!(mapping.events.len(), 1);
    let native = native_event(&mapping.events[0]);
    match native {
        NativeEvent::ElicitationRequest {
            request_id,
            message,
            schema,
        } => {
            assert_eq!(request_id, "ui-1");
            assert_eq!(message, "Delete branch?");
            let confirmed = &schema["properties"]["confirmed"];
            assert_eq!(confirmed["type"], "boolean");
            assert!(schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == "confirmed"));
        }
        other => panic!("expected ElicitationRequest, got {other:?}"),
    }
}

#[test]
fn select_dialog_carries_enum_options() {
    let mapping = map_record(
        &serde_json::json!({
            "type": "extension_ui_request",
            "id": "ui-2",
            "method": "select",
            "title": "Pick target",
            "options": ["a", "b"]
        }),
        SESSION_ID,
    );
    assert_eq!(mapping.events.len(), 1);
    let native = native_event(&mapping.events[0]);
    match native {
        NativeEvent::ElicitationRequest {
            request_id,
            message,
            schema,
        } => {
            assert_eq!(request_id, "ui-2");
            assert_eq!(message, "Pick target");
            let value = &schema["properties"]["value"];
            assert_eq!(value["type"], "string");
            let enum_values = value["enum"].as_array().unwrap();
            assert_eq!(enum_values.len(), 2);
            assert_eq!(enum_values[0], "a");
            assert_eq!(enum_values[1], "b");
        }
        other => panic!("expected ElicitationRequest, got {other:?}"),
    }
}

#[test]
fn fire_and_forget_methods_map_to_nothing() {
    for method in ["setStatus", "setWidget", "setTitle"] {
        let mapping = map_record(
            &serde_json::json!({
                "type": "extension_ui_request",
                "id": "ui-3",
                "method": method,
                "message": "hello"
            }),
            SESSION_ID,
        );
        assert!(
            mapping.events.is_empty(),
            "{method} should produce no events"
        );
    }
}

#[test]
fn extension_notify_is_surfaced_1361() {
    // Issue #1361: Pi extension `notify` records were dropped, so commands
    // like `/extensions` showed nothing in the native chat. Absent and
    // non-`error` `notifyType` values surface as agent text with a trailing
    // newline; `"error"` surfaces as an Error event.
    let record = |notify_type: Option<&str>| {
        let mut record = serde_json::json!({
            "type": "extension_ui_request",
            "id": "u5",
            "method": "notify",
            "message": "3 extensions loaded"
        });
        if let Some(notify_type) = notify_type {
            record["notifyType"] = serde_json::json!(notify_type);
        }
        record
    };

    for notify_type in [None, Some("info"), Some("warning")] {
        let mapping = map_record(&record(notify_type), SESSION_ID);
        assert_eq!(
            mapping.events.len(),
            1,
            "notify (notifyType {notify_type:?}) must surface exactly one event"
        );
        match native_event(&mapping.events[0]) {
            NativeEvent::AgentText(text) => assert_eq!(text, "3 extensions loaded\n"),
            other => panic!("expected an AgentText event, got {other:?}"),
        }
        assert!(!mapping.finished, "notify must not finish the run");
    }

    let mapping = map_record(&record(Some("error")), SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    match native_event(&mapping.events[0]) {
        NativeEvent::Error(text) => assert_eq!(text, "3 extensions loaded"),
        other => panic!("expected an Error event, got {other:?}"),
    }

    // A notify without a non-empty string message surfaces nothing.
    let mut silent = record(None);
    silent["message"] = serde_json::json!("");
    assert!(map_record(&silent, SESSION_ID).events.is_empty());
}

#[test]
fn dialog_without_id_maps_to_nothing() {
    let mapping = map_record(
        &serde_json::json!({
            "type": "extension_ui_request",
            "method": "confirm",
            "message": "Delete branch?"
        }),
        SESSION_ID,
    );
    assert!(mapping.events.is_empty());
}

/// The error text of the single `NativeEvent::Error` in `events`, failing on
/// any other event count or shape.
fn sole_error(events: &[AgentEvent]) -> &str {
    let [event] = events else {
        panic!("expected exactly one event, got {events:?}");
    };
    match native_event(event) {
        NativeEvent::Error(text) => text.as_str(),
        other => panic!("expected an Error event, got {other:?}"),
    }
}

#[test]
fn assistant_error_message_end_is_held_not_emitted_1335() {
    // The assistant error alone is not an event: Pi may still retry it.
    let record = serde_json::json!({
        "type": "message_end",
        "message": { "role": "assistant", "stopReason": "error", "errorMessage": "529 overloaded" },
    });
    let mapping = map_record(&record, SESSION_ID);

    assert!(mapping.events.is_empty());
    assert!(!mapping.finished);
    assert!(!mapping.ended_aborted);
}

#[test]
fn auto_retry_end_failure_emits_final_error_1335() {
    let record = serde_json::json!({
        "type": "auto_retry_end",
        "success": false,
        "attempt": 3,
        "finalError": "529 overloaded after 3 attempts",
    });
    let mapping = map_record(&record, SESSION_ID);

    assert_eq!(
        sole_error(&mapping.events),
        "529 overloaded after 3 attempts"
    );
    assert!(!mapping.finished);
}

#[test]
fn auto_retry_end_success_and_retry_start_emit_nothing_1335() {
    for record in [
        serde_json::json!({ "type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 2000, "errorMessage": "529 overloaded" }),
        serde_json::json!({ "type": "auto_retry_end", "success": true, "attempt": 2 }),
    ] {
        assert!(
            map_record(&record, SESSION_ID).events.is_empty(),
            "{record}"
        );
    }
}

#[test]
fn compaction_end_error_emits_error_1335() {
    let record = serde_json::json!({
        "type": "compaction_end",
        "reason": "overflow",
        "aborted": false,
        "willRetry": false,
        "errorMessage": "compaction model unavailable",
    });
    let mapping = map_record(&record, SESSION_ID);

    assert_eq!(sole_error(&mapping.events), "compaction model unavailable");
}

#[test]
fn aborted_compaction_emits_nothing_1335() {
    let record = serde_json::json!({
        "type": "compaction_end",
        "reason": "overflow",
        "aborted": true,
        "willRetry": false,
        "errorMessage": "cancelled by user",
    });

    assert!(map_record(&record, SESSION_ID).events.is_empty());
}

#[test]
fn extension_error_names_extension_and_error_1335() {
    let record = serde_json::json!({
        "type": "extension_error",
        "extensionPath": "/p/ext.ts",
        "event": "tool_call",
        "error": "boom",
    });
    let mapping = map_record(&record, SESSION_ID);
    let text = sole_error(&mapping.events);

    assert!(text.contains("boom"), "{text}");
    assert!(text.contains("/p/ext.ts"), "{text}");
    assert!(!mapping.finished);
}

/// The `NativeEvent::ToolCall` of the single event in `events`, failing on
/// any other count or shape.
fn sole_tool_call(events: &[AgentEvent]) -> &NativeEvent {
    let [event] = events else {
        panic!("expected exactly one event, got {events:?}");
    };
    native_event(event)
}

#[test]
fn tool_kind_and_title_map_by_tool_name_1339() {
    // (toolName, args) -> expected (kind, title).
    for (name, args, kind, title) in [
        (
            "read",
            serde_json::json!({ "path": "/tmp/a.txt" }),
            "read",
            "read: /tmp/a.txt",
        ),
        (
            "edit",
            serde_json::json!({ "path": "/tmp/b.rs", "oldText": "x", "newText": "y" }),
            "edit",
            "edit: /tmp/b.rs",
        ),
        (
            "write",
            serde_json::json!({ "path": "/tmp/c.md", "content": "hello" }),
            "edit",
            "write: /tmp/c.md",
        ),
        (
            "grep",
            // `pattern` outranks `path`; the with-path case has its own test.
            serde_json::json!({ "pattern": "foo" }),
            "search",
            "grep: foo",
        ),
        (
            "find",
            serde_json::json!({ "pattern": "*.rs" }),
            "search",
            "find: *.rs",
        ),
        ("ls", serde_json::json!({ "path": "." }), "search", "ls: ."),
        (
            "bash",
            serde_json::json!({ "command": "echo hi" }),
            "execute",
            "bash: echo hi",
        ),
        (
            "mystery_tool",
            serde_json::json!({}),
            "other",
            "mystery_tool",
        ),
    ] {
        let record = serde_json::json!({
            "type": "tool_execution_start",
            "toolCallId": "call-1",
            "toolName": name,
            "args": args
        });
        let mapping = map_record(&record, SESSION_ID);
        assert_eq!(mapping.events.len(), 1, "{name}");
        let NativeEvent::ToolCall {
            title: got_title,
            kind: got_kind,
            status,
            ..
        } = sole_tool_call(&mapping.events)
        else {
            panic!(
                "expected a ToolCall event for {name}, got {:?}",
                mapping.events[0]
            );
        };
        assert_eq!(got_title.as_str(), title, "{name}");
        assert_eq!(got_kind.as_str(), kind, "{name}");
        assert_eq!(status.as_str(), "in_progress");
    }
}

#[test]
fn tool_call_without_detail_args_keeps_bare_name_1339() {
    let record = serde_json::json!({
        "type": "tool_execution_start",
        "toolCallId": "call-2",
        "toolName": "bash"
    });
    let mapping = map_record(&record, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    let NativeEvent::ToolCall { title, kind, .. } = sole_tool_call(&mapping.events) else {
        panic!("expected a ToolCall event, got {:?}", mapping.events[0]);
    };
    assert_eq!(title.as_str(), "bash");
    assert_eq!(kind.as_str(), "execute");
}

#[test]
fn grep_title_prefers_pattern_over_path_1339() {
    let record = serde_json::json!({
        "type": "tool_execution_start",
        "toolCallId": "call-3",
        "toolName": "grep",
        "args": { "pattern": "TODO", "path": "src" }
    });
    let mapping = map_record(&record, SESSION_ID);
    assert_eq!(mapping.events.len(), 1);
    let NativeEvent::ToolCall { title, kind, .. } = sole_tool_call(&mapping.events) else {
        panic!("expected a ToolCall event, got {:?}", mapping.events[0]);
    };
    assert_eq!(title.as_str(), "grep: TODO");
    assert_eq!(kind.as_str(), "search");
}
