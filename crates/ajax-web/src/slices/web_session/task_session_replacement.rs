//! Transactional ACP child replacement after exit or intentional respawn.

use super::context_continuity::ContextContinuity;
use super::task_session::TaskSessionState;
use super::task_session_exit::{
    has_healthy_client, recover_prompt_ledger, try_dispatch_next_if_idle,
};
use super::transcript::{already_noted, context_reset_note};
use super::SessionError;
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    applied_model_id_for_persist, config_option_descriptors, AcpStdioClient, SpawnOutcome,
    SpawnReport,
};
use crate::adapters::web_session_store;

/// JSONL meta must store Ajax pipe-form (or catalog ids), not bare handshake bases ([#1079]).
pub(super) fn meta_model_for_persist(report: &SpawnReport, operator_model: &str) -> String {
    meta_model_from_config_options(report.config_options.as_deref(), operator_model)
}

pub(super) fn meta_model_from_config_options(
    config_options: Option<&[agent_client_protocol::schema::v1::SessionConfigOption]>,
    operator_model: &str,
) -> String {
    if let Some(options) = config_options {
        if let Ok(pipe) = applied_model_id_for_persist(options) {
            return pipe;
        }
    }
    operator_model.trim().to_string()
}

fn apply_spawn_capabilities(state: &mut TaskSessionState, report: &SpawnReport) {
    if let Some(options) = report.config_options.as_deref() {
        state.acp.session_config_options = Some(config_option_descriptors(options));
    }
    state.acp.session_prompt_capabilities = Some(report.prompt_capabilities.clone());
}

pub(super) fn discard_staged_client(mut client: AcpStdioClient) {
    if !client.host_exited() {
        let _ = client.cancel();
    }
    let _ = client.shutdown();
}

fn persist_client_identity(
    state: &TaskSessionState,
    session_id: &str,
    report: &SpawnReport,
    model: &str,
) -> Result<(), SessionError> {
    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(session_id),
        &meta_model_for_persist(report, model),
    )
    .map_err(|error| SessionError::persist(error.to_string()))
}

fn persist_new_context_identity(
    state: &TaskSessionState,
    session_id: &str,
    report: &SpawnReport,
    model: &str,
    context_epoch: u64,
) -> Result<(), SessionError> {
    web_session_store::save_meta_with_context_epoch(
        &state.state_dir,
        &state.qualified_handle,
        Some(session_id),
        &meta_model_for_persist(report, model),
        context_epoch,
    )
    .map_err(|error| SessionError::persist(error.to_string()))
}

fn append_context_reset_note_if_needed(state: &mut TaskSessionState, report: &SpawnReport) {
    let note = context_reset_note();
    if report.outcome.is_created()
        && !state.log.events.is_empty()
        && !already_noted(&state.log, &note)
    {
        let _ = state.append_to_log(vec![note]);
    }
}

pub(super) fn apply_context_from_spawn(state: &mut TaskSessionState, report: &SpawnReport) {
    state.context_continuity = match &report.outcome {
        SpawnOutcome::Created => ContextContinuity::live(state.context_continuity.epoch),
        SpawnOutcome::Restored { .. } => {
            ContextContinuity::restored(state.context_continuity.epoch)
        }
    };
}

pub(super) fn enter_restore_unavailable(
    state: &mut TaskSessionState,
    error: &str,
    model: &str,
) -> Result<(), SessionError> {
    state.acp.model = model.to_string();
    state.acp.client = None;
    state.acp.acp_alive = false;
    state.context_continuity =
        ContextContinuity::unavailable(state.context_continuity.epoch, error.to_string());
    if !state.log.events.is_empty() {
        state.generation = state.generation.saturating_add(1);
    }
    Ok(())
}

fn apply_installed_client_metadata(
    state: &mut TaskSessionState,
    report: &SpawnReport,
    model: &str,
    bump_generation: bool,
) {
    state.acp.model = model.to_string();
    state.acp.applied_model = report.applied_model.clone();
    apply_spawn_capabilities(state, report);
    if let Some(error) = &report.model_apply_error {
        let _ = state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    if bump_generation {
        state.generation = state.generation.saturating_add(1);
    }
    state.acp.acp_alive = true;
    state.prompts.child_exit_reconciled = false;
}

pub(super) fn install_replaced_client(
    state: &mut TaskSessionState,
    new_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
) -> Result<(), SessionError> {
    state.acp.acp_alive = false;
    apply_spawn_capabilities(state, report);
    match recover_prompt_ledger(state) {
        Ok(()) => {
            let session_id = new_client.session_id().to_string();
            if let Err(error) = persist_client_identity(state, &session_id, report, model) {
                discard_staged_client(new_client);
                enter_restore_unavailable(state, &error.to_string(), model)?;
                return Ok(());
            }
            append_context_reset_note_if_needed(state, report);
            apply_context_from_spawn(state, report);
            state.acp.client = Some(new_client);
            apply_installed_client_metadata(state, report, model, true);
            try_dispatch_next_if_idle(state);
            Ok(())
        }
        Err(error) => {
            discard_staged_client(new_client);
            state.acp.acp_alive = false;
            Err(error)
        }
    }
}

/// Stage a fresh ACP context: persist the new identity and epoch before install.
pub(super) fn install_new_context_client(
    state: &mut TaskSessionState,
    new_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
    post_install_note: Option<SessionServerEvent>,
    bump_generation: bool,
) -> Result<(), SessionError> {
    state.acp.acp_alive = false;
    apply_spawn_capabilities(state, report);
    match recover_prompt_ledger(state) {
        Ok(()) => {
            let new_epoch = state.context_continuity.epoch.saturating_add(1);
            let session_id = new_client.session_id().to_string();
            if let Err(error) =
                persist_new_context_identity(state, &session_id, report, model, new_epoch)
            {
                discard_staged_client(new_client);
                state.acp.acp_alive = false;
                return Err(error);
            }
            if let Some(note) = post_install_note {
                if !already_noted(&state.log, &note) {
                    let _ = state.append_to_log(vec![note]);
                }
            }
            state.context_continuity = ContextContinuity::live(new_epoch);
            state.acp.client = Some(new_client);
            apply_installed_client_metadata(state, report, model, bump_generation);
            try_dispatch_next_if_idle(state);
            Ok(())
        }
        Err(error) => {
            discard_staged_client(new_client);
            state.acp.acp_alive = false;
            Err(error)
        }
    }
}

pub(super) fn finish_first_acquire(
    state: &mut TaskSessionState,
    staged_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
) -> Result<(), SessionError> {
    let session_id = staged_client.session_id().to_string();
    if let Err(error) = persist_client_identity(state, &session_id, report, model) {
        discard_staged_client(staged_client);
        enter_restore_unavailable(state, &error.to_string(), model)?;
        state.acquire_holder();
        return Ok(());
    }
    append_context_reset_note_if_needed(state, report);
    apply_context_from_spawn(state, report);
    state.acp.client = Some(staged_client);
    apply_installed_client_metadata(state, report, model, false);
    state.reset_holders_to_one();
    try_dispatch_next_if_idle(state);
    debug_assert!(has_healthy_client(state));
    Ok(())
}
