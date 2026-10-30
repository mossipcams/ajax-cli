#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use ajax_core::adapters::{parse_model_selection, ModelSelection};
use serde_json::{json, Value};

use super::agent_event::NativeCommand;
use super::cursor_sdk_map::map_cursor_message;
use super::jsonl_process::JsonlProcess;
use super::rpc_client::RpcClientCore;
use super::rpc_handshake::{
    await_init_response, check_resumed_session, init_commands, init_session_id, RpcHandshake,
    RpcModel,
};
use super::rpc_session::RpcSession;
use super::session_client::ADOPTED_PROMPT_REQUEST_ID;

const SIDECAR_SOURCE: &str = include_str!("../../../sidecar/cursor_sdk_sidecar.mjs");

pub fn sidecar_launch_args(sdk_module: Option<&str>) -> Vec<String> {
    super::rpc_handshake::sidecar_launch_args(SIDECAR_SOURCE, sdk_module)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorCatalogModel {
    pub id: String,
    pub label: String,
    pub efforts: Vec<String>,
    pub has_fast: bool,
}

struct CursorParam {
    id: String,
    name: String,
    values: Vec<(String, String)>,
}

struct CursorModel {
    id: String,
    name: String,
    parameters: Vec<CursorParam>,
    defaults: Vec<(String, String)>,
}

struct CursorState {
    models: Vec<CursorModel>,
    selection: ModelSelection,
    mode: String,
}

pub const MODES: [&str; 2] = ["agent", "plan"];

fn parse_models(data: &Value) -> Vec<CursorModel> {
    let str_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    data.get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = str_of(entry, "id")?;
            let parameters = entry
                .get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|param| {
                    let pid = str_of(param, "id")?;
                    let values = param
                        .get("values")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|v| {
                            let value = str_of(v, "value")?;
                            let display = str_of(v, "displayName").unwrap_or_else(|| value.clone());
                            Some((value, display))
                        })
                        .collect();
                    Some(CursorParam {
                        name: str_of(param, "displayName").unwrap_or_else(|| pid.clone()),
                        id: pid,
                        values,
                    })
                })
                .collect();
            let variants = entry.get("variants").and_then(Value::as_array);
            let default_variant = variants.and_then(|all| {
                all.iter()
                    .find(|v| v.get("isDefault").and_then(Value::as_bool) == Some(true))
                    .or_else(|| all.first())
            });
            let defaults = default_variant
                .and_then(|v| v.get("params"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|p| Some((str_of(p, "id")?, str_of(p, "value")?)))
                .collect();
            Some(CursorModel {
                name: str_of(entry, "displayName").unwrap_or_else(|| id.clone()),
                id,
                parameters,
                defaults,
            })
        })
        .collect()
}

fn selection_json(selection: &ModelSelection) -> Value {
    let params: Vec<Value> = selection
        .options
        .iter()
        .map(|(id, value)| json!({ "id": id, "value": value }))
        .collect();
    json!({ "id": selection.model, "params": params })
}
pub struct CursorSdkClient {
    core: RpcClientCore,
    state: Box<Mutex<CursorState>>,
}

impl CursorSdkClient {
    pub(crate) fn adopt(process: JsonlProcess, session_id: String) -> Self {
        let session = RpcSession::adopt(map_cursor_message, process, session_id);
        let core = RpcClientCore::adopt(session, ADOPTED_PROMPT_REQUEST_ID, None);
        CursorSdkClient {
            core,
            state: Box::new(Mutex::new(CursorState {
                models: Vec::new(),
                selection: ModelSelection {
                    model: "default".to_string(),
                    options: Vec::new(),
                },
                mode: "agent".to_string(),
            })),
        }
    }

    pub fn spawn(
        program: &Path,
        program_args: &[String],
        cwd: &Path,
        model: Option<&str>,
        resume: Option<&str>,
        init_timeout: Duration,
    ) -> Result<CursorSdkClient, String> {
        let selection = model
            .and_then(parse_model_selection)
            .unwrap_or_else(|| ModelSelection {
                model: "default".to_string(),
                options: Vec::new(),
            });
        let mut process = JsonlProcess::spawn(program, program_args, cwd)
            .map_err(|e| format!("failed to spawn cursor sdk sidecar: {e}"))?;
        let mut fields = json!({
            "cwd": cwd.to_string_lossy(),
            "model": selection_json(&selection),
        });
        if let Some(resume) = resume {
            fields["resume"] = json!(resume);
        }
        let init_id = process.send("init", fields)?;
        let (data, pending) =
            await_init_response(&mut process, &init_id, init_timeout, "cursor sdk")?;
        let models = parse_models(&data);
        let session_id = init_session_id(&data, "cursor sdk")?;
        check_resumed_session("cursor sdk", resume, &session_id)?;
        let handshake = RpcHandshake {
            session_id,
            session_file: None,
            model_id: Some(selection.model.clone()),
            thinking_level: None,
            models: data
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    let id = entry.get("id")?.as_str()?;
                    Some(RpcModel {
                        id: id.to_string(),
                        name: entry
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(id)
                            .to_string(),
                        provider: "cursor".to_string(),
                        reasoning: false,
                    })
                })
                .collect(),
            thinking_levels: Vec::new(),
            commands: init_commands(&data),
            context_window: None,
            pending,
        };
        let commands: Vec<NativeCommand> = handshake
            .commands
            .iter()
            .map(NativeCommand::from_rpc_command)
            .collect();
        let session = RpcSession::new(process, handshake, map_cursor_message);
        let core = RpcClientCore::new(session, None);
        core.set_available_commands(commands);

        let apply_defaults = selection.options.is_empty()
            && models
                .iter()
                .any(|m| m.id == selection.model && !m.defaults.is_empty());
        let client = CursorSdkClient {
            core,
            state: Box::new(Mutex::new(CursorState {
                models,
                selection: selection.clone(),
                mode: "agent".to_string(),
            })),
        };
        if apply_defaults {
            client.set_model(&selection.model)?;
        }
        Ok(client)
    }

    pub(super) fn core(&self) -> &RpcClientCore {
        &self.core
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, CursorState> {
        self.state.lock().expect("cursor_sdk_client mutex poisoned")
    }

    pub fn applied_model(&self) -> String {
        self.lock().selection.encode()
    }

    fn send_selection(&self, selection: ModelSelection) -> Result<(), String> {
        self.core.with_session(|s| {
            s.request(
                "set_model",
                json!({ "model": selection_json(&selection) }),
                Duration::from_secs(10),
            )
        })??;
        self.lock().selection = selection;
        Ok(())
    }

    pub fn set_model(&self, value: &str) -> Result<(), String> {
        let mut selection =
            parse_model_selection(value).ok_or_else(|| format!("invalid model: {value}"))?;
        {
            let state = self.lock();
            match state.models.iter().find(|m| m.id == selection.model) {
                Some(model) if selection.options.is_empty() => {
                    selection.options = model.defaults.clone();
                }
                Some(_) => {}
                None if state.models.is_empty() => {}
                None => return Err(format!("unknown model: {}", selection.model)),
            }
        }
        self.send_selection(selection)
    }

    pub fn set_option(&self, id: &str, value: &str) -> Result<(), String> {
        let selection = {
            let state = self.lock();
            let model = state
                .models
                .iter()
                .find(|m| m.id == state.selection.model)
                .ok_or_else(|| format!("unsupported Cursor option {id}"))?;
            let param = model
                .parameters
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("unsupported Cursor option {id}"))?;
            if !param.values.iter().any(|(v, _)| v == value) {
                return Err(format!("unknown {id} value: {value}"));
            }
            let mut selection = state.selection.clone();
            match selection.options.iter_mut().find(|(k, _)| k == id) {
                Some(entry) => entry.1 = value.to_string(),
                None => selection.options.push((id.to_string(), value.to_string())),
            }
            selection
        };
        self.send_selection(selection)
    }

    pub fn set_mode(&self, mode: &str) -> Result<(), String> {
        if !MODES.contains(&mode) {
            return Err(format!("unknown mode: {mode}"));
        }
        self.core.with_session(|s| {
            s.request("set_mode", json!({ "mode": mode }), Duration::from_secs(10))
        })??;
        self.lock().mode = mode.to_string();
        Ok(())
    }

    pub fn config_options(&self) -> Vec<super::NeutralConfigOption> {
        let state = self.lock();
        let select =
            |id: &str, name: &str, category, current: String, choices: Vec<(String, String)>| {
                super::NeutralConfigOption {
                    id: id.to_string(),
                    name: name.to_string(),
                    description: None,
                    category: Some(category),
                    kind: super::NeutralConfigKind::Select {
                        current_value: current,
                        choices: choices
                            .into_iter()
                            .map(|(value, name)| super::NeutralConfigChoice {
                                value,
                                name,
                                description: None,
                            })
                            .collect(),
                    },
                }
            };
        let (selection, models, mode) = (&state.selection, &state.models, &state.mode);
        let model_choices = |m: &CursorModel| (m.id.clone(), m.name.clone());
        let mut options = Vec::new();
        if !models.is_empty() {
            let choices: Vec<(String, String)> = models.iter().map(model_choices).collect();
            let current = selection.model.clone();
            let category = super::NeutralConfigCategory::Model;
            options.push(select("model", "Model", category, current, choices));
        }
        if let Some(model) = models.iter().find(|m| m.id == selection.model) {
            for param in &model.parameters {
                let first: Option<String> = param.values.first().map(|(v, _)| v.clone());
                let sel = selection.options.iter().find(|(k, _)| k == &param.id);
                let current = sel.map(|(_, v)| v.clone()).or(first).unwrap_or_default();
                let category = if param.id == "effort" {
                    super::NeutralConfigCategory::ThoughtLevel
                } else {
                    super::NeutralConfigCategory::Other("model_config".to_string())
                };
                options.push(select(
                    &param.id,
                    &param.name,
                    category,
                    current,
                    param.values.clone(),
                ));
            }
        }
        let category = super::NeutralConfigCategory::Other("mode".to_string());
        let mode_choices: Vec<(String, String)> = MODES
            .iter()
            .map(|m| (m.to_string(), m.to_string()))
            .collect();
        options.push(select("mode", "Mode", category, mode.clone(), mode_choices));
        options
    }

    pub fn model_catalog(&self) -> Vec<CursorCatalogModel> {
        let state = self.lock();
        state
            .models
            .iter()
            .map(|model| CursorCatalogModel {
                id: if model.id == "default" {
                    "auto".to_string()
                } else {
                    model.id.clone()
                },
                label: model.name.clone(),
                efforts: model
                    .parameters
                    .iter()
                    .find(|p| p.id == "effort")
                    .map(|p| p.values.iter().map(|(v, _)| v.clone()).collect())
                    .unwrap_or_default(),
                has_fast: model.parameters.iter().any(|p| p.id == "fast"),
            })
            .collect()
    }
}

#[cfg(test)]
mod adopt_tests {
    use std::path::Path;

    use ajax_core::models::AgentClient;
    use serde_json::json;

    use super::super::cursor_sdk_spawn::with_test_cursor_sdk_module;
    use super::super::session_client::assert_adopted_turn_finishes;

    #[test]
    fn adopted_cursor_turn_finishes_without_a_prompt_or_handshake() {
        let finish = json!({"type": "result", "status": "finished"});
        with_test_cursor_sdk_module(Path::new("cursor_sdk_sidecar.mjs"), || {
            assert_adopted_turn_finishes(AgentClient::Cursor, &finish.to_string(), "");
        });
    }
}
