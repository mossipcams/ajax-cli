//! Transactional ACP child replacement after exit or intentional respawn.

use super::task_session::TaskSessionState;
use super::task_session_exit::{
    has_healthy_client, recover_prompt_ledger, try_dispatch_next_if_idle,
};
use super::transcript::{already_noted, context_reset_needed, context_reset_note};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{config_option_descriptors, AcpStdioClient, SpawnReport};
use crate::adapters::web_session_store;

fn apply_spawn_capabilities(state: &mut TaskSessionState, report: &SpawnReport) {
    if let Some(options) = report.config_options.as_deref() {
        state.session_config_options = Some(config_option_descriptors(options));
    }
    state.session_prompt_capabilities = Some(report.prompt_capabilities.clone());
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
) {
    let note = context_reset_note();
    if context_reset_needed(report.resumed, &state.log) && !already_noted(&state.log, &note) {
        state.append_to_log(vec![note]);
    }
    web_session_store::save_meta(
        &state.state_dir,
        &state.qualified_handle,
        Some(session_id),
        &report.applied_model,
    );
    state.model = model.to_string();
    state.applied_model = report.applied_model.clone();
    apply_spawn_capabilities(state, report);
    if let Some(error) = &report.model_apply_error {
        state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    if bump_generation {
        state.generation = state.generation.saturating_add(1);
    }
    state.acp_alive = true;
    state.child_exit_reconciled = false;
}

pub(super) fn install_replaced_client(
    state: &mut TaskSessionState,
    new_client: AcpStdioClient,
    report: &SpawnReport,
    model: &str,
) -> Result<(), String> {
    state.client = Some(new_client);
    state.acp_alive = false;
    match recover_prompt_ledger(state) {
        Ok(()) => {
            let session_id = state
                .client
                .as_ref()
                .expect("staged replacement client")
                .session_id()
                .to_string();
            finalize_client_metadata(state, &session_id, report, model, true);
            try_dispatch_next_if_idle(state);
            Ok(())
        }
        Err(error) => {
            if let Some(client) = state.client.take() {
                discard_staged_client(client);
            }
            state.acp_alive = false;
            Err(error)
        }
    }
}

pub(super) fn finish_first_acquire(
    state: &mut TaskSessionState,
    report: &SpawnReport,
    model: &str,
) -> Result<(), String> {
    let session_id = state
        .client
        .as_ref()
        .expect("acquired client")
        .session_id()
        .to_string();
    finalize_client_metadata(state, &session_id, report, model, false);
    state.reset_holders_to_one();
    try_dispatch_next_if_idle(state);
    debug_assert!(has_healthy_client(state));
    Ok(())
}
