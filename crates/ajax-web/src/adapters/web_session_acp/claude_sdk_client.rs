#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeCommand, NativeEvent};
use super::claude_sdk_map::map_sdk_message;
use super::jsonl_process::JsonlProcess;
use super::rpc_client::RpcClientCore;
use super::rpc_handshake::{
    await_init_response, check_resumed_session, init_commands, init_session_id, RpcHandshake,
    RpcModel,
};
use super::rpc_session::RpcSession;
use super::session_client::ADOPTED_PROMPT_REQUEST_ID;

const SIDECAR_SOURCE: &str = include_str!("../../../sidecar/claude_sdk_sidecar.mjs");

const USAGE_TIMEOUT: Duration = Duration::from_secs(2);

pub const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn sidecar_launch_args(sdk_module: Option<&str>) -> Vec<String> {
    super::rpc_handshake::sidecar_launch_args(SIDECAR_SOURCE, sdk_module)
}
fn claude_usage_hook(session: &mut RpcSession) -> Option<AgentEvent> {
    let data = session
        .request("get_context_usage", json!({}), USAGE_TIMEOUT)
        .ok()?;
    let used = data.get("totalTokens")?.as_u64()?;
    let size = data.get("maxTokens")?.as_u64()?;
    if used == 0 || size == 0 {
        return None;
    }
    Some(AgentEvent::Native(NativeEvent::Usage { used, size }))
}
pub struct ClaudeSdkClient {
    core: RpcClientCore,
    state: Box<Mutex<ClaudeState>>,
}

struct ClaudeState {
    models: Vec<(String, String)>,
    current_model: String,
    current_effort: String,
}

impl ClaudeSdkClient {
    pub fn spawn(
        program: &Path,
        program_args: &[String],
        cwd: &Path,
        model: Option<&str>,
        resume: Option<&str>,
        init_timeout: Duration,
    ) -> Result<ClaudeSdkClient, String> {
        let mut process = JsonlProcess::spawn(program, program_args, cwd)
            .map_err(|e| format!("failed to spawn claude sdk sidecar: {e}"))?;
        let mut fields = json!({ "cwd": cwd.to_string_lossy() });
        if let Some(model) = model {
            fields["model"] = json!(model);
        }
        if let Some(resume) = resume {
            fields["resume"] = json!(resume);
        }
        let init_id = process.send("init", fields)?;
        let (data, pending) =
            await_init_response(&mut process, &init_id, init_timeout, "claude sdk")?;
        let session_id = init_session_id(&data, "claude sdk")?;
        let handshake = RpcHandshake {
            session_id,
            session_file: None,
            model_id: model.map(str::to_string),
            thinking_level: None,
            models: data
                .get("models")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    let value = entry.get("value")?.as_str()?;
                    Some(RpcModel {
                        id: value.to_string(),
                        name: entry
                            .get("displayName")
                            .and_then(Value::as_str)
                            .unwrap_or(value)
                            .to_string(),
                        provider: "claude".to_string(),
                        reasoning: false,
                    })
                })
                .collect(),
            thinking_levels: EFFORT_LEVELS
                .iter()
                .map(|level| level.to_string())
                .collect(),
            commands: init_commands(&data),
            context_window: None,
            pending,
        };
        check_resumed_session("claude sdk", resume, &handshake.session_id)?;
        let state = ClaudeState {
            models: handshake
                .models
                .iter()
                .map(|model| (model.id.clone(), model.name.clone()))
                .collect(),
            current_model: model.unwrap_or("default").to_string(),
            current_effort: "medium".to_string(),
        };
        let commands: Vec<NativeCommand> = handshake
            .commands
            .iter()
            .map(NativeCommand::from_rpc_command)
            .collect();

        let session = RpcSession::new(process, handshake, map_sdk_message);
        let core = RpcClientCore::new(session, Some(claude_usage_hook));
        core.set_available_commands(commands);

        Ok(ClaudeSdkClient {
            core,
            state: Box::new(Mutex::new(state)),
        })
    }

    pub(crate) fn adopt(process: JsonlProcess, session_id: String) -> Self {
        let session = RpcSession::adopt(map_sdk_message, process, session_id);
        let core =
            RpcClientCore::adopt(session, ADOPTED_PROMPT_REQUEST_ID, Some(claude_usage_hook));
        ClaudeSdkClient {
            core,
            state: Box::new(Mutex::new(ClaudeState {
                models: Vec::new(),
                current_model: "default".to_string(),
                current_effort: "medium".to_string(),
            })),
        }
    }

    pub(super) fn core(&self) -> &RpcClientCore {
        &self.core
    }

    pub fn config_options(&self) -> Vec<super::NeutralConfigOption> {
        let state = self.state.lock().expect("claude_sdk_client mutex poisoned");
        let mut options = Vec::new();
        if !state.models.is_empty() {
            let choices: Vec<super::NeutralConfigChoice> = state
                .models
                .iter()
                .map(|(value, name)| super::NeutralConfigChoice {
                    value: value.clone(),
                    name: name.clone(),
                    description: None,
                })
                .collect();
            options.push(super::NeutralConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                description: None,
                category: Some(super::NeutralConfigCategory::Model),
                kind: super::NeutralConfigKind::Select {
                    current_value: state.current_model.clone(),
                    choices,
                },
            });
        }
        let choices: Vec<super::NeutralConfigChoice> = EFFORT_LEVELS
            .iter()
            .map(|level| super::NeutralConfigChoice {
                value: (*level).to_string(),
                name: (*level).to_string(),
                description: None,
            })
            .collect();
        options.push(super::NeutralConfigOption {
            id: "effort".to_string(),
            name: "Effort".to_string(),
            description: None,
            category: Some(super::NeutralConfigCategory::ThoughtLevel),
            kind: super::NeutralConfigKind::Select {
                current_value: state.current_effort.clone(),
                choices,
            },
        });
        options
    }

    pub fn applied_model(&self) -> String {
        self.state
            .lock()
            .expect("claude_sdk_client mutex poisoned")
            .current_model
            .clone()
    }

    pub fn set_model(&self, value: &str) -> Result<(), String> {
        let unknown = {
            let state = self.state.lock().expect("claude_sdk_client mutex poisoned");
            !state.models.is_empty() && !state.models.iter().any(|(m, _)| m == value)
        };
        if unknown {
            return Err(format!("unknown model: {value}"));
        }
        self.core.with_session(|s| {
            s.request(
                "set_model",
                json!({ "model": value }),
                Duration::from_secs(10),
            )
        })??;
        self.state
            .lock()
            .expect("claude_sdk_client mutex poisoned")
            .current_model = value.to_string();
        Ok(())
    }

    pub fn set_effort(&self, level: &str) -> Result<(), String> {
        if !EFFORT_LEVELS.contains(&level) {
            return Err(format!("unknown effort level: {level}"));
        }
        self.core.with_session(|s| {
            s.request(
                "set_effort",
                json!({ "level": level }),
                Duration::from_secs(10),
            )
        })??;
        self.state
            .lock()
            .expect("claude_sdk_client mutex poisoned")
            .current_effort = level.to_string();
        Ok(())
    }

    pub fn respond_elicitation(
        &self,
        request_id: &str,
        fields: serde_json::Value,
    ) -> Result<(), String> {
        self.core
            .respond("elicitation_response", request_id, fields)
    }
}

#[cfg(test)]
mod adopt_tests {
    use std::path::Path;

    use ajax_core::models::AgentClient;
    use serde_json::json;

    use super::super::claude_sdk_spawn::with_test_claude_sdk_module;
    use super::super::session_client::assert_adopted_turn_finishes;

    #[test]
    fn adopted_claude_turn_finishes_without_a_prompt_or_handshake() {
        let finish = json!({"type": "result", "subtype": "success", "is_error": false});
        with_test_claude_sdk_module(Path::new("claude_sdk_sidecar.mjs"), || {
            assert_adopted_turn_finishes(
                AgentClient::Claude,
                &finish.to_string(),
                "get_context_usage",
            );
        });
    }
}
