//! Pure mapping from Pi RPC-mode (`pi --mode rpc`) stdout records to the native
//! [`AgentEvent`] values consumed by the web session adapter.
//!
//! The connection loop that reads the JSONL stream (a later task) calls
//! [`map_record`] once per parsed record and folds the returned
//! [`RpcMapping`] values: each emitted event is forwarded to the session in
//! order, and `finished` / `ended_aborted` track whether the run settled and
//! whether it ended aborted. Records that carry no adapter meaning
//! (`response`, the `extension_ui_request` methods other than dialogs and
//! `notify`, and unknown types) map to an empty result and never panic on missing or malformed fields.

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeEvent};
use super::rpc_session::RpcMapping;

/// Map one parsed Pi RPC stdout record to adapter events and run state.
///
/// `session_id` is retained for interface parity with the ACP mappers; native
/// events carry no session id (the session slice binds them). The mapping is
/// total: any record — including `extension_ui_request` dialogs, the
/// fire-and-forget methods, and unknown types — returns an empty result
/// instead of panicking.
pub fn map_record(record: &Value, _session_id: &str) -> RpcMapping {
    let mut events = Vec::new();
    let finished = match record.get("type").and_then(Value::as_str).unwrap_or("") {
        "message_update" => map_message_update(record, &mut events),
        "tool_execution_start" => {
            if let Some(tool_call) = tool_call_started(record) {
                events.push(AgentEvent::Native(tool_call));
            }
            false
        }
        "tool_execution_update" => {
            if let Some(update) = tool_call_progress(record) {
                events.push(AgentEvent::Native(update));
            }
            false
        }
        "tool_execution_end" => {
            events.push(AgentEvent::Native(tool_call_finished(record)));
            false
        }
        "compaction_end" => {
            // A successful `/compact` is otherwise invisible (the assistant
            // message it interrupts never ends), so confirm it. Failed manual
            // compactions surface no text here: the compact command's own
            // failure response already rejects the turn with that error.
            if record.get("reason").and_then(Value::as_str) == Some("manual")
                && record.get("aborted").and_then(Value::as_bool) != Some(true)
                && string_field(record, "errorMessage").is_none()
            {
                events.push(AgentEvent::Native(NativeEvent::AgentText(
                    manual_compaction_text(record),
                )));
            }
            false
        }
        "agent_settled" => true,
        // Dialogs (`confirm`, `select`, `input`, `editor`) become elicitation
        // requests; `notify` is surfaced as agent text (or an error when its
        // `notifyType` is `"error"`); the other fire-and-forget methods
        // (`setStatus`, `setWidget`), lifecycle records without adapter
        // output, and unknown record types produce no events.
        "extension_ui_request" => {
            if let Some(event) = dialog_request(record).or_else(|| notify_event(record)) {
                events.push(AgentEvent::Native(event));
            }
            false
        }
        _ => false,
    };
    if let Some(PiRunError::Failed(text) | PiRunError::Notice(text)) = run_error(record) {
        events.push(AgentEvent::Native(NativeEvent::Error(text)));
    }
    RpcMapping {
        events,
        finished,
        ended_aborted: record_reports_aborted(record),
    }
}

/// How one Pi record bears on the error state of the current run. The
/// assistant error memo itself lives in the session (see
/// `rpc_session::fold_pending_error`); `Failed` and `Notice` are also emitted
/// as events by [`map_record`].
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PiRunError {
    /// An assistant message ended with `stopReason: "error"`. Pi may still
    /// retry it, so it is only surfaced if the run settles without a retry.
    Pending(String),
    /// A retry started or a later assistant message ended, so any pending
    /// assistant error was superseded.
    Clear,
    /// The run itself failed (final retry failure or a failed compaction).
    Failed(String),
    /// An extension failed; the run's own outcome is unaffected.
    Notice(String),
}

/// The run-error meaning of one Pi record, if any.
pub(super) fn run_error(record: &Value) -> Option<PiRunError> {
    match record.get("type").and_then(Value::as_str)? {
        "message_end" => {
            let message = record.get("message")?;
            if message.get("role").and_then(Value::as_str) != Some("assistant") {
                return None;
            }
            match message.get("stopReason").and_then(Value::as_str) {
                Some("error") => Some(PiRunError::Pending(
                    string_field(message, "errorMessage")
                        .unwrap_or_else(|| "assistant message ended with an error".to_string()),
                )),
                _ => Some(PiRunError::Clear),
            }
        }
        "auto_retry_start" => Some(PiRunError::Clear),
        "auto_retry_end" => {
            if record.get("success").and_then(Value::as_bool) == Some(false) {
                Some(PiRunError::Failed(
                    string_field(record, "finalError")
                        .unwrap_or_else(|| "automatic retry failed".to_string()),
                ))
            } else {
                Some(PiRunError::Clear)
            }
        }
        "compaction_start" => (record.get("reason").and_then(Value::as_str) == Some("overflow"))
            .then_some(PiRunError::Clear),
        "compaction_end" => {
            if record.get("aborted").and_then(Value::as_bool) == Some(true)
                || record.get("reason").and_then(Value::as_str) == Some("manual")
            {
                return None;
            }
            string_field(record, "errorMessage").map(PiRunError::Failed)
        }
        "extension_error" => {
            let path = string_field(record, "extensionPath")
                .unwrap_or_else(|| "unknown extension".to_string());
            let event =
                string_field(record, "event").unwrap_or_else(|| "unknown event".to_string());
            let error =
                string_field(record, "error").unwrap_or_else(|| "unknown error".to_string());
            Some(PiRunError::Notice(format!(
                "Pi extension {path} failed during {event}: {error}"
            )))
        }
        _ => None,
    }
}

/// Build an elicitation request from one `extension_ui_request` record that
/// opens a dialog (`confirm`, `select`, `input`, `editor`). The other
/// fire-and-forget methods (`setStatus`, `setWidget`, and anything else)
/// return `None` (`notify` is surfaced separately by [`notify_event`]), as do
/// records without a string `id`.
fn dialog_request(record: &Value) -> Option<NativeEvent> {
    let request_id = record.get("id")?.as_str()?.to_string();
    match record.get("method")?.as_str()? {
        "confirm" => {
            let text = string_field(record, "message").or_else(|| string_field(record, "title"))?;
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: text.clone(),
                schema: json!({
                    "type": "object",
                    "properties": { "confirmed": { "type": "boolean", "title": text } },
                    "required": ["confirmed"]
                }),
            })
        }
        "select" => {
            let title = string_field(record, "title")?;
            let options = record.get("options").and_then(Value::as_array)?;
            let enum_values: Vec<Value> = options
                .iter()
                .filter_map(Value::as_str)
                .map(Value::from)
                .collect();
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title.clone(),
                schema: json!({
                    "type": "object",
                    "properties": { "value": { "type": "string", "title": title, "enum": enum_values } },
                    "required": ["value"]
                }),
            })
        }
        "input" => {
            let title = string_field(record, "title")?;
            let mut schema = json!({
                "type": "object",
                "properties": { "value": { "type": "string", "title": title } },
                "required": ["value"]
            });
            if let Some(placeholder) = string_field(record, "placeholder") {
                schema["properties"]["value"]["description"] = json!(placeholder);
            }
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title,
                schema,
            })
        }
        "editor" => {
            let title = string_field(record, "title")?;
            let mut schema = json!({
                "type": "object",
                "properties": { "value": { "type": "string", "title": title } },
                "required": ["value"]
            });
            if let Some(prefill) = record.get("prefill") {
                schema["properties"]["value"]["default"] = prefill.clone();
            }
            Some(NativeEvent::ElicitationRequest {
                request_id,
                message: title,
                schema,
            })
        }
        _ => None,
    }
}

/// Surface one fire-and-forget `notify` from an `extension_ui_request`
/// record (issue #1361): `"error"` notifies become [`NativeEvent::Error`],
/// everything else becomes agent text with a trailing newline so chat renders
/// it on its own line. Returns `None` for other methods and for records
/// without a non-empty string `message`.
fn notify_event(record: &Value) -> Option<NativeEvent> {
    if record.get("method")?.as_str()? != "notify" {
        return None;
    }
    let message = string_field(record, "message")?;
    if message.trim().is_empty() {
        return None;
    }
    Some(
        if record.get("notifyType").and_then(Value::as_str) == Some("error") {
            NativeEvent::Error(message)
        } else {
            NativeEvent::AgentText(format!("{message}\n"))
        },
    )
}

/// One string field of an `extension_ui_request` record, if present.
fn string_field(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Map one `message_update` record's `assistantMessageEvent`, if it carries
/// text. Message starts/ends and streamed tool-call argument deltas carry no
/// adapter output.
fn map_message_update(record: &Value, events: &mut Vec<AgentEvent>) -> bool {
    let Some(event) = record.get("assistantMessageEvent") else {
        return false;
    };
    let Some(delta) = event.get("delta").and_then(Value::as_str) else {
        return false;
    };
    let update = match event.get("type").and_then(Value::as_str) {
        Some("text_delta") => NativeEvent::AgentText(delta.to_string()),
        Some("thinking_delta") => NativeEvent::AgentThought(delta.to_string()),
        _ => return false,
    };
    events.push(AgentEvent::Native(update));
    false
}

fn tool_call_started(record: &Value) -> Option<NativeEvent> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let name = record.get("toolName").and_then(Value::as_str)?;
    let args = record.get("args").filter(|args| args.is_object());
    let title = match args.and_then(tool_call_detail) {
        Some(detail) => format!("{name}: {detail}"),
        None => name.to_string(),
    };
    Some(NativeEvent::ToolCall {
        id: id.to_string(),
        title,
        kind: tool_kind(name),
        status: "in_progress".to_string(),
        raw_input: Some(args.cloned().unwrap_or(Value::Null)),
    })
}

/// The native tool kind for one Pi tool name (compared case-insensitively).
fn tool_kind(tool_name: &str) -> String {
    match tool_name.to_ascii_lowercase().as_str() {
        "read" => "read",
        "edit" | "write" => "edit",
        "grep" | "find" | "ls" => "search",
        "bash" => "execute",
        _ => "other",
    }
    .to_string()
}

/// The first non-empty string among `args.command`, `args.pattern`, and
/// `args.path`, if any. The search pattern outranks the path so `grep`/`find`
/// titles show what was searched even when a path is also given.
fn tool_call_detail(args: &Value) -> Option<String> {
    for key in ["command", "pattern", "path"] {
        if let Some(detail) = args.get(key).and_then(Value::as_str) {
            if !detail.trim().is_empty() {
                return Some(detail.to_string());
            }
        }
    }
    None
}

fn tool_call_progress(record: &Value) -> Option<NativeEvent> {
    let id = record.get("toolCallId").and_then(Value::as_str)?;
    let texts = pi_text_blocks(record.get("partialResult")?.get("content")?.as_array()?);
    let joined = texts.join("");
    (!joined.trim().is_empty()).then(|| NativeEvent::ToolCallUpdate {
        id: id.to_string(),
        status: "in_progress".to_string(),
        content: Some(joined),
        raw_input: None,
    })
}

fn tool_call_finished(record: &Value) -> NativeEvent {
    let id = record
        .get("toolCallId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = if record
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "failed".to_string()
    } else {
        "completed".to_string()
    };
    let texts = record
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .map(|blocks| pi_text_blocks(blocks))
        .unwrap_or_default();
    NativeEvent::ToolCallUpdate {
        id: id.to_string(),
        status,
        content: Some(texts.join("")),
        raw_input: None,
    }
}

/// `message_end` / `turn_end` record reporting an aborted assistant message.
fn record_reports_aborted(record: &Value) -> bool {
    matches!(
        record
            .get("message")
            .and_then(|m| m.get("stopReason"))
            .and_then(Value::as_str),
        Some("aborted")
    )
}

/// A short confirmation for a successful manual compaction (`/compact`), with
/// the token counts when the record carries them.
fn manual_compaction_text(record: &Value) -> String {
    let result = record.get("result");
    let before = result
        .and_then(|r| r.get("tokensBefore"))
        .and_then(Value::as_u64);
    let after = result
        .and_then(|r| r.get("estimatedTokensAfter"))
        .and_then(Value::as_u64);
    match (before, after) {
        (Some(before), Some(after)) => {
            format!("Context compacted: {before} \u{2192} {after} tokens.\n")
        }
        _ => "Context compacted.\n".to_string(),
    }
}

/// The `text` values of Pi content blocks, in order. Blocks without a string
/// `text` field are skipped so unexpected block shapes never fail the mapping.
fn pi_text_blocks(blocks: &[Value]) -> Vec<String> {
    blocks
        .iter()
        .filter_map(|block| {
            block
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect()
}
