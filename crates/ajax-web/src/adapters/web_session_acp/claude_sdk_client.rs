//! Thin Claude Agent SDK client: drives the Node sidecar through the shared JSONL process, session and client core.
// A later packet wires this client into the session bridge.
#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::agent_event::NativeImage;
use super::agent_event::{AgentEvent, NativeCommand};
use super::claude_sdk_map::map_sdk_message;
use super::jsonl_process::JsonlProcess;
use super::rpc_client::RpcClientCore;
use super::rpc_handshake::{
    await_init_response, check_resumed_session, init_commands, init_session_id, RpcHandshake,
    RpcModel,
};
use super::rpc_session::RpcSession;

const SIDECAR_SOURCE: &str = include_str!("../../../sidecar/claude_sdk_sidecar.mjs");

// ponytail: fixed list; the SDK reports per-model effort support, refine later.
pub const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn sidecar_launch_args(sdk_module: Option<&str>) -> Vec<String> {
    super::rpc_handshake::sidecar_launch_args(SIDECAR_SOURCE, sdk_module)
}

pub struct ClaudeSdkClient {
    core: RpcClientCore,
    // Box keeps SessionClient variants size-comparable (clippy::large_enum_variant).
    state: Box<Mutex<ClaudeState>>,
}

struct ClaudeState {
    models: Vec<(String, String)>, // (value, display name)
    current_model: String,
    current_effort: String,
}

impl ClaudeSdkClient {
    /// OS process id of the live child while a session exists; `None` after shutdown.
    pub fn child_pid(&self) -> Option<u32> {
        self.core.child_pid()
    }
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
            // ponytail: the SDK default is not reported yet; use medium until it is.
            current_effort: "medium".to_string(),
        };
        // The handshake's advertised commands are exposed through
        // `take_available_commands` once, like an event, rather than queued as
        // one.
        let commands: Vec<NativeCommand> = handshake
            .commands
            .iter()
            .map(NativeCommand::from_rpc_command)
            .collect();

        let session = RpcSession::new(process, handshake, map_sdk_message);
        let core = RpcClientCore::new(session, None);
        // The mapper emits usage from Claude's result message.
        core.set_available_commands(commands);

        Ok(ClaudeSdkClient {
            core,
            state: Box::new(Mutex::new(state)),
        })
    }

    /// The shared transport core, used by the session client's two-arm dispatch.
    pub(super) fn core(&self) -> &RpcClientCore {
        &self.core
    }

    pub fn session_id(&self) -> String {
        self.core.session_id()
    }

    /// Slash commands advertised at handshake, returned by the first call
    /// only (one-shot, like an event) so every later drain yields an empty
    /// list; ACP `AvailableCommandsUpdated` updates replace the list within
    /// the same drain.
    pub fn take_available_commands(&self) -> Vec<NativeCommand> {
        self.core.take_available_commands()
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

    /// Begin a prompt run from a plain-text message.
    pub fn begin_prompt(&self, message: &str) -> Result<u64, String> {
        self.core.begin_prompt(message)
    }

    /// Begin a prompt run from a text message plus attached images.
    pub fn begin_prompt_with_images(
        &self,
        message: &str,
        images: &[NativeImage],
    ) -> Result<u64, String> {
        self.core.begin_prompt_with_images(message, images)
    }

    pub fn cancel(&self) -> Result<(), String> {
        self.core.cancel()
    }

    pub fn prompt_in_flight(&self) -> bool {
        self.core.prompt_in_flight()
    }

    /// Send a response for a pending elicitation to the SDK sidecar.
    pub fn respond_elicitation(
        &self,
        request_id: &str,
        fields: serde_json::Value,
    ) -> Result<(), String> {
        self.core
            .respond("elicitation_response", request_id, fields)
    }

    pub fn poll_event(&self) -> Option<AgentEvent> {
        self.core.poll_event()
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AgentEvent> {
        self.core.wait_event(timeout)
    }

    pub fn shutdown(&self) -> Option<String> {
        self.core.shutdown()
    }

    pub fn host_exited(&self) -> bool {
        self.core.host_exited()
    }
}
