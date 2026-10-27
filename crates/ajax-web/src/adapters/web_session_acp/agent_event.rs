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
