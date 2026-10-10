//! Transport-neutral result of the start-up discovery every RPC-style harness
//! transport performs (the Pi RPC child today, the Claude SDK sidecar next).
//! The Pi-specific command exchange that fills it lives in `pi_rpc_handshake`.
#![allow(dead_code)]

use serde_json::Value;
/// The four discovery commands and the handshake response that completes them.
#[derive(Debug)]
pub struct RpcHandshake {
    /// `get_state.data.sessionId`; required, its absence fails the handshake.
    pub session_id: String,
    /// `get_state.data.sessionFile`, when the harness reports a persisted session path.
    pub session_file: Option<String>,
    /// `get_state.data.model.id`, when a model is selected.
    pub model_id: Option<String>,
    /// `get_state.data.thinkingLevel`, when present.
    pub thinking_level: Option<String>,
    /// `get_available_models.data.models`, empty on failure or degradation.
    pub models: Vec<RpcModel>,
    /// `get_available_thinking_levels.data.levels`.
    pub thinking_levels: Vec<String>,
    /// `get_commands.data.commands`.
    pub commands: Vec<RpcCommand>,
    /// `get_state.data.model.contextWindow`, when a model is selected.
    pub context_window: Option<u64>,
    /// Every non-handshake record (events, other responses) captured in order
    /// while the handshake ran; the caller replays these.
    pub pending: Vec<Value>,
}

/// One entry of `get_available_models.data.models`.
#[derive(Debug)]
pub struct RpcModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub reasoning: bool,
}

/// One entry of `get_commands.data.commands`.
#[derive(Debug)]
pub struct RpcCommand {
    pub name: String,
    pub description: Option<String>,
}
