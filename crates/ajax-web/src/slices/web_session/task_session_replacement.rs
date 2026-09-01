//! Transactional ACP child replacement after exit or intentional respawn.

use super::task_session::TaskSessionState;
use super::task_session_exit::{
    has_healthy_client, recover_prompt_ledger, try_dispatch_next_if_idle,
};
use super::transcript::{already_noted, context_reset_needed, context_reset_note};
use super::{SessionError, SessionServerEvent};
use crate::adapters::web_session_acp::{
    applied_model_id_for_persist, config_option_descriptors, AcpStdioClient, SpawnReport,
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

pub(super) fn finalize_client_metadata(
    state: &mut TaskSessionState,
    session_id: &str,
    report: &SpawnReport,
    model: &str,
    bump_generation: bool,
) -> Result<(), SessionError> {
    let note = context_reset_note();
    if context_reset_needed(report.resumed, &state.log) && !already_noted(&state.log, &note) {
        let _ = state.append_to_log(vec![note]);
    }
    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(session_id),
        &meta_model_for_persist(report, model),
    );
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
    Ok(())
}

pub(super) fn install_replaced_client(
    state: &mut TaskSessionState,
    new_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
) -> Result<(), SessionError> {
    state.acp.client = Some(new_client);
    state.acp.acp_alive = false;
    match recover_prompt_ledger(state) {
        Ok(()) => {
            let session_id = state
                .acp
                .client
                .as_ref()
                .expect("staged replacement client")
                .session_id()
                .to_string();
            finalize_client_metadata(state, &session_id, report, model, true)?;
            try_dispatch_next_if_idle(state);
            Ok(())
        }
        Err(error) => {
            if let Some(client) = state.acp.client.take() {
                discard_staged_client(client);
            }
            state.acp.acp_alive = false;
            Err(error)
        }
    }
}

pub(super) fn finish_first_acquire(
    state: &mut TaskSessionState,
    report: &SpawnReport,
    model: &str,
) -> Result<(), SessionError> {
    let session_id = state
        .acp
        .client
        .as_ref()
        .expect("acquired client")
        .session_id()
        .to_string();
    finalize_client_metadata(state, &session_id, report, model, false)?;
    state.reset_holders_to_one();
    try_dispatch_next_if_idle(state);
    debug_assert!(has_healthy_client(state));
    Ok(())
}
