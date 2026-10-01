//! Signature and success evidence for canonical activity events.
//!
//! The signature is a stable, bounded digest of a tool call's input so the
//! agent watcher can detect repeated calls. Only the tool name and a
//! normalised, whitespace-collapsed, length-capped summary of the input
//! fields are digested; raw command text, tool output, and transcripts are
//! never stored in the event envelope.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SUMMARY_LIMIT: usize = 256;
const SUMMARY_KEYS: [&str; 5] = ["command", "file_path", "path", "pattern", "url"];

/// 16-hex lowercase FNV-1a-64 digest of `input`.
pub(crate) fn fnv1a64_hex(input: &str) -> String {
    let mut hash = FNV_OFFSET_BASIS;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

fn string_at<'a>(value: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    value
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_str())
}

/// Normalised bounded summary of the tool's input fields: the string values of
/// `command`/`file_path`/`path`/`pattern`/`url` (from `tool_input` first, then
/// top level, in that fixed order), whitespace-collapsed, truncated to the
/// first 256 chars.
pub(crate) fn normalised_summary(payload: &serde_json::Value) -> String {
    let tool_input = payload.get("tool_input");
    let mut parts: Vec<&str> = Vec::new();
    for key in SUMMARY_KEYS {
        if let Some(value) = string_at(tool_input, key) {
            parts.push(value);
        }
        if let Some(value) = string_at(Some(payload), key) {
            parts.push(value);
        }
    }
    let collapsed = parts
        .join("\n")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.chars().take(SUMMARY_LIMIT).collect()
}

/// Tool name from the payload's `tool_name` or `tool` field.
pub(crate) fn tool_name_from_payload(payload: &serde_json::Value) -> Option<String> {
    ["tool_name", "tool"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(|value| value.as_str()))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

/// `<tool_name>:<16-hex FNV-1a-64 of the normalised bounded summary>`.
///
/// Stable for identical calls and different for different inputs; `None` when
/// the payload carries no tool name.
pub(crate) fn activity_signature(payload: &serde_json::Value) -> Option<String> {
    let tool_name = tool_name_from_payload(payload)?;
    Some(format!(
        "{tool_name}:{}",
        fnv1a64_hex(&normalised_summary(payload))
    ))
}

/// Success evidence for a finished activity event.
///
/// `failed` marks client events that only fire on failure (cursor
/// `postToolUseFailure`). Otherwise `tool_response.is_error == true` or a
/// non-empty top-level `error` mean `Some(false)`; a present
/// `tool_response`/`result` means `Some(true)`; anything else is `None`.
pub(crate) fn activity_success(failed: bool, payload: &serde_json::Value) -> Option<bool> {
    if failed {
        return Some(false);
    }
    let is_error = payload
        .get("tool_response")
        .and_then(|response| response.get("is_error"))
        .and_then(|value| value.as_bool());
    if is_error == Some(true) {
        return Some(false);
    }
    let has_error = payload.get("error").is_some_and(|value| {
        !value.is_null() && !matches!(value, serde_json::Value::String(text) if text.is_empty())
    });
    if has_error {
        return Some(false);
    }
    let has_response = payload
        .get("tool_response")
        .is_some_and(|value| !value.is_null())
        || payload.get("result").is_some_and(|value| !value.is_null());
    has_response.then_some(true)
}
