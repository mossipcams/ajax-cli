#![allow(dead_code)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::jsonl_process::{JsonlProcess, JsonlRecord};
use super::rpc_handshake::{RpcCommand, RpcHandshake, RpcModel};

const POLL_INTERVAL: Duration = Duration::from_millis(5);

pub fn handshake(process: &mut JsonlProcess, timeout: Duration) -> Result<RpcHandshake, String> {
    handshake_with_model_provider(process, timeout).map(|(handshake, _)| handshake)
}

pub fn handshake_with_model_provider(
    process: &mut JsonlProcess,
    timeout: Duration,
) -> Result<(RpcHandshake, Option<String>), String> {
    let state_id = process.send("get_state", json!({}))?;
    let models_id = process.send("get_available_models", json!({}))?;
    let levels_id = process.send("get_available_thinking_levels", json!({}))?;
    let commands_id = process.send("get_commands", json!({}))?;

    let deadline = Instant::now() + timeout;
    let mut state: Option<Value> = None;
    let mut models: Option<Value> = None;
    let mut levels: Option<Value> = None;
    let mut commands: Option<Value> = None;
    let mut pending: Vec<Value> = Vec::new();

    loop {
        if state.is_some() && models.is_some() && levels.is_some() && commands.is_some() {
            break;
        }

        match process.try_recv() {
            Ok(record) => match record {
                JsonlRecord::Record(value) => {
                    let id = value
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    let is_expected_response = value.get("type").and_then(Value::as_str)
                        == Some("response")
                        && [
                            state_id.as_str(),
                            models_id.as_str(),
                            levels_id.as_str(),
                            commands_id.as_str(),
                        ]
                        .contains(&id.as_str());

                    if !is_expected_response {
                        pending.push(value);
                        continue;
                    }

                    match id {
                        s if s == state_id && state.is_none() => state = Some(value),
                        s if s == models_id && models.is_none() => models = Some(value),
                        s if s == levels_id && levels.is_none() => levels = Some(value),
                        s if s == commands_id && commands.is_none() => commands = Some(value),
                        _ => pending.push(value),
                    }
                }
                JsonlRecord::Error(text) => {
                    if state.is_none() {
                        return Err(format!(
                            "pi child emitted a non-JSON stdout line before get_state \
                             completed: {text} (stderr tail: {})",
                            process.stderr_tail()
                        ));
                    }
                    pending.push(Value::String(text));
                }
                JsonlRecord::Exited => {
                    return Err(format!(
                        "pi child exited before the handshake completed (get_state received: {}, stderr tail: {})",
                        state.is_some(),
                        process.stderr_tail()
                    ));
                }
            },
            Err(mpsc::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    if state.is_none() {
                        return Err(format!(
                            "timed out after {timeout:?} waiting for get_state (stderr tail: {})",
                            process.stderr_tail()
                        ));
                    }
                    break;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                return Err(format!(
                    "pi child stdout closed before the handshake completed \
                     (get_state received: {}, stderr tail: {})",
                    state.is_some(),
                    process.stderr_tail()
                ));
            }
        }
    }

    let state = state.ok_or("get_state response was not received".to_owned())?;

    if state.get("success").and_then(Value::as_bool) != Some(true) {
        let detail = state
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| "no error field in the get_state response".to_owned());
        return Err(format!("get_state failed: {detail}"));
    }

    let data = state.get("data");
    let session_id = data
        .and_then(|d| d.get("sessionId"))
        .and_then(Value::as_str)
        .ok_or_else(|| "get_state response is missing data.sessionId".to_owned())?;

    let model = data.and_then(|d| d.get("model"));
    let model_provider = model
        .and_then(|m| m.get("provider"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let handshake = RpcHandshake {
        session_id: session_id.to_owned(),
        session_file: string_field(data, "sessionFile"),
        model_id: model
            .and_then(|m| m.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        thinking_level: string_field(data, "thinkingLevel"),
        models: parse_models(models.as_ref()),
        thinking_levels: parse_thinking_levels(levels.as_ref()),
        commands: parse_commands(commands.as_ref()),
        context_window: model
            .and_then(|m| m.get("contextWindow"))
            .and_then(Value::as_u64),
        pending,
    };
    Ok((handshake, model_provider))
}

fn string_field(data: Option<&Value>, key: &str) -> Option<String> {
    data.and_then(|d| d.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_models(response: Option<&Value>) -> Vec<RpcModel> {
    let Some(array) = success_data_list(response, "models") else {
        return Vec::new();
    };
    array.iter().filter_map(parse_model).collect()
}

fn parse_model(value: &Value) -> Option<RpcModel> {
    let id = value.get("id")?.as_str()?.to_owned();
    Some(RpcModel {
        id,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        provider: value
            .get("provider")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        reasoning: value
            .get("reasoning")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

fn parse_thinking_levels(response: Option<&Value>) -> Vec<String> {
    let Some(array) = success_data_list(response, "levels") else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|level| level.as_str().map(str::to_owned))
        .collect()
}

fn parse_commands(response: Option<&Value>) -> Vec<RpcCommand> {
    let Some(array) = success_data_list(response, "commands") else {
        return Vec::new();
    };
    array.iter().filter_map(parse_command).collect()
}

fn parse_command(value: &Value) -> Option<RpcCommand> {
    let name = value.get("name")?.as_str()?.to_owned();
    Some(RpcCommand {
        name,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn success_data_list<'a>(response: Option<&'a Value>, key: &str) -> Option<&'a Vec<Value>> {
    let response = response?;
    if response.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    response
        .get("data")
        .and_then(|d| d.get(key))
        .and_then(Value::as_array)
}
