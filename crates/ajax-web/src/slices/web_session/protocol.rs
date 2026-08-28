//! Versioned WebSocket envelopes for orchestration chat (protocol v2).

use super::context_continuity::{ContextContinuity, ContextState};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    AvailableCommandDescriptor, ConfigOptionDescriptor, PromptCapabilityDescriptor,
};
use serde::{Deserialize, Serialize};

pub const SESSION_PROTOCOL_VERSION: u32 = 2;

/// Live session chrome grouped for attach/snapshot construction.
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

/// Outstanding permission and elicitation prompts included in attach snapshots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionPending {
    pub permission: Option<PendingPermission>,
    pub elicitation: Option<PendingElicitation>,
}

/// Attach state sent once per logical attach or generation change.
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
    #[serde(rename = "pendingPermission", skip_serializing_if = "Option::is_none")]
    pub pending_permission: Option<PendingPermission>,
    #[serde(rename = "pendingElicitation", skip_serializing_if = "Option::is_none")]
    pub pending_elicitation: Option<PendingElicitation>,
    #[serde(rename = "contextState")]
    pub context_state: ContextState,
    #[serde(rename = "contextEpoch")]
    pub context_epoch: u64,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "contextError"
    )]
    pub context_error: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "transcriptError"
    )]
    pub transcript_error: Option<String>,
}

impl SessionSnapshot {
    pub fn new(
        cursor: usize,
        model: String,
        busy: bool,
        reset: bool,
        pending: SessionPending,
        chrome: SessionChrome,
        continuity: ContextContinuity,
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
            pending_permission: pending.permission,
            pending_elicitation: pending.elicitation,
            context_state: continuity.state,
            context_epoch: continuity.epoch,
            context_error: continuity.error,
            transcript_error: None,
        }
    }

    pub fn with_transcript_error(mut self, error: Option<String>) -> Self {
        self.transcript_error = error;
        self
    }
}

/// One persisted transcript row with its absolute cursor.
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
