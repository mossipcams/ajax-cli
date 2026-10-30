#![allow(dead_code)]

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};

use super::agent_event::{AgentEvent, NativeCommand, NativeEvent, NativeImage};
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake_with_model_provider;
use super::pi_rpc_map::map_record;
use super::rpc_client::RpcClientCore;
use super::rpc_session::RpcSession;
use super::session_client::ADOPTED_PROMPT_REQUEST_ID;

const USAGE_TIMEOUT: Duration = Duration::from_secs(2);

pub struct PiRpcClient {
    core: RpcClientCore,
    catalog: Box<Mutex<Catalog>>,
}

struct Catalog {
    models: Vec<ModelEntry>,
    thinking_levels: Vec<String>,
    current_model_id: Option<String>,
    current_model_provider: Option<String>,
    current_thinking_level: Option<String>,
}

struct ModelEntry {
    id: String,
    name: String,
    provider: String,
}

fn current_model_value(
    models: &[ModelEntry],
    current_model_id: Option<&str>,
    current_model_provider: Option<&str>,
) -> String {
    let current = models
        .iter()
        .find(|m| {
            current_model_id == Some(m.id.as_str())
                && current_model_provider.is_none_or(|p| p == m.provider)
        })
        .or_else(|| models.first())
        .expect("models was checked non-empty");
    format!("{}/{}", current.provider, current.id)
}

fn current_level_value(levels: &[String], current_level: Option<&str>) -> String {
    levels
        .iter()
        .find(|level| current_level == Some(level.as_str()))
        .or_else(|| levels.first())
        .expect("levels was checked non-empty")
        .clone()
}

fn pi_usage_hook(session: &mut RpcSession) -> Option<AgentEvent> {
    let data = session
        .request("get_session_stats", serde_json::json!({}), USAGE_TIMEOUT)
        .ok()?;
    let context = data.get("contextUsage")?;
    let tokens = context.get("tokens")?.as_u64()?;
    let window = context.get("contextWindow")?.as_u64()?;
    Some(AgentEvent::Native(NativeEvent::Usage {
        used: tokens,
        size: window,
    }))
}

pub(super) fn builtin_command(message: &str) -> Option<(&'static str, Value)> {
    let trimmed = message.trim();
    if trimmed == "/compact" {
        return Some(("compact", json!({})));
    }
    if let Some(rest) = trimmed.strip_prefix("/compact ") {
        let instructions = rest.trim();
        let fields = if instructions.is_empty() {
            json!({})
        } else {
            json!({ "customInstructions": instructions })
        };
        return Some(("compact", fields));
    }
    if let Some(name) = trimmed.strip_prefix("/name ") {
        let name = name.trim();
        if !name.is_empty() {
            return Some(("set_session_name", json!({ "name": name })));
        }
    }
    None
}

impl PiRpcClient {
    pub(crate) fn adopt(process: JsonlProcess, session_id: String) -> Self {
        let session = RpcSession::adopt(map_record, process, session_id).with_image_type_tag();
        let core = RpcClientCore::adopt(session, ADOPTED_PROMPT_REQUEST_ID, Some(pi_usage_hook));
        PiRpcClient {
            core,
            catalog: Box::new(Mutex::new(Catalog {
                models: Vec::new(),
                thinking_levels: Vec::new(),
                current_model_id: None,
                current_model_provider: None,
                current_thinking_level: None,
            })),
        }
    }

    pub fn spawn(
        program: &Path,
        extra_args: &[String],
        cwd: &Path,
        resume_session_id: Option<&str>,
        handshake_timeout: Duration,
    ) -> Result<PiRpcClient, String> {
        let mut args: Vec<String> = vec!["--mode".into(), "rpc".into(), "-na".into()];
        if let Some(id) = resume_session_id {
            args.push("--session-id".into());
            args.push(id.to_string());
        }
        args.extend_from_slice(extra_args);

        let mut process = JsonlProcess::spawn(program, &args, cwd)
            .map_err(|e| format!("failed to spawn pi rpc process: {e}"))?;

        let (hs, state_model_provider) =
            handshake_with_model_provider(&mut process, handshake_timeout)
                .map_err(|e| format!("pi rpc handshake failed: {e}"))?;

        let models: Vec<ModelEntry> = hs
            .models
            .iter()
            .map(|m| ModelEntry {
                id: m.id.clone(),
                name: m.name.clone(),
                provider: m.provider.clone(),
            })
            .collect();
        let thinking_levels = hs.thinking_levels.clone();
        let current_model_id = hs.model_id.clone();
        let current_model_provider = state_model_provider.or_else(|| {
            models
                .iter()
                .find(|m| current_model_id.as_deref() == Some(m.id.as_str()))
                .map(|m| m.provider.clone())
        });
        let current_thinking_level = hs.thinking_level.clone();

        let commands: Vec<NativeCommand> = hs
            .commands
            .iter()
            .map(NativeCommand::from_rpc_command)
            .collect();

        let session = RpcSession::new(process, hs, map_record).with_image_type_tag();

        let core = RpcClientCore::new(session, Some(pi_usage_hook));
        core.set_available_commands(commands);

        Ok(PiRpcClient {
            core,
            catalog: Box::new(Mutex::new(Catalog {
                models,
                thinking_levels,
                current_model_id,
                current_model_provider,
                current_thinking_level,
            })),
        })
    }

    pub(super) fn core(&self) -> &RpcClientCore {
        &self.core
    }

    pub fn config_options(&self) -> Vec<super::NeutralConfigOption> {
        let c = self.catalog.lock().expect("pi_rpc_client mutex poisoned");
        let mut options = Vec::new();

        if !c.models.is_empty() {
            let choices: Vec<super::NeutralConfigChoice> = c
                .models
                .iter()
                .map(|m| {
                    let value = format!("{}/{}", m.provider, m.id);
                    let name = if m.name.is_empty() {
                        m.id.clone()
                    } else {
                        m.name.clone()
                    };
                    super::NeutralConfigChoice {
                        value,
                        name,
                        description: None,
                    }
                })
                .collect();
            options.push(super::NeutralConfigOption {
                id: "model".to_string(),
                name: "Model".to_string(),
                description: None,
                category: Some(super::NeutralConfigCategory::Model),
                kind: super::NeutralConfigKind::Select {
                    current_value: current_model_value(
                        &c.models,
                        c.current_model_id.as_deref(),
                        c.current_model_provider.as_deref(),
                    ),
                    choices,
                },
            });
        }

        if !c.thinking_levels.is_empty() {
            let choices: Vec<super::NeutralConfigChoice> = c
                .thinking_levels
                .iter()
                .map(|level| super::NeutralConfigChoice {
                    value: level.clone(),
                    name: level.clone(),
                    description: None,
                })
                .collect();
            options.push(super::NeutralConfigOption {
                id: "thought_level".to_string(),
                name: "Thinking".to_string(),
                description: None,
                category: Some(super::NeutralConfigCategory::ThoughtLevel),
                kind: super::NeutralConfigKind::Select {
                    current_value: current_level_value(
                        &c.thinking_levels,
                        c.current_thinking_level.as_deref(),
                    ),
                    choices,
                },
            });
        }

        options
    }

    pub fn applied_model(&self) -> String {
        let g = self.catalog.lock().expect("pi_rpc_client mutex poisoned");
        if g.models.is_empty() {
            return String::new();
        }
        match (&g.current_model_id, &g.current_model_provider) {
            (Some(id), Some(provider)) => format!("{provider}/{id}"),
            _ => String::new(),
        }
    }

    pub fn set_model(&self, value: &str) -> Result<(), String> {
        let Some((provider, model_id)) = value.split_once('/') else {
            return Err("model must be provider/id".to_string());
        };

        self.core.with_session(|s| {
            s.request(
                "set_model",
                serde_json::json!({ "provider": provider, "modelId": model_id }),
                Duration::from_secs(10),
            )
        })??;
        let mut g = self.catalog.lock().expect("pi_rpc_client mutex poisoned");

        g.current_model_id = Some(model_id.to_string());
        g.current_model_provider = Some(provider.to_string());
        Ok(())
    }

    pub fn set_thinking_level(&self, level: &str) -> Result<(), String> {
        self.core.with_session(|s| {
            s.request(
                "set_thinking_level",
                serde_json::json!({ "level": level }),
                Duration::from_secs(10),
            )
        })??;
        let mut g = self.catalog.lock().expect("pi_rpc_client mutex poisoned");

        g.current_thinking_level = Some(level.to_string());
        Ok(())
    }

    pub fn begin_prompt_with_images(
        &self,
        message: &str,
        images: &[NativeImage],
    ) -> Result<u64, String> {
        if images.is_empty() {
            if let Some((command, fields)) = builtin_command(message) {
                return self.core.begin_command(command, fields);
            }
            return self.core.begin_prompt(message);
        }
        self.core.begin_prompt_with_images(message, images)
    }

    pub fn respond_dialog(
        &self,
        request_id: &str,
        fields: serde_json::Value,
    ) -> Result<(), String> {
        self.core
            .respond("extension_ui_response", request_id, fields)
    }
}

#[cfg(test)]
mod adopt_tests {
    use std::path::Path;

    use ajax_core::models::AgentClient;
    use serde_json::json;

    use super::super::session_client::{assert_adopted_turn_finishes, with_test_pi_program};

    #[test]
    fn adopted_pi_turn_finishes_without_a_prompt_or_handshake() {
        let finish = json!({"type": "agent_settled"});
        with_test_pi_program(Path::new("pi"), &[], || {
            assert_adopted_turn_finishes(AgentClient::Pi, &finish.to_string(), "get_session_stats");
        });
    }

    #[test]
    fn adopted_pi_compact_and_handled_responses_end_the_turn() {
        let compact = json!({"type": "response", "id": "7", "command": "compact", "success": true});
        let handled = json!({
            "type": "response",
            "id": "8",
            "command": "prompt",
            "success": true,
            "data": {"disposition": "handled"}
        });
        with_test_pi_program(Path::new("pi"), &[], || {
            assert_adopted_turn_finishes(
                AgentClient::Pi,
                &compact.to_string(),
                "get_session_stats",
            );
            assert_adopted_turn_finishes(
                AgentClient::Pi,
                &handled.to_string(),
                "get_session_stats",
            );
        });
    }
}
