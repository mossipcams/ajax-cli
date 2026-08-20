//! ACP child spawn, replace, and first attach for per-task session slots.

use super::task_session::TaskSessionState;
use super::transcript::{
    already_noted, context_reset_needed, context_reset_note, harness_switch_note, slot_must_replace,
};
use super::{apply_cancel_to_queue, SessionServerEvent};
use crate::adapters::web_session_acp::{
    applied_model_id_for_persist, config_option_descriptors, option_triggers_model_persist,
    AcpStdioClient, SpawnReport,
};
use crate::adapters::web_session_store::{self, StoredSession};
use ajax_core::models::AgentClient;
use std::path::Path;

pub(super) async fn acquire(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<(), String> {
    state.worktree_path = Some(worktree_path.to_path_buf());
    state.agent = agent;

    if let Some(client) = state.client.as_mut() {
        let host_exited = client.host_exited();
        if !slot_must_replace(state.acp_alive, &state.model, model, host_exited) {
            state.acquire_holder();
            return Ok(());
        }
        let resume_id = replace_resume_id(
            &state.model,
            model,
            &state.state_dir,
            &state.qualified_handle,
        );
        state.acquire_holder();
        release_live_client(state)?;
        let (new_client, report) =
            spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await?;
        install_replaced_client(state, new_client, &report, model)?;
        return Ok(());
    }

    let stored: StoredSession<SessionServerEvent> =
        web_session_store::load(&state.state_dir, &state.qualified_handle);
    let resume_id = stored.acp_session_id.clone();
    let (client, report) = spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await?;

    let mut log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
    let note = context_reset_note();
    if context_reset_needed(report.resumed, &log) && !already_noted(&log, &note) {
        log.append(vec![note.clone()]);
        web_session_store::append_events(
            &state.state_dir,
            &state.qualified_handle,
            std::slice::from_ref(&note),
        );
    }
    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(client.session_id()),
        &report.applied_model,
    );
    state.client = Some(client);
    state.model = model.to_string();
    state.applied_model = report.applied_model.clone();
    if let Some(options) = report.config_options.as_deref() {
        state.session_config_options = Some(config_option_descriptors(options));
    }
    if let Some(error) = &report.model_apply_error {
        log.append(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
        web_session_store::append_events(
            &state.state_dir,
            &state.qualified_handle,
            &[SessionServerEvent::Error {
                message: error.clone(),
            }],
        );
    }
    state.generation = 0;
    state.reset_holders_to_one();
    state.log = log;
    state.queued.clear();
    state.last_released = None;
    state.acp_alive = true;
    Ok(())
}

pub(super) async fn apply_model(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
) -> Result<u64, String> {
    let Some(client) = state.client.as_mut() else {
        state.model = model.to_string();
        return Ok(state.generation);
    };

    if client.host_exited() {
        return respawn(state, worktree_path, model, true).await;
    }

    let generation_before = state.generation;
    let apply_result = tokio::task::block_in_place(|| client.apply_model_pin(model));
    match apply_result {
        Ok(outcome) if outcome.error.is_none() => {
            state.model = model.to_string();
            state.applied_model = outcome.applied_model.clone();
            if let Some(options) = outcome.config_options.as_deref() {
                state.session_config_options = Some(config_option_descriptors(options));
            }
            web_session_store::save_meta(
                &state.state_dir,
                &state.qualified_handle,
                Some(client.session_id()),
                &state.applied_model,
            );
            state.pending_model_snapshot = Some(outcome.applied_model);
            state.pending_config_snapshot = state.session_config_options.clone();
            Ok(generation_before)
        }
        Ok(outcome) => {
            let message = outcome.error.unwrap_or_else(|| {
                format!(
                    "session model {model} was refused — harness is running {}",
                    outcome.applied_model
                )
            });
            state.append_to_log(vec![SessionServerEvent::Error {
                message: message.clone(),
            }]);
            Err(message)
        }
        Err(error) => Err(error),
    }
}

pub(crate) struct ApplyConfigOptionResult {
    pub generation: u64,
    /// Pipe-form task metadata to persist after a successful model-option apply.
    pub persist_model: Option<String>,
    /// Live apply succeeded, but its confirmed state cannot be persisted safely.
    pub persist_warning: Option<String>,
}

pub(super) async fn apply_config_option(
    state: &mut TaskSessionState,
    config_id: &str,
    value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
) -> Result<ApplyConfigOptionResult, String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };

    if client.host_exited() {
        return Err("ACP process exited — reconnect to change config".to_string());
    }

    let generation_before = state.generation;
    let apply_result = tokio::task::block_in_place(|| client.apply_config_option(config_id, value));
    match apply_result {
        Ok(outcome) if outcome.error.is_none() => {
            state.applied_model = outcome.applied_model.clone();
            if let Some(options) = outcome.config_options.as_deref() {
                state.session_config_options = Some(config_option_descriptors(options));
            }
            web_session_store::save_meta(
                &state.state_dir,
                &state.qualified_handle,
                Some(client.session_id()),
                &state.applied_model,
            );
            state.pending_model_snapshot = Some(outcome.applied_model);
            state.pending_config_snapshot = state.session_config_options.clone();
            let (persist_model, persist_warning) = match outcome.config_options.as_deref() {
                Some(options) if option_triggers_model_persist(options, config_id) => {
                    match applied_model_id_for_persist(options) {
                        Ok(model) => (Some(model), None),
                        Err(error) => (
                            None,
                            Some(format!(
                                "Model changed but restart state was not saved — {error}"
                            )),
                        ),
                    }
                }
                _ => (None, None),
            };
            Ok(ApplyConfigOptionResult {
                generation: generation_before,
                persist_model,
                persist_warning,
            })
        }
        Ok(outcome) => {
            let message = outcome.error.unwrap_or_else(|| {
                format!(
                    "config option {config_id} was refused — harness is running {}",
                    outcome.applied_model
                )
            });
            state.append_to_log(vec![SessionServerEvent::Error {
                message: message.clone(),
            }]);
            Err(message)
        }
        Err(error) => Err(error),
    }
}

pub(super) async fn respawn(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    force: bool,
) -> Result<u64, String> {
    if state.client.is_none() {
        return Err("session slot missing".to_string());
    }
    let host_exited = state
        .client
        .as_mut()
        .map(|client| client.host_exited())
        .unwrap_or(true);
    if !force && !slot_must_replace(state.acp_alive, &state.model, model, host_exited) {
        return Ok(state.generation);
    }
    let resume_id = replace_resume_id(
        &state.model,
        model,
        &state.state_dir,
        &state.qualified_handle,
    );
    let agent = state.agent;
    release_live_client(state)?;
    let (new_client, report) = spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await?;
    install_replaced_client(state, new_client, &report, model)?;
    Ok(state.generation)
}

pub(super) async fn reset_harness_context(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<u64, String> {
    release_live_client(state)?;

    web_session_store::clear_acp_session_id(&state.state_dir, &state.qualified_handle);

    let (new_client, report) = spawn_acp(agent, worktree_path, model, None).await?;

    let note = harness_switch_note(state.stream_normalizer.fresh_item_id());
    state.append_to_log(vec![note]);

    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(new_client.session_id()),
        &report.applied_model,
    );
    state.client = Some(new_client);
    state.model = model.to_string();
    state.applied_model = report.applied_model.clone();
    if let Some(options) = report.config_options.as_deref() {
        state.session_config_options = Some(config_option_descriptors(options));
    }
    state.agent = agent;
    state.generation = state.generation.saturating_add(1);
    state.acp_alive = true;
    state.stream_normalizer = super::normalize::StreamNormalizer::default();
    state.usage_deduper = super::acp_usage::UsageDeduper::default();
    if let Some(error) = &report.model_apply_error {
        state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    Ok(state.generation)
}

/// Cancel and drop the live ACP child so the next spawn owns stdio alone.
fn release_live_client(state: &mut TaskSessionState) -> Result<(), String> {
    let Some(mut client) = state.client.take() else {
        apply_cancel_to_queue(&mut state.queued, false);
        return Ok(());
    };
    apply_cancel_to_queue(&mut state.queued, false);
    if !client.host_exited() {
        let cancelled = client.cancel()?;
        let resolved: Vec<SessionServerEvent> = cancelled
            .into_iter()
            .map(|request_id| SessionServerEvent::PermissionResolved {
                request_id,
                approved: false,
            })
            .collect();
        state.append_to_log(resolved);
    }
    drop(client);
    Ok(())
}

fn install_replaced_client(
    state: &mut TaskSessionState,
    new_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
) -> Result<(), String> {
    let note = context_reset_note();
    if context_reset_needed(report.resumed, &state.log) && !already_noted(&state.log, &note) {
        state.append_to_log(vec![note]);
    }
    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(new_client.session_id()),
        &report.applied_model,
    );
    state.client = Some(new_client);
    state.model = model.to_string();
    state.applied_model = report.applied_model.clone();
    if let Some(options) = report.config_options.as_deref() {
        state.session_config_options = Some(config_option_descriptors(options));
    }
    if let Some(error) = &report.model_apply_error {
        state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    state.generation = state.generation.saturating_add(1);
    state.acp_alive = true;
    Ok(())
}

fn replace_resume_id(
    slot_model: &str,
    want_model: &str,
    state_dir: &Path,
    handle: &str,
) -> Option<String> {
    if slot_model == want_model {
        web_session_store::load::<SessionServerEvent>(state_dir, handle).acp_session_id
    } else {
        None
    }
}

async fn spawn_acp(
    agent: AgentClient,
    worktree_path: &Path,
    model: &str,
    resume_id: Option<&str>,
) -> Result<(AcpStdioClient, SpawnReport), String> {
    // Spawn on the session task thread so test ACP overrides (thread-local) and
    // the child's dedicated owner stay aligned. One task per session, so this
    // does not block the directory or other sessions.
    let worktree = worktree_path.to_path_buf();
    let resume = resume_id.map(str::to_string);
    tokio::task::block_in_place(|| {
        AcpStdioClient::spawn_with_operator_pin(agent, &worktree, model, resume.as_deref())
    })
}
