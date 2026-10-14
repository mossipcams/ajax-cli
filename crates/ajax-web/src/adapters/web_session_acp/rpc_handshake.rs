//! Transport-neutral result of the start-up discovery every RPC-style harness
//! transport performs (the Pi RPC child today, the Claude SDK sidecar next).
//! The Pi-specific command exchange that fills it lives in `pi_rpc_handshake`.
#![allow(dead_code)]

use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::jsonl_process::{JsonlProcess, JsonlRecord};
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

/// Wait for the `response` record answering `init_id`, collecting every other
/// record read meanwhile. Returns the response's `data` and those pending records.
pub fn await_init_response(
    process: &mut JsonlProcess,
    init_id: &str,
    timeout: Duration,
    label: &str, // e.g. "claude sdk"
) -> Result<(Value, Vec<Value>), String> {
    let started = Instant::now();
    let mut pending = Vec::new();
    let response = loop {
        if started.elapsed() >= timeout {
            return Err(format!("{label} init timed out: {}", process.stderr_tail()));
        }
        match process.try_recv() {
            Ok(JsonlRecord::Record(record)) => {
                if record.get("type").and_then(Value::as_str) == Some("response")
                    && record.get("id").and_then(Value::as_str) == Some(init_id)
                {
                    break record;
                }
                pending.push(record);
            }
            Ok(JsonlRecord::Error(error)) => {
                return Err(format!(
                    "{label} init failed: {error}; stderr: {}",
                    process.stderr_tail()
                ));
            }
            Ok(JsonlRecord::Exited) | Err(TryRecvError::Disconnected) => {
                return Err(format!(
                    "{label} sidecar exited before init response: {}",
                    process.stderr_tail()
                ));
            }
            Err(TryRecvError::Empty) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    if response.get("success").and_then(Value::as_bool) != Some(true) {
        let error = response
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("missing successful init response");
        return Err(format!("{label} init failed: {error}"));
    }
    Ok((response["data"].clone(), pending))
}

/// Node argv for a sidecar run from an embedded script, optionally pinning the SDK module.
pub fn sidecar_launch_args(source: &str, sdk_module: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "--input-type=module".to_string(),
        "-e".to_string(),
        source.to_string(),
    ];
    if let Some(module) = sdk_module {
        args.extend([
            "--".to_string(),
            "--sdk-module".to_string(),
            module.to_string(),
        ]);
    }
    args
}

/// `data.sessionId` of an init response; `label` is e.g. "claude sdk".
pub fn init_session_id(data: &Value, label: &str) -> Result<String, String> {
    Ok(data
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} init failed: missing sessionId"))?
        .to_string())
}

/// Fail when a resume request came back with a different session id.
pub fn check_resumed_session(
    label: &str,
    resume: Option<&str>,
    session_id: &str,
) -> Result<(), String> {
    if let Some(id) = resume {
        if session_id != id {
            return Err(format!(
                "{label} resumed a different session: asked {id}, got {session_id}"
            ));
        }
    }
    Ok(())
}

/// `data.commands` entries (`name`, optional `description`) as RpcCommand values.
pub fn init_commands(data: &Value) -> Vec<RpcCommand> {
    data.get("commands")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(RpcCommand {
                name: entry.get("name")?.as_str()?.to_string(),
                description: entry
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            })
        })
        .collect()
}
