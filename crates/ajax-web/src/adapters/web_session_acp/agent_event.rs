//! Shared event vocabulary for native (Claude Agent SDK / Pi RPC) adapters and
//! the web session slice. `NativeEvent` carries no `agent_client_protocol`
//! types: in addition to everything `claude_sdk_map.rs` and `pi_rpc_map.rs`
//! currently emit as ACP `SessionUpdate`s, it carries lifecycle events (turn
//! finished, error, exited), so those mappers can stop translating through
//! ACP without changing the wire output.

use serde_json::Value;

use super::client::AcpClientEvent;

/// One unit of native adapter output (Claude Agent SDK / Pi RPC), free of any
/// `agent_client_protocol` types — the equivalent of the ACP `SessionUpdate`s
/// (`agent_message_chunk`, `agent_thought_chunk`, `tool_call`,
/// `tool_call_update`, `plan`, and `usage_update`) the mappers emit today.
#[derive(Debug, Clone)]
pub enum NativeEvent {
    /// Streamed assistant text chunk.
    AgentText(String),
    /// Streamed thinking chunk.
    AgentThought(String),
    /// A tool call opening; `title`, `kind`, and `status` are the wire strings
    /// the ACP mappers produce for it.
    ToolCall {
        id: String,
        title: String,
        kind: String,
        status: String,
        raw_input: Option<Value>,
    },
    /// A status/content update to an already-open tool call; `content` is the
    /// joined text output, if any, and `raw_input` carries the complete tool
    /// input so the mapper can derive locations from it (as ACP does).
    ToolCallUpdate {
        id: String,
        status: String,
        content: Option<String>,
        raw_input: Option<Value>,
    },
    /// Plan entries as `(content, status)` pairs.
    Plan(Vec<(String, String)>),
    /// Turn usage: tokens used against the model context window size.
    Usage { used: u64, size: u64 },
    /// A turn finished; `Ok` carries the stop reason (e.g. `"end_turn"` or
    /// `"cancelled"`) and `Err` carries the rejection text.
    TurnFinished {
        request_id: u64,
        result: Result<String, String>,
    },
    /// An operator-visible adapter error.
    Error(String),
    /// The harness process exited.
    Exited,
}

/// Every event a session client can emit: an ACP-protocol event for harnesses
/// that speak ACP, or a native event from the Claude Agent SDK / Pi RPC
/// adapters. The web session slice maps both to its own wire events.
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// An event from an ACP harness connection.
    Acp(AcpClientEvent),
    /// An event from a native (non-ACP) adapter.
    Native(NativeEvent),
}
