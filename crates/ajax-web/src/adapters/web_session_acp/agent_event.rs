use serde_json::Value;

use super::client::AcpClientEvent;

#[derive(Debug, Clone)]
pub enum NativeEvent {
    AgentText(String),
    AgentThought(String),
    ToolCall {
        id: String,
        title: String,
        kind: String,
        status: String,
        raw_input: Option<Value>,
    },
    ToolCallUpdate {
        id: String,
        status: String,
        content: Option<String>,
        raw_input: Option<Value>,
    },
    Plan(Vec<(String, String)>),
    Usage {
        used: u64,
        size: u64,
    },
    TurnFinished {
        request_id: u64,
        result: Result<String, String>,
    },
    Error(String),
    RateLimited {
        message: String,
    },
    Exited,
    ElicitationRequest {
        request_id: String,
        message: String,
        schema: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCommand {
    pub name: String,
    pub description: String,
    pub input_hint: Option<String>,
}

impl NativeCommand {
    pub fn from_rpc_command(command: &super::rpc_handshake::RpcCommand) -> Self {
        Self {
            name: command.name.clone(),
            description: command.description.clone().unwrap_or_default(),
            input_hint: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub enum AgentEvent {
    Acp(AcpClientEvent),
    Native(NativeEvent),
}

const LIMIT_ERROR_NEEDLES: [&str; 10] = [
    "rate limit",
    "rate_limit",
    "ratelimit",
    "too many requests",
    "usage limit",
    "usage_limit",
    "usagelimit",
    "quota exceeded",
    "insufficient_quota",
    "out of usage",
];

pub(crate) fn is_limit_error_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    LIMIT_ERROR_NEEDLES
        .iter()
        .any(|needle| lower.contains(needle))
        || contains_standalone_429(&lower)
}

fn contains_standalone_429(lower: &str) -> bool {
    lower.match_indices("429").any(|(index, _)| {
        let before = lower[..index].chars().next_back();
        let after = lower[index + 3..].chars().next();
        !before.is_some_and(is_identifier_char) && !after.is_some_and(is_identifier_char)
    })
}

fn is_identifier_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

#[cfg(test)]
mod limit_error_tests {
    use super::is_limit_error_text;

    #[test]
    fn limit_error_text_classifies_limits_and_not_overload_or_billing() {
        for text in [
            "Rate limit exceeded",
            "rate_limit_error",
            "RateLimitError: slow down",
            "too many requests",
            "HTTP 429 from upstream",
            "429",
            "usage limit reached",
            "usage_limit",
            "UsageLimitError",
            "quota exceeded for this project",
            "insufficient_quota",
            "You're out of usage credits",
        ] {
            assert!(is_limit_error_text(text), "expected a limit: {text}");
        }
        for text in [
            "550e8400-e29b-41d4-a429-446655440000",
            "550e8400-429-41d4-a716-446655440000",
            "request 4290 failed",
            "529 overloaded",
            "API Error: overloaded",
            "billing: payment method required",
            "connection reset",
        ] {
            assert!(!is_limit_error_text(text), "expected not a limit: {text}");
        }
    }
}
