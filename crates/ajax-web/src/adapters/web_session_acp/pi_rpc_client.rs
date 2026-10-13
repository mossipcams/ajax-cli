#![allow(dead_code)]
//! Thin client over the Pi RPC building blocks, exposing the prompt/cancel/event/shutdown
//! shape the web session slice uses from `AcpStdioClient`.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use agent_client_protocol::schema::v1::ContentBlock;

use super::agent_event::{AgentEvent, NativeEvent};
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::pi_rpc_map::map_record;
use super::rpc_client::RpcClientCore;
use super::rpc_session::RpcSession;

/// Timeout for the `get_session_stats` request sent before a prompt finish.
const USAGE_TIMEOUT: Duration = Duration::from_secs(2);

/// A thin client wrapping the Pi RPC process + session behind a `Mutex` so that
/// event and prompt methods can all take `&self`.
pub struct PiRpcClient {
    core: RpcClientCore,
    /// Boxed: the client sits inside the SessionClient enum, whose variants must stay size-comparable (clippy::large_enum_variant).
    catalog: Box<Mutex<Catalog>>,
}

/// Model and thinking-level state snapshotted from the Pi handshake and kept
/// up to date as selections are applied.
struct Catalog {
    /// Model catalog copied out of the handshake before the session consumed it.
    models: Vec<ModelEntry>,
    /// Thinking levels copied out of the handshake before the session consumed it.
    thinking_levels: Vec<String>,
    /// Id of the currently selected Pi model (handshake state, then successful `set_model`).
    current_model_id: Option<String>,
    /// Provider of the currently selected Pi model (handshake state, then `set_model`).
    current_model_provider: Option<String>,
    /// Currently selected thinking level (handshake state, then `set_thinking_level`).
    current_thinking_level: Option<String>,
}

/// One model from the Pi handshake catalog, flattened to plain strings so the
/// handshake record can be dropped after `spawn`.
struct ModelEntry {
    id: String,
    name: String,
    provider: String,
}

/// The "provider/id" value of the entry matching `current_model_id`,
/// falling back to the first entry when nothing matches.
fn current_model_value(models: &[ModelEntry], current_model_id: Option<&str>) -> String {
    let current = models
        .iter()
        .find(|m| current_model_id == Some(m.id.as_str()))
        .or_else(|| models.first())
        .expect("models was checked non-empty");
    format!("{}/{}", current.provider, current.id)
}

/// The level matching `current_level`, falling back to the first level when
/// nothing matches.
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

impl PiRpcClient {
    /// Spawn the Pi RPC process, perform the handshake, and wrap it in a session.
    ///
    /// Arguments passed to the child are exactly:
    /// `["--mode", "rpc", "-na"]` then optionally `["--session-id", <id>]`, then `extra_args`.
    /// (`--session` only matches sessions already on disk; Pi persists a
    /// session only after its first message, so `--session-id` is required for
    /// resume; issue #1283.)
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

        let hs = handshake(&mut process, handshake_timeout)
            .map_err(|e| format!("pi rpc handshake failed: {e}"))?;

        // `RpcSession::new` consumes the handshake record, so copy out the
        // model / thinking-level state before handing it over.
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
        let current_model_provider = models
            .iter()
            .find(|m| current_model_id.as_deref() == Some(m.id.as_str()))
            .map(|m| m.provider.clone());
        let current_thinking_level = hs.thinking_level.clone();

        let session = RpcSession::new(process, hs, map_record);

        Ok(PiRpcClient {
            core: RpcClientCore::new(session, Some(pi_usage_hook)),
            catalog: Box::new(Mutex::new(Catalog {
                models,
                thinking_levels,
                current_model_id,
                current_model_provider,
                current_thinking_level,
            })),
        })
    }

    /// The Pi session id assigned during handshake. Returns an empty string if the
    /// session has been shut down.
    pub fn session_id(&self) -> String {
        self.core.session_id()
    }

    /// OS process id of the live child while a session exists; `None` after shutdown.
    pub fn child_pid(&self) -> Option<u32> {
        self.core.child_pid()
    }

    /// The Pi model and thinking-level options as harness-neutral values.
    ///
    /// The model option (id "model") is omitted when no models were reported, and
    /// the thinking option (id "thought_level") when no levels were reported.
    /// Both reflect the latest successful `set_model` / `set_thinking_level`.
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
                    current_value: current_model_value(&c.models, c.current_model_id.as_deref()),
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

    /// The currently applied Pi model as "provider/id", or an empty string when
    /// there is no model catalog or no current model.
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

    /// Apply a model selection of the form "provider/id" through Pi's
    /// `set_model` command, storing it as the current model on success.
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

    /// Apply a thinking-level selection through Pi's `set_thinking_level`
    /// command, storing it as the current level on success.
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

    /// Build the prompt text from content blocks and begin a prompt run.
    ///
    /// Text blocks are joined with `"\n\n"`; ResourceLink blocks contribute their URI as a line;
    /// all other block kinds produce a short placeholder. Returns the request id for tracking.
    pub fn begin_prompt(&self, blocks: &[ContentBlock]) -> Result<u64, String> {
        self.core.begin_prompt(blocks)
    }

    /// Abort the active prompt run. Returns `Err` when no run is in flight.
    pub fn cancel(&self) -> Result<(), String> {
        self.core.cancel()
    }

    /// True while a prompt run is active.
    pub fn prompt_in_flight(&self) -> bool {
        self.core.prompt_in_flight()
    }

    /// Non-blocking: returns the next queued event, or `None` if the queue is empty and no
    /// new step is immediately available. Uses `Duration::ZERO`.
    pub fn poll_event(&self) -> Option<AgentEvent> {
        self.core.poll_event()
    }

    /// Block up to `timeout` for the next event. Returns `None` on timeout with no events.
    pub fn wait_event(&self, timeout: Duration) -> Option<AgentEvent> {
        self.core.wait_event(timeout)
    }

    /// Take the session out of its `Option`, dropping it (which closes stdin and reaps the
    /// child). Returns the session id on first call, `None` thereafter.
    pub fn shutdown(&self) -> Option<String> {
        self.core.shutdown()
    }

    /// True once `Exited` was observed from the event stream or `shutdown` removed the session.
    pub fn host_exited(&self) -> bool {
        self.core.host_exited()
    }
}
