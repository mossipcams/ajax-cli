//! Per-turn token usage from ACP `session/prompt` results and deduplication.
//!
//! Context-window pressure (`usage_update` → `usage`) stays separate so operators
//! are not double-counted when a harness reports both shapes.

use super::SessionServerEvent;
use serde_json::Value;
use std::collections::HashSet;

#[derive(Default)]
pub(crate) struct UsageDeduper {
    seen: HashSet<String>,
}

impl UsageDeduper {
    pub(crate) fn should_emit(&mut self, key: &str) -> bool {
        if key.is_empty() {
            return true;
        }
        self.seen.insert(key.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NormalizedTurnUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

/// Parse a provider usage object from `session/prompt` result.usage.
pub(crate) fn parse_turn_usage(raw: &Value) -> Option<NormalizedTurnUsage> {
    let usage = raw.as_object()?;
    let mut normalized = NormalizedTurnUsage {
        input_tokens: read_present_u64(raw, &["inputTokens", "input_tokens"]),
        output_tokens: read_present_u64(raw, &["outputTokens", "output_tokens"]),
        cache_read_tokens: read_present_u64(
            raw,
            &[
                "cacheReadTokens",
                "cachedReadTokens",
                "cache_read_tokens",
                "cached_read_tokens",
            ],
        ),
        cache_write_tokens: read_present_u64(
            raw,
            &[
                "cacheWriteTokens",
                "cachedWriteTokens",
                "cache_write_tokens",
                "cached_write_tokens",
            ],
        ),
        total_tokens: read_present_u64(raw, &["totalTokens", "total_tokens"]),
    };
    if normalized.total_tokens.is_none() {
        normalized.total_tokens = computed_total(&normalized);
    }
    if normalized.is_empty() && usage.is_empty() {
        return None;
    }
    normalized.has_any_field().then_some(normalized)
}

pub(crate) fn turn_usage_dedup_key(raw: &Value, request_id: Option<u64>) -> String {
    for key in [
        "requestId",
        "request_id",
        "generationId",
        "generation_id",
        "turnId",
        "turn_id",
    ] {
        if let Some(id) = raw
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
        {
            return id.to_string();
        }
    }
    request_id.map(|id| id.to_string()).unwrap_or_default()
}

pub(crate) fn map_prompt_result_usage(
    result: &Value,
    request_id: Option<u64>,
    deduper: &mut UsageDeduper,
) -> Option<SessionServerEvent> {
    let usage_body = result.get("usage")?;
    let normalized = parse_turn_usage(usage_body)?;
    let dedup_key = turn_usage_dedup_key(usage_body, request_id);
    if !deduper.should_emit(&dedup_key) {
        return None;
    }
    Some(turn_usage_event(
        normalized,
        (!dedup_key.is_empty()).then_some(dedup_key),
    ))
}

pub(crate) fn turn_usage_event(
    usage: NormalizedTurnUsage,
    request_id: Option<String>,
) -> SessionServerEvent {
    SessionServerEvent::TurnUsage {
        request_id,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        total_tokens: usage.total_tokens,
    }
}

impl NormalizedTurnUsage {
    fn has_any_field(&self) -> bool {
        self.input_tokens.is_some()
            || self.output_tokens.is_some()
            || self.cache_read_tokens.is_some()
            || self.cache_write_tokens.is_some()
            || self.total_tokens.is_some()
    }

    fn is_empty(&self) -> bool {
        !self.has_any_field()
    }
}

fn computed_total(usage: &NormalizedTurnUsage) -> Option<u64> {
    let parts = [
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens,
        usage.cache_write_tokens,
    ];
    let present: Vec<u64> = parts.into_iter().flatten().collect();
    if present.is_empty() {
        None
    } else {
        Some(present.iter().sum())
    }
}

fn read_present_u64(obj: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        obj.get(*key).and_then(|value| {
            if value.is_null() {
                None
            } else {
                parse_u64(value)
            }
        })
    })
}

fn parse_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
}
