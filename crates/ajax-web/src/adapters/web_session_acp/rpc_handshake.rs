#![allow(dead_code)]

use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::jsonl_process::{JsonlProcess, JsonlRecord};
#[derive(Debug)]
pub struct RpcHandshake {
    pub session_id: String,
    pub session_file: Option<String>,
    pub model_id: Option<String>,
    pub thinking_level: Option<String>,
    pub models: Vec<RpcModel>,
    pub thinking_levels: Vec<String>,
    pub commands: Vec<RpcCommand>,
    pub context_window: Option<u64>,
    pub pending: Vec<Value>,
}

#[derive(Debug)]
pub struct RpcModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub reasoning: bool,
}

#[derive(Debug)]
pub struct RpcCommand {
    pub name: String,
    pub description: Option<String>,
}

pub fn await_init_response(
    process: &mut JsonlProcess,
    init_id: &str,
    timeout: Duration,
    label: &str,
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

pub fn init_session_id(data: &Value, label: &str) -> Result<String, String> {
    Ok(data
        .get("sessionId")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} init failed: missing sessionId"))?
        .to_string())
}

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
