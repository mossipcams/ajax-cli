//! Signature and success evidence for canonical activity events.
//!
//! The signature is a stable, bounded digest of a tool call's input so the
//! agent watcher can detect repeated calls. It is the tool name plus an
//! FNV-1a-64 digest of the canonical form of the whole `tool_input` value:
//! the value is walked recursively and object entries are fed to the hasher
//! in sorted key order (arrays in order, type-tagged scalars, unambiguous
//! separators, and length-prefixed strings), so the digest is independent of
//! JSON key order and never truncates the input. Payloads without a
//! `tool_input` fall back to a bounded summary of the top-level keys. Raw
//! command text, tool output, and transcripts are never stored in the event
//! envelope.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const SUMMARY_LIMIT: usize = 256;
const SUMMARY_KEYS: [&str; 5] = ["command", "file_path", "path", "pattern", "url"];
/// Streaming FNV-1a-64 hasher.
struct Fnv1a64 {
    hash: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            hash: FNV_OFFSET_BASIS,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(FNV_PRIME);
        }
    }

    /// 16-hex lowercase digest.
    fn hex(self) -> String {
        format!("{:016x}", self.hash)
    }
}

/// 16-hex lowercase FNV-1a-64 digest of `input`.
pub(crate) fn fnv1a64_hex(input: &str) -> String {
    let mut hasher = Fnv1a64::new();
    hasher.update(input.as_bytes());
    hasher.hex()
}

/// Feed the canonical form of `value` to `hasher`: objects in sorted key
/// order, arrays in order, type-tagged scalars, length-prefixed strings.
fn hash_canonical_value(hasher: &mut Fnv1a64, value: &serde_json::Value) {
    match value {
        serde_json::Value::Null => hasher.update(b"n"),
        serde_json::Value::Bool(flag) => hasher.update(if *flag { b"t" } else { b"f" }),
        serde_json::Value::Number(number) => {
            hasher.update(b"d");
            hasher.update(number.to_string().as_bytes());
        }
        serde_json::Value::String(text) => {
            hasher.update(b"s");
            hasher.update((text.len() as u64).to_le_bytes().as_slice());
            hasher.update(text.as_bytes());
        }
        serde_json::Value::Array(items) => {
            hasher.update(b"[");
            for item in items {
                hash_canonical_value(hasher, item);
                hasher.update(b",");
            }
            hasher.update(b"]");
        }
        serde_json::Value::Object(map) => {
            hasher.update(b"{");
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                hasher.update(b"s");
                hasher.update((key.len() as u64).to_le_bytes().as_slice());
                hasher.update(key.as_bytes());
                hasher.update(b":");
                hash_canonical_value(hasher, &map[key]);
                hasher.update(b",");
            }
            hasher.update(b"}");
        }
    }
}

fn string_at<'a>(value: Option<&'a serde_json::Value>, key: &str) -> Option<&'a str> {
    value
        .and_then(|item| item.get(key))
        .and_then(|item| item.as_str())
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
    let digest = match payload.get("tool_input") {
        Some(input) if !input.is_null() => {
            let mut hasher = Fnv1a64::new();
            hash_canonical_value(&mut hasher, input);
            hasher.hex()
        }
        _ => fnv1a64_hex(&normalised_summary(payload)),
    };
    Some(format!("{tool_name}:{digest}"))
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
