//! ACP child spawn, replace, and first attach for per-task session slots.

use super::context_continuity::ContextState;
use super::task_session::TaskSessionState;
use super::task_session_exit::{
    interrupt_active_prompt, recover_prompt_ledger, retry_pending_exit_interruption,
};
use super::task_session_replacement::{
    discard_staged_client, enter_restore_unavailable, finish_first_acquire,
    install_new_context_client, install_replaced_client, meta_model_from_config_options,
};
use super::transcript::{context_reset_note, harness_switch_note, slot_must_replace};
use super::{apply_cancel_to_queue, SessionServerEvent};
use crate::adapters::web_session_acp::{
    applied_model_id_for_persist, config_option_descriptors, is_restore_unavailable,
    option_triggers_model_persist, AcpStdioClient, SpawnReport,
};
use crate::adapters::web_session_store::{self, StoredSession};
use ajax_core::models::AgentClient;
use std::path::Path;

fn apply_spawn_capabilities(state: &mut TaskSessionState, report: &SpawnReport) {
    if let Some(options) = report.config_options.as_deref() {
        state.session_config_options = Some(config_option_descriptors(options));
    }
    state.session_prompt_capabilities = Some(report.prompt_capabilities.clone());
}

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
        release_live_client(state, resume_id.is_none())?;
        match spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await {
            Ok((new_client, report)) => {
                install_replaced_client(state, new_client, &report, model)?;
            }
            Err(error) if is_restore_unavailable(&error) => {
                enter_restore_unavailable(state, &error, model)?;
            }
            Err(error) => return Err(error),
        }
        state.acquire_holder();
        return Ok(());
    }

    let stored: StoredSession<SessionServerEvent> =
        web_session_store::load(&state.state_dir, &state.qualified_handle);
    state.context_continuity.epoch = stored.context_epoch;
    let resume_id = stored.acp_session_id.clone();
    match spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await {
        Ok((client, report)) => {
            state.model = model.to_string();
            state.applied_model = report.applied_model.clone();
            apply_spawn_capabilities(state, &report);
            state.log =
                super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
            state.generation = 0;
            state.last_released = None;
            state.acp_alive = false;
            match recover_prompt_ledger(state) {
                Ok(()) => finish_first_acquire(state, client, &report, model),
                Err(error) => {
                    discard_staged_client(client);
                    state.acp_alive = false;
                    Err(error)
                }
            }
        }
        Err(error) if is_restore_unavailable(&error) => {
            state.log =
                super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
            state.model = model.to_string();
            state.applied_model = if stored.model.is_empty() {
                model.to_string()
            } else {
                stored.model.clone()
            };
            state.generation = 0;
            state.last_released = None;
            match recover_prompt_ledger(state) {
                Ok(()) => {
                    enter_restore_unavailable(state, &error, model)?;
                    state.acquire_holder();
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
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
            let _ = web_session_store::save_meta(
                &state.state_dir,
                &state.qualified_handle,
                Some(client.session_id()),
                &meta_model_from_config_options(outcome.config_options.as_deref(), model),
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
            let _ = state.append_to_log(vec![SessionServerEvent::Error {
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
            let _ = web_session_store::save_meta(
                &state.state_dir,
                &state.qualified_handle,
                Some(client.session_id()),
                &meta_model_from_config_options(outcome.config_options.as_deref(), &state.model),
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
            let _ = state.append_to_log(vec![SessionServerEvent::Error {
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
    release_live_client(state, resume_id.is_none())?;
    match spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await {
        Ok((new_client, report)) => install_replaced_client(state, new_client, &report, model)?,
        Err(error) if is_restore_unavailable(&error) => {
            enter_restore_unavailable(state, &error, model)?;
        }
        Err(error) => return Err(error),
    }
    Ok(state.generation)
}

pub(super) async fn reset_harness_context(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<u64, String> {
    release_live_client(state, true)?;
    apply_cancel_to_queue(&mut state.queued, false);
    state.prompt_ledger.remove_queued();
    let _ = web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &state.prompt_ledger,
    );

    let (new_client, report) = spawn_acp(agent, worktree_path, model, None).await?;
    let note = harness_switch_note(state.stream_normalizer.fresh_item_id());
    install_new_context_client(state, new_client, &report, model, Some(note), true)?;

    state.agent = agent;
    state.stream_normalizer = super::normalize::StreamNormalizer::default();
    state.usage_deduper = super::acp_usage::UsageDeduper::default();
    Ok(state.generation)
}

pub(super) async fn start_new_context(state: &mut TaskSessionState) -> Result<(), String> {
    if !matches!(state.context_continuity.state, ContextState::Unavailable) {
        return Err("ACP context is not unavailable".to_string());
    }
    let worktree_path = state
        .worktree_path
        .as_deref()
        .ok_or_else(|| "worktree path missing".to_string())?;
    let model = state.model.clone();
    let agent = state.agent;
    let (new_client, report) = spawn_acp(agent, worktree_path, &model, None).await?;
    let note = context_reset_note();
    install_new_context_client(state, new_client, &report, &model, Some(note), true)
}

pub(super) async fn retry_restore(state: &mut TaskSessionState) -> Result<(), String> {
    if !matches!(state.context_continuity.state, ContextState::Unavailable) {
        return Err("ACP context is not unavailable".to_string());
    }
    let worktree_path = state
        .worktree_path
        .as_deref()
        .ok_or_else(|| "worktree path missing".to_string())?;
    let model = state.model.clone();
    let agent = state.agent;
    let stored =
        web_session_store::load::<SessionServerEvent>(&state.state_dir, &state.qualified_handle);
    let resume_id = stored
        .acp_session_id
        .as_deref()
        .ok_or_else(|| "no stored ACP session id to restore".to_string())?;
    match spawn_acp(agent, worktree_path, &model, Some(resume_id)).await {
        Ok((new_client, report)) => install_replaced_client(state, new_client, &report, &model),
        Err(error) if is_restore_unavailable(&error) => {
            enter_restore_unavailable(state, &error, &model)?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// Cancel and drop the live ACP child so the next spawn owns stdio alone.
fn release_live_client(state: &mut TaskSessionState, close_session: bool) -> Result<(), String> {
    retry_pending_exit_interruption(state);
    if state.pending_exit_interruption.is_some() {
        return Err("prompt ownership recovery pending".to_string());
    }
    state.suppress_exit_evidence = true;
    let result = (|| {
        interrupt_active_prompt(state)?;
        let Some(mut client) = state.client.take() else {
            return Ok(());
        };
        if !client.host_exited() {
            let cancelled = client.cancel()?;
            let mut resolved = Vec::new();
            for request_id in cancelled.permissions {
                resolved.push(SessionServerEvent::PermissionResolved {
                    request_id,
                    approved: false,
                });
            }
            for request_id in cancelled.elicitations {
                resolved.push(SessionServerEvent::ElicitationResolved {
                    request_id,
                    action: "cancel".to_string(),
                });
            }
            let _ = state.append_to_log(resolved);
        }
        let message = if close_session {
            client.shutdown()
        } else {
            client.detach()
        };
        if let Some(message) = message {
            let _ = state.append_to_log(vec![SessionServerEvent::Error { message }]);
        }
        Ok(())
    })();
    state.suppress_exit_evidence = false;
    if result.is_ok() {
        state.child_exit_reconciled = true;
        state.acp_alive = false;
    }
    result
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
