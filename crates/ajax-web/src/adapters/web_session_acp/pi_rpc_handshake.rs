//! The Pi RPC handshake: the four state-discovery commands issued to a freshly
//! spawned [`PiRpcProcess`] and their id-correlated responses, typed into
//! [`PiHandshake`]. This module is pure transport-plus-parsing on top of the
//! `pi_rpc_process` surface (std only, no tokio): it writes all four commands
//! before waiting for any answer, then drains stdout records until every
//! expected response has arrived or the deadline passes. It does not wire into
//! the live session path and does not interpret event semantics beyond
//! capturing non-response records as pending replay material.
//!
//! Correlation rules: each response is a `{"type":"response","id":…}` record
//! echoing the id [`PiRpcProcess::send`] allocated for its command, so the four
//! replies are matched by id, never by arrival order. Any record that is not
//! one of those four responses (events such as `extension_ui_request`, other
//! in-flight responses) is preserved, in arrival order, in
//! [`PiHandshake::pending`] for the caller to replay — none are dropped.
//!
//! Failure policy: a failing or missing `get_state` (including an absent
//! `data.sessionId`) fails the whole handshake; failures of the other three
//! commands degrade to empty lists so a partial discovery state still yields a
//! usable [`PiHandshake`]. A timeout, non-JSON stdout line, or child exit
//! *before* `get_state` completes returns an error that carries the process
//! stderr tail for diagnostics.

// Not yet reachable from the crate surface: nothing wires this in until the
// connection-loop task lands, so every public item is dead code for now.
#![allow(dead_code)]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::pi_rpc_process::{PiRpcProcess, PiRpcRecord};

/// How long each `try_recv` gap sleeps before re-polling the record channel.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// The four discovery commands and the handshake response that completes them.
#[derive(Debug)]
pub struct PiHandshake {
    /// `get_state.data.sessionId`; required, its absence fails the handshake.
    pub session_id: String,
    /// `get_state.data.sessionFile`, when pi reports a persisted session path.
    pub session_file: Option<String>,
    /// `get_state.data.model.id`, when a model is selected.
    pub model_id: Option<String>,
    /// `get_state.data.thinkingLevel`, when present.
    pub thinking_level: Option<String>,
    /// `get_available_models.data.models`, empty on failure or degradation.
    pub models: Vec<PiModel>,
    /// `get_available_thinking_levels.data.levels`.
    pub thinking_levels: Vec<String>,
    /// `get_commands.data.commands`.
    pub commands: Vec<PiCommand>,
    /// `get_state.data.model.contextWindow`, when a model is selected.
    pub context_window: Option<u64>,
    /// Every non-handshake record (events, other responses) captured in order
    /// while the handshake ran; the caller replays these.
    pub pending: Vec<Value>,
}

/// One entry of `get_available_models.data.models`.
#[derive(Debug)]
pub struct PiModel {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub reasoning: bool,
}

/// One entry of `get_commands.data.commands`.
#[derive(Debug)]
pub struct PiCommand {
    pub name: String,
    pub description: Option<String>,
}

/// Issue the four discovery commands and collect their responses.
///
/// All four commands are written to stdin (via [`PiRpcProcess::send`]) before
/// any record is read, then records are consumed until all four matching-id
/// responses have arrived or `timeout` (measured from the first send) elapses.
/// See the module docs for the correlation and failure policy.
pub fn handshake(process: &mut PiRpcProcess, timeout: Duration) -> Result<PiHandshake, String> {
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
                PiRpcRecord::Record(value) => {
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

                    // Anything that is not one of the four expected responses —
                    // events, extension_ui_request, other in-flight responses —
                    // is preserved in arrival order for the caller to replay.
                    if !is_expected_response {
                        pending.push(value);
                        continue;
                    }

                    match id {
                        s if s == state_id && state.is_none() => state = Some(value),
                        s if s == models_id && models.is_none() => models = Some(value),
                        s if s == levels_id && levels.is_none() => levels = Some(value),
                        // A second response for an already-seen command cannot
                        // be one of the four (ids are fresh per process); treat
                        // any such duplicate as pending replay material.
                        s if s == commands_id && commands.is_none() => commands = Some(value),
                        _ => pending.push(value),
                    }
                }
                PiRpcRecord::Error(text) => {
                    if state.is_none() {
                        return Err(format!(
                            "pi child emitted a non-JSON stdout line before get_state \
                             completed: {text} (stderr tail: {})",
                            process.stderr_tail()
                        ));
                    }
                    // After get_state the stream is replay material, not fatal;
                    // preserve the raw line rather than dropping it.
                    pending.push(Value::String(text));
                }
                PiRpcRecord::Exited => {
                    if state.is_none() {
                        return Err(format!(
                            "pi child exited before get_state completed (stderr tail: {})",
                            process.stderr_tail()
                        ));
                    }
                    // No more records can arrive; the remaining discovery lists
                    // degrade to empty below.
                    break;
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
                    // get_state is already in hand; whatever discovery response
                    // never arrived degrades to an empty list.
                    break;
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                // The reader thread only disconnects after delivering Exited,
                // which the arm above has already handled (or exited on).
                if state.is_none() {
                    return Err(format!(
                        "pi child stdout closed before get_state completed \
                         (stderr tail: {})",
                        process.stderr_tail()
                    ));
                }
                break;
            }
        }
    }

    let state = state.ok_or("get_state response was not received".to_owned())?;

    // A failing get_state fails the handshake with pi's error text.
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
    Ok(PiHandshake {
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
    })
}

/// Read a string field from `get_state`'s data object.
fn string_field(data: Option<&Value>, key: &str) -> Option<String> {
    data.and_then(|d| d.get(key))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Parse `get_available_models`'s response into typed model entries. A failed,
/// missing, or malformed response degrades to an empty list; individual
/// entries lacking the required fields are skipped.
fn parse_models(response: Option<&Value>) -> Vec<PiModel> {
    let Some(array) = success_data_list(response, "models") else {
        return Vec::new();
    };
    array.iter().filter_map(parse_model).collect()
}

/// Parse one `data.models` entry.
fn parse_model(value: &Value) -> Option<PiModel> {
    let id = value.get("id")?.as_str()?.to_owned();
    Some(PiModel {
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

/// Parse `get_available_thinking_levels`'s `data.levels`.
fn parse_thinking_levels(response: Option<&Value>) -> Vec<String> {
    let Some(array) = success_data_list(response, "levels") else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|level| level.as_str().map(str::to_owned))
        .collect()
}

/// Parse `get_commands`'s `data.commands`; entries without a name are skipped.
fn parse_commands(response: Option<&Value>) -> Vec<PiCommand> {
    let Some(array) = success_data_list(response, "commands") else {
        return Vec::new();
    };
    array.iter().filter_map(parse_command).collect()
}

/// Parse one `data.commands` entry.
fn parse_command(value: &Value) -> Option<PiCommand> {
    let name = value.get("name")?.as_str()?.to_owned();
    Some(PiCommand {
        name,
        description: value
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

/// The `data.<key>` array of a successful response; `None` when the response
/// is absent, failed, or malformed. This is what turns a failed optional
/// discovery command into an empty list instead of an error.
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
