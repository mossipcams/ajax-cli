use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    AvailableCommandDescriptor, ConfigOptionDescriptor, PromptCapabilityDescriptor,
};
use serde::{Deserialize, Serialize};

pub const SESSION_PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionChrome {
    pub session_config_options: Option<Vec<ConfigOptionDescriptor>>,
    pub available_commands: Option<Vec<AvailableCommandDescriptor>>,
    pub prompt_capabilities: Option<PromptCapabilityDescriptor>,
    pub session_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingPermission {
    #[serde(rename = "requestId")]
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingElicitation {
    #[serde(rename = "requestId")]
    pub request_id: String,
    pub message: String,
    pub schema: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    pub cursor: usize,
    pub model: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "sessionConfigOptions"
    )]
    pub session_config_options: Option<Vec<ConfigOptionDescriptor>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "availableCommands"
    )]
    pub available_commands: Option<Vec<AvailableCommandDescriptor>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "promptCapabilities"
    )]
    pub prompt_capabilities: Option<PromptCapabilityDescriptor>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "sessionTitle"
    )]
    pub session_title: Option<String>,
    #[serde(rename = "turnState")]
    pub turn_state: String,
    pub reset: bool,
    #[serde(
        default,
        rename = "agentPending",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub agent_pending: bool,
    #[serde(rename = "pendingPermission", skip_serializing_if = "Option::is_none")]
    pub pending_permission: Option<PendingPermission>,
    #[serde(rename = "pendingElicitation", skip_serializing_if = "Option::is_none")]
    pub pending_elicitation: Option<PendingElicitation>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transcriptError"
    )]
    pub transcript_error: Option<String>,
    #[serde(
        default,
        rename = "promptsBlocked",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub prompts_blocked: bool,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transcript"
    )]
    pub transcript: Option<Vec<SessionEventEnvelope>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transcriptFrom"
    )]
    pub transcript_from: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transcriptFloor"
    )]
    pub transcript_floor: Option<usize>,
}

impl SessionSnapshot {
    pub fn new(
        cursor: usize,
        model: String,
        busy: bool,
        reset: bool,
        pending_permission: Option<PendingPermission>,
        pending_elicitation: Option<PendingElicitation>,
        chrome: SessionChrome,
    ) -> Self {
        Self {
            kind: "snapshot".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            cursor,
            model,
            session_config_options: chrome.session_config_options,
            available_commands: chrome.available_commands,
            prompt_capabilities: chrome.prompt_capabilities,
            session_title: chrome.session_title,
            turn_state: if busy { "busy" } else { "idle" }.to_string(),
            reset,
            agent_pending: false,
            pending_permission,
            pending_elicitation,
            transcript_error: None,
            prompts_blocked: false,
            transcript: None,
            transcript_from: None,
            transcript_floor: None,
        }
    }

    pub fn with_transcript_error(mut self, error: Option<String>) -> Self {
        self.transcript_error = error;
        self
    }

    pub fn with_prompts_blocked(mut self, blocked: bool) -> Self {
        self.prompts_blocked = blocked;
        self
    }

    pub fn with_transcript(
        mut self,
        transcript: Vec<SessionEventEnvelope>,
        transcript_from: Option<usize>,
    ) -> Self {
        self.transcript = Some(transcript);
        self.transcript_from = transcript_from;
        self
    }

    pub fn with_transcript_floor(mut self, floor: usize) -> Self {
        self.transcript_floor = Some(floor);
        self
    }

    pub fn with_agent_pending(mut self, pending: bool) -> Self {
        self.agent_pending = pending;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEventEnvelope {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(rename = "protocolVersion")]
    pub protocol_version: u32,
    pub cursor: usize,
    pub payload: SessionServerEvent,
}

impl SessionEventEnvelope {
    pub fn new(cursor: usize, payload: SessionServerEvent) -> Self {
        Self {
            kind: "event".to_string(),
            protocol_version: SESSION_PROTOCOL_VERSION,
            cursor,
            payload,
        }
    }
}

pub fn parse_client_cursor(query: Option<&str>) -> Option<usize> {
    query?.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        if key != "cursor" {
            return None;
        }
        value.parse().ok()
    })
}

pub fn parse_tail_snapshot_flag(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        query
            .split('&')
            .any(|pair| pair.split_once('=') == Some(("snapshotTail", "1")))
    })
}

#[cfg(test)]
mod tests {
    use super::{SessionChrome, SessionSnapshot};

    #[test]
    fn snapshot_agent_pending_serializes_only_when_true() {
        let snapshot = |pending| {
            SessionSnapshot::new(
                5,
                "auto".to_string(),
                false,
                true,
                None,
                None,
                SessionChrome::default(),
            )
            .with_agent_pending(pending)
        };
        let off = serde_json::to_value(snapshot(false)).expect("serialize snapshot");
        assert!(off.get("agentPending").is_none(), "{off}");
        let on = serde_json::to_value(snapshot(true)).expect("serialize snapshot");
        assert_eq!(on.get("agentPending"), Some(&serde_json::json!(true)));
    }
}
