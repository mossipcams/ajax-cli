//! Signature and success evidence for canonical activity events.
//!
//! The signature is a stable, bounded digest of a tool call's input so the
//! agent watcher can detect repeated calls. It is the tool name plus an
//! FNV-1a-64 digest of the canonical JSON string of the whole `tool_input`
//! value (serde_json maps are sorted, so the string is canonical),
//! truncated to its first 4096 bytes on a char boundary before hashing.
//! Payloads without a `tool_input` fall back to a bounded summary of the
//! top-level keys. Raw command text, tool output, and transcripts are never
//! stored in the event envelope.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SUMMARY_LIMIT: usize = 256;
const SUMMARY_KEYS: [&str; 5] = ["command", "file_path", "path", "pattern", "url"];
/// Maximum canonical-JSON bytes of a tool input that feed the digest.
const MAX_SIGNATURE_INPUT_BYTES: usize = 4096;

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

/// Canonical JSON string of the whole tool input, truncated to the first
/// 4096 bytes on a char boundary so huge inputs stay bounded.
fn canonical_tool_input(input: &serde_json::Value) -> String {
    let text = serde_json::to_string(input).unwrap_or_else(|_| input.to_string());
    truncate_on_char_boundary(&text, MAX_SIGNATURE_INPUT_BYTES)
}

/// Truncate `text` to at most `max_bytes`, never splitting a char.
fn truncate_on_char_boundary(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Bounded summary of the top-level fields that identify "the same call"
/// without storing raw tool output: the string values of
/// `command`/`file_path`/`path`/`pattern`/`url`, whitespace-collapsed,
/// truncated to the first 256 chars. Used only when the payload has no
/// `tool_input`.
pub(crate) fn normalised_summary(payload: &serde_json::Value) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for key in SUMMARY_KEYS {
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

/// `<tool_name>:<16-hex FNV-1a-64 of the canonical tool input>`.
///
/// Stable for identical calls and different for different inputs; `None` when
/// the payload carries no tool name. Payloads without a `tool_input` digest
/// the bounded top-level key summary instead. The raw input is never stored,
/// only the digest.
pub(crate) fn activity_signature(payload: &serde_json::Value) -> Option<String> {
    let tool_name = tool_name_from_payload(payload)?;
    let digest_source = match payload.get("tool_input") {
        Some(input) if !input.is_null() => canonical_tool_input(input),
        _ => normalised_summary(payload),
    };
    Some(format!("{tool_name}:{}", fnv1a64_hex(&digest_source)))
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
