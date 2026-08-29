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
use super::{apply_cancel_to_queue, SessionError, SessionServerEvent};
use crate::adapters::web_session_acp::{
    applied_model_id_for_persist, config_option_descriptors, option_triggers_model_persist,
    AcpStdioClient, SpawnReport,
};
use crate::adapters::web_session_store::{self, StoredSession};
use agent_client_protocol::schema::v1::SessionConfigOptionValue;
use ajax_core::adapters::{parse_model_selection, ModelSelection};
use ajax_core::models::AgentClient;
use std::path::Path;

fn apply_spawn_capabilities(state: &mut TaskSessionState, report: &SpawnReport) {
    if let Some(options) = report.config_options.as_deref() {
        state.acp.session_config_options = Some(config_option_descriptors(options));
    }
    state.acp.session_prompt_capabilities = Some(report.prompt_capabilities.clone());
}

pub(super) async fn acquire(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<(), SessionError> {
    state.worktree_path = Some(worktree_path.to_path_buf());
    state.agent = agent;

    if let Some(client) = state.acp.client.as_mut() {
        let host_exited = client.host_exited();
        if !slot_must_replace(state.acp.acp_alive, &state.acp.model, model, host_exited) {
            state.acquire_holder();
            return Ok(());
        }
        let resume_id = replace_resume_id(
            &state.acp.model,
            model,
            &state.state_dir,
            &state.qualified_handle,
        );
        release_live_client(state, resume_id.is_none())?;
        match spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await {
            Ok((new_client, report)) => {
                install_replaced_client(state, new_client, &report, model)?;
            }
            Err(error) if error.is_restore_unavailable() => {
                enter_restore_unavailable(state, &error.to_string(), model)?;
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
            state.acp.model = model.to_string();
            state.acp.applied_model = report.applied_model.clone();
            apply_spawn_capabilities(state, &report);
            state.log =
                super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
            state.generation = 0;
            state.last_released = None;
            state.acp.acp_alive = false;
            match recover_prompt_ledger(state) {
                Ok(()) => finish_first_acquire(state, client, &report, model),
                Err(error) => {
                    discard_staged_client(client);
                    state.acp.acp_alive = false;
                    Err(error)
                }
            }
        }
        Err(error) if error.is_restore_unavailable() => {
            state.log =
                super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
            state.acp.model = model.to_string();
            state.acp.applied_model = if stored.model.is_empty() {
                model.to_string()
            } else {
                stored.model.clone()
            };
            state.generation = 0;
            state.last_released = None;
            match recover_prompt_ledger(state) {
                Ok(()) => {
                    enter_restore_unavailable(state, &error.to_string(), model)?;
                    state.acquire_holder();
                    Ok(())
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

fn merge_config_into_desired_pin(
    state: &mut TaskSessionState,
    config_id: &str,
    value: &SessionConfigOptionValue,
) -> Result<(), SessionError> {
    let wire = config_option_wire_token(value);
    if let Some(mut selection) = parse_model_selection(state.acp.model.trim()) {
        if config_id == "model" {
            selection.model = wire;
        } else if let Some(pair) = selection
            .options
            .iter_mut()
            .find(|(key, _)| key == config_id)
        {
            pair.1 = wire;
        } else {
            selection.options.push((config_id.to_string(), wire));
        }
        state.acp.model = selection.encode();
        return Ok(());
    }
    if config_id == "model" {
        state.acp.model = wire;
        return Ok(());
    }
    state.acp.model = ModelSelection {
        model: state.acp.model.trim().to_string(),
        options: vec![(config_id.to_string(), wire)],
    }
    .encode();
    Ok(())
}

fn config_option_wire_token(value: &SessionConfigOptionValue) -> String {
    match value {
        SessionConfigOptionValue::ValueId { value } => value.0.to_string(),
        SessionConfigOptionValue::Boolean { value } => value.to_string(),
        _ => String::new(),
    }
}

fn persist_model_from_config_apply(
    config_options: Option<&[agent_client_protocol::schema::v1::SessionConfigOption]>,
    config_id: &str,
) -> (Option<String>, Option<String>) {
    match config_options {
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
    value: SessionConfigOptionValue,
) -> Result<ApplyConfigOptionResult, SessionError> {
    let Some(client) = state.acp.client.as_mut() else {
        merge_config_into_desired_pin(state, config_id, &value)?;
        return Ok(ApplyConfigOptionResult {
            generation: state.generation,
            persist_model: None,
            persist_warning: None,
        });
    };

    if client.host_exited() {
        let worktree_path = state
            .worktree_path
            .clone()
            .ok_or_else(|| SessionError::protocol("worktree path missing"))?;
        merge_config_into_desired_pin(state, config_id, &value)?;
        let model = state.acp.model.clone();
        let generation = respawn(state, &worktree_path, &model, true).await?;
        return Ok(ApplyConfigOptionResult {
            generation,
            persist_model: Some(state.acp.model.clone()),
            persist_warning: None,
        });
    }

    let generation_before = state.generation;
    let apply_result = tokio::task::block_in_place(|| client.apply_config_option(config_id, value));
    match apply_result {
        Ok(outcome) if outcome.error.is_none() => {
            state.acp.applied_model = outcome.applied_model.clone();
            if let Some(options) = outcome.config_options.as_deref() {
                state.acp.session_config_options = Some(config_option_descriptors(options));
            }
            let _ = web_session_store::save_meta(
                &state.state_dir,
                &state.qualified_handle,
                Some(client.session_id()),
                &meta_model_from_config_options(
                    outcome.config_options.as_deref(),
                    &state.acp.model,
                ),
            );
            state.acp.pending_model_snapshot = Some(outcome.applied_model);
            state.acp.pending_config_snapshot = state.acp.session_config_options.clone();
            let (persist_model, persist_warning) =
                persist_model_from_config_apply(outcome.config_options.as_deref(), config_id);
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
            Err(SessionError::protocol(message))
        }
        Err(error) => Err(SessionError::protocol(error)),
    }
}

pub(super) async fn respawn(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    force: bool,
) -> Result<u64, SessionError> {
    if state.acp.client.is_none() {
        return Err(SessionError::protocol("session slot missing"));
    }
    let host_exited = state
        .acp
        .client
        .as_mut()
        .map(|client| client.host_exited())
        .unwrap_or(true);
    if !force && !slot_must_replace(state.acp.acp_alive, &state.acp.model, model, host_exited) {
        return Ok(state.generation);
    }
    let resume_id = replace_resume_id(
        &state.acp.model,
        model,
        &state.state_dir,
        &state.qualified_handle,
    );
    let agent = state.agent;
    release_live_client(state, resume_id.is_none())?;
    match spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await {
        Ok((new_client, report)) => install_replaced_client(state, new_client, &report, model)?,
        Err(error) if error.is_restore_unavailable() => {
            enter_restore_unavailable(state, &error.to_string(), model)?;
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
) -> Result<u64, SessionError> {
    release_live_client(state, true)?;
    apply_cancel_to_queue(&mut state.prompts.queued, false);
    state.prompts.prompt_ledger.remove_queued();
    let _ = web_session_store::prompt_ledger::persist(
        &state.state_dir,
        &state.qualified_handle,
        &state.prompts.prompt_ledger,
    );

    let (new_client, report) = spawn_acp(agent, worktree_path, model, None).await?;
    let note = harness_switch_note(state.stream_normalizer.fresh_item_id());
    install_new_context_client(state, new_client, &report, model, Some(note), true)?;

    state.agent = agent;
    state.stream_normalizer = super::normalize::StreamNormalizer::default();
    state.acp.usage_deduper = super::acp_usage::UsageDeduper::default();
    Ok(state.generation)
}

pub(super) async fn start_new_context(state: &mut TaskSessionState) -> Result<(), SessionError> {
    if !matches!(state.context_continuity.state, ContextState::Unavailable) {
        return Err(SessionError::protocol("ACP context is not unavailable"));
    }
    let worktree_path = state
        .worktree_path
        .as_deref()
        .ok_or_else(|| SessionError::protocol("worktree path missing"))?;
    let model = state.acp.model.clone();
    let agent = state.agent;
    let (new_client, report) = spawn_acp(agent, worktree_path, &model, None).await?;
    let note = context_reset_note();
    install_new_context_client(state, new_client, &report, &model, Some(note), true)
}

pub(super) async fn retry_restore(state: &mut TaskSessionState) -> Result<(), SessionError> {
    if !matches!(state.context_continuity.state, ContextState::Unavailable) {
        return Err(SessionError::protocol("ACP context is not unavailable"));
    }
    let worktree_path = state
        .worktree_path
        .as_deref()
        .ok_or_else(|| SessionError::protocol("worktree path missing"))?;
    let model = state.acp.model.clone();
    let agent = state.agent;
    let stored =
        web_session_store::load::<SessionServerEvent>(&state.state_dir, &state.qualified_handle);
    let resume_id = stored
        .acp_session_id
        .as_deref()
        .ok_or_else(|| SessionError::protocol("no stored ACP session id to restore"))?;
    match spawn_acp(agent, worktree_path, &model, Some(resume_id)).await {
        Ok((new_client, report)) => install_replaced_client(state, new_client, &report, &model),
        Err(error) if error.is_restore_unavailable() => {
            enter_restore_unavailable(state, &error.to_string(), &model)?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// Cancel and drop the live ACP child so the next spawn owns stdio alone.
fn release_live_client(
    state: &mut TaskSessionState,
    close_session: bool,
) -> Result<(), SessionError> {
    retry_pending_exit_interruption(state);
    if state.prompts.pending_exit_interruption.is_some() {
        return Err(SessionError::persist("prompt ownership recovery pending"));
    }
    state.prompts.suppress_exit_evidence = true;
    let result = (|| {
        if let Some(active) = state.prompts.active_prompt.as_mut() {
            active.mark_cancel_requested();
        }
        let awaiting_cancel_terminal = state
            .prompts
            .active_prompt
            .as_ref()
            .is_some_and(|active| active.terminal.is_none());
        if !awaiting_cancel_terminal {
            interrupt_active_prompt(state)?;
        }
        let Some(client) = state.acp.client.as_mut() else {
            return Ok(());
        };
        if !client.host_exited() {
            let cancelled = client.cancel().map_err(SessionError::protocol)?;
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
        if awaiting_cancel_terminal {
            state.pump();
            if state.prompts.active_prompt.is_some() {
                interrupt_active_prompt(state)?;
            }
        }
        let Some(mut client) = state.acp.client.take() else {
            return Ok(());
        };
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
    state.prompts.suppress_exit_evidence = false;
    if result.is_ok() {
        state.prompts.child_exit_reconciled = true;
        state.acp.acp_alive = false;
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
) -> Result<(AcpStdioClient, SpawnReport), SessionError> {
    // Spawn on the session task thread so test ACP overrides (thread-local) and
    // the child's dedicated owner stay aligned. One task per session, so this
    // does not block the directory or other sessions.
    let worktree = worktree_path.to_path_buf();
    let resume = resume_id.map(str::to_string);
    tokio::task::block_in_place(|| {
        AcpStdioClient::spawn_with_operator_pin(agent, &worktree, model, resume.as_deref())
            .map_err(|error| SessionError::classify_spawn(&error))
    })
}
