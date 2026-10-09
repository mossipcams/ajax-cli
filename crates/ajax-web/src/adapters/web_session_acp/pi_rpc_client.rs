#![allow(dead_code)]
//! Thin client over the Pi RPC building blocks, exposing the prompt/cancel/event/shutdown
//! shape the web session slice uses from `AcpStdioClient`.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, ResourceLink, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionNotification, SessionUpdate, TextContent, UsageUpdate,
};

use super::client::AcpClientEvent;
use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::pi_rpc_map::map_record;
use super::rpc_session::{RpcSession, RpcStep};

/// Timeout for the `get_session_stats` request sent before a prompt finish.
const USAGE_TIMEOUT: Duration = Duration::from_secs(2);

/// A thin client wrapping the Pi RPC process + session behind a `Mutex` so that
/// event and prompt methods can all take `&self`.
pub struct PiRpcClient {
    inner: Mutex<Inner>,
}

struct Inner {
    session: Option<RpcSession>,
    /// Remainder of a `RpcStep::Events` batch not yet consumed by callers.
    event_queue: VecDeque<AcpClientEvent>,
    /// The request id assigned by the most recent `begin_prompt`, cleared on finish.
    active_request_id: Option<u64>,
    /// Monotonically increasing counter for request ids (starts at 1).
    next_request_id: u64,
    /// True once `Exited` was observed or `shutdown` removed the session.
    host_exited: bool,
    /// Model / thinking-level state copied out of the handshake (boxed: the
    /// client sits inside an enum whose variants must stay size-comparable).
    catalog: Box<Catalog>,
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

impl PiRpcClient {
    /// Spawn the Pi RPC process, perform the handshake, and wrap it in a session.
    ///
    /// Arguments passed to the child are exactly:
    /// `["--mode", "rpc", "-na"]` then optionally `["--session", <id>]`, then `extra_args`.
    pub fn spawn(
        program: &Path,
        extra_args: &[String],
        cwd: &Path,
        resume_session_id: Option<&str>,
        handshake_timeout: Duration,
    ) -> Result<PiRpcClient, String> {
        let mut args: Vec<String> = vec!["--mode".into(), "rpc".into(), "-na".into()];
        if let Some(id) = resume_session_id {
            args.push("--session".into());
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
            inner: Mutex::new(Inner {
                session: Some(session),
                event_queue: VecDeque::new(),
                active_request_id: None,
                next_request_id: 1,
                host_exited: false,
                catalog: Box::new(Catalog {
                    models,
                    thinking_levels,
                    current_model_id,
                    current_model_provider,
                    current_thinking_level,
                }),
            }),
        })
    }

    /// The Pi session id assigned during handshake. Returns an empty string if the
    /// session has been shut down.
    pub fn session_id(&self) -> String {
        let g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        g.session
            .as_ref()
            .map(|s| s.session_id().to_string())
            .unwrap_or_default()
    }

    /// The Pi model and thinking-level options in the ACP config-option shape.
    ///
    /// The model option (id "model") is omitted when no models were reported, and
    /// the thinking option (id "thought_level") when no levels were reported.
    /// Both reflect the latest successful `set_model` / `set_thinking_level`.
    pub fn config_options(&self) -> Vec<SessionConfigOption> {
        let g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        let c = &g.catalog;
        let mut options = Vec::new();

        if !c.models.is_empty() {
            let entries: Vec<SessionConfigSelectOption> = c
                .models
                .iter()
                .map(|m| {
                    let value = format!("{}/{}", m.provider, m.id);
                    let name = if m.name.is_empty() {
                        m.id.clone()
                    } else {
                        m.name.clone()
                    };
                    SessionConfigSelectOption::new(value, name)
                })
                .collect();
            let current_value = current_model_value(&c.models, c.current_model_id.as_deref());
            options.push(
                SessionConfigOption::select("model", "Model", current_value, entries)
                    .category(SessionConfigOptionCategory::Model),
            );
        }

        if !c.thinking_levels.is_empty() {
            let entries: Vec<SessionConfigSelectOption> = c
                .thinking_levels
                .iter()
                .map(|level| SessionConfigSelectOption::new(level.clone(), level.clone()))
                .collect();
            let current =
                current_level_value(&c.thinking_levels, c.current_thinking_level.as_deref());
            options.push(
                SessionConfigOption::select("thought_level", "Thinking", current, entries)
                    .category(SessionConfigOptionCategory::ThoughtLevel),
            );
        }

        options
    }

    /// The currently applied Pi model as "provider/id", or an empty string when
    /// there is no model catalog or no current model.
    pub fn applied_model(&self) -> String {
        let g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        if g.catalog.models.is_empty() {
            return String::new();
        }
        match (
            &g.catalog.current_model_id,
            &g.catalog.current_model_provider,
        ) {
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

        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session.request(
            "set_model",
            serde_json::json!({ "provider": provider, "modelId": model_id }),
            Duration::from_secs(10),
        )?;

        g.catalog.current_model_id = Some(model_id.to_string());
        g.catalog.current_model_provider = Some(provider.to_string());
        Ok(())
    }

    /// Apply a thinking-level selection through Pi's `set_thinking_level`
    /// command, storing it as the current level on success.
    pub fn set_thinking_level(&self, level: &str) -> Result<(), String> {
        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session.request(
            "set_thinking_level",
            serde_json::json!({ "level": level }),
            Duration::from_secs(10),
        )?;

        g.catalog.current_thinking_level = Some(level.to_string());
        Ok(())
    }

    /// Build the prompt text from content blocks and begin a prompt run.
    ///
    /// Text blocks are joined with `"\n\n"`; ResourceLink blocks contribute their URI as a line;
    /// all other block kinds produce a short placeholder. Returns the request id for tracking.
    pub fn begin_prompt(&self, blocks: &[ContentBlock]) -> Result<u64, String> {
        let mut parts: Vec<String> = Vec::new();

        for block in blocks {
            match block {
                ContentBlock::Text(tc @ TextContent { .. }) => {
                    parts.push(tc.text.clone());
                }
                ContentBlock::ResourceLink(rl @ ResourceLink { .. }) => {
                    parts.push(rl.uri.clone());
                }
                // ponytail: images are not forwarded yet — only a placeholder is emitted
                _ => {
                    parts.push("[image omitted]".to_string());
                }
            }
        }

        let message = parts.join("\n\n");
        if message.is_empty() {
            return Err("empty prompt content: no blocks supplied".to_string());
        }

        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session
            .begin_prompt(&message)
            .map_err(|e| format!("begin_prompt failed: {e}"))?;

        let id = g.next_request_id;
        g.next_request_id += 1;
        g.active_request_id = Some(id);

        Ok(id)
    }

    /// Abort the active prompt run. Returns `Err` when no run is in flight.
    pub fn cancel(&self) -> Result<(), String> {
        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        let session = g
            .session
            .as_mut()
            .ok_or_else(|| "session not available (shut down)".to_string())?;
        session.abort().map_err(|e| format!("cancel failed: {e}"))
    }

    /// True while a prompt run is active.
    pub fn prompt_in_flight(&self) -> bool {
        let g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        g.session.as_ref().is_some_and(|s| s.run_active())
    }

    /// Non-blocking: returns the next queued event, or `None` if the queue is empty and no
    /// new step is immediately available. Uses `Duration::ZERO`.
    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.wait_event(Duration::ZERO)
    }

    /// Block up to `timeout` for the next event. Returns `None` on timeout with no events.
    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");

        // Drain the remainder of a previous Events batch first.
        if let Some(ev) = g.event_queue.pop_front() {
            return Some(ev);
        }

        let session = g.session.as_mut()?;

        match session.next_step(timeout) {
            RpcStep::Events(evs) => {
                for ev in evs {
                    g.event_queue.push_back(ev);
                }
                g.event_queue.pop_front()
            }
            RpcStep::RunFinished { aborted } => {
                let id = g.active_request_id.take().unwrap_or(0);
                let stop_reason = if aborted { "cancelled" } else { "end_turn" };

                // Ask Pi for the final context usage ahead of the finish, the
                // way ACP agents report usage before the request result.
                let usage = g.session.as_mut().and_then(|session| {
                    let data = session
                        .request("get_session_stats", serde_json::json!({}), USAGE_TIMEOUT)
                        .ok()?;
                    let context = data.get("contextUsage")?;
                    let tokens = context.get("tokens")?.as_u64()?;
                    let window = context.get("contextWindow")?.as_u64()?;
                    let session_id = session.session_id().to_string();
                    Some(AcpClientEvent::SessionUpdate(Box::new(
                        SessionNotification::new(
                            session_id,
                            SessionUpdate::UsageUpdate(UsageUpdate::new(tokens, window)),
                        ),
                    )))
                });

                let finished = AcpClientEvent::RequestFinished {
                    id,
                    method: "session/prompt",
                    result: Ok(serde_json::json!({ "stopReason": stop_reason })),
                };

                match usage {
                    Some(usage_event) => {
                        g.event_queue.push_back(finished);
                        Some(usage_event)
                    }
                    None => Some(finished),
                }
            }
            RpcStep::PromptRejected(e) => {
                let _id = g.active_request_id.take();
                Some(AcpClientEvent::RequestFinished {
                    id: _id.unwrap_or(0),
                    method: "session/prompt",
                    result: Err(e),
                })
            }
            RpcStep::Error(t) => Some(AcpClientEvent::Error(t)),
            RpcStep::Exited => {
                g.host_exited = true;
                Some(AcpClientEvent::Exited)
            }
            RpcStep::Idle => None,
        }
    }

    /// Take the session out of its `Option`, dropping it (which closes stdin and reaps the
    /// child). Returns the session id on first call, `None` thereafter.
    pub fn shutdown(&self) -> Option<String> {
        let mut g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        if let Some(session) = g.session.take() {
            let id = session.session_id().to_string();
            g.host_exited = true;
            Some(id)
        } else {
            None
        }
    }

    /// True once `Exited` was observed from the event stream or `shutdown` removed the session.
    pub fn host_exited(&self) -> bool {
        let g = self.inner.lock().expect("pi_rpc_client mutex poisoned");
        g.host_exited || g.session.is_none()
    }
}
