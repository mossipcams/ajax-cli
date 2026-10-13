//! Thin Claude Agent SDK client: drives the Node sidecar through the shared JSONL process, session and client core.
// A later packet wires this client into the session bridge.
#![allow(dead_code)]

use std::path::Path;
use std::sync::{mpsc::TryRecvError, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeCommand};
use super::claude_sdk_map::map_sdk_message;
use super::jsonl_process::{JsonlProcess, JsonlRecord};
use super::rpc_client::RpcClientCore;
use super::rpc_handshake::{RpcCommand, RpcHandshake, RpcModel};
use super::rpc_session::RpcSession;

const SIDECAR_SOURCE: &str = include_str!("../../../sidecar/claude_sdk_sidecar.mjs");

// ponytail: fixed list; the SDK reports per-model effort support, refine later.
pub const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn sidecar_launch_args(sdk_module: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "--input-type=module".to_string(),
        "-e".to_string(),
        SIDECAR_SOURCE.to_string(),
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

pub struct ClaudeSdkClient {
    core: RpcClientCore,
    // Box keeps SessionClient variants size-comparable (clippy::large_enum_variant).
    state: Box<Mutex<ClaudeState>>,
    // Slash commands advertised at handshake; handed to the first drain that
    // asks for them, one-shot like an event.
    commands: Mutex<Option<Vec<NativeCommand>>>,
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
        let started = Instant::now();
        let mut pending = Vec::new();
        let response = loop {
            if started.elapsed() >= init_timeout {
                return Err(format!(
                    "claude sdk init timed out: {}",
                    process.stderr_tail()
                ));
            }
            match process.try_recv() {
                Ok(JsonlRecord::Record(record)) => {
                    if record.get("type").and_then(Value::as_str) == Some("response")
                        && record.get("id").and_then(Value::as_str) == Some(init_id.as_str())
                    {
                        break record;
                    }
                    pending.push(record);
                }
                Ok(JsonlRecord::Error(error)) => {
                    return Err(format!(
                        "claude sdk init failed: {error}; stderr: {}",
                        process.stderr_tail()
                    ));
                }
                Ok(JsonlRecord::Exited) | Err(TryRecvError::Disconnected) => {
                    return Err(format!(
                        "claude sdk sidecar exited before init response: {}",
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
            return Err(format!("claude sdk init failed: {error}"));
        }
        let data = &response["data"];
        let session_id = data
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| "claude sdk init failed: missing sessionId".to_string())?
            .to_string();
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
            commands: data
                .get("commands")
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
                .collect(),
            context_window: None,
            pending,
        };
        if let Some(id) = resume {
            if handshake.session_id != id {
                let got = &handshake.session_id;
                return Err(format!(
                    "claude sdk resumed a different session: asked {id}, got {got}"
                ));
            }
        }
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
        Ok(ClaudeSdkClient {
            // The mapper emits usage from Claude's result message.
            core: RpcClientCore::new(session, None),
            state: Box::new(Mutex::new(state)),
            commands: Mutex::new(Some(commands)),
        })
    }

    pub fn session_id(&self) -> String {
        self.core.session_id()
    }

    /// Slash commands advertised at handshake, returned by the first call
    /// only (one-shot, like an event) so every later drain yields an empty
    /// list; ACP `AvailableCommandsUpdated` updates replace the list within
    /// the same drain.
    pub fn take_available_commands(&self) -> Vec<NativeCommand> {
        self.commands
            .lock()
            .expect("claude_sdk_client mutex poisoned")
            .take()
            .unwrap_or_default()
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

    pub fn cancel(&self) -> Result<(), String> {
        self.core.cancel()
    }

    pub fn prompt_in_flight(&self) -> bool {
        self.core.prompt_in_flight()
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
