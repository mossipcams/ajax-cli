use super::process_identity::foreign_live_child;
use super::session_storage::SessionStorage;
use super::task_session::{agent_client_name, TaskSessionState};
use super::task_session_exit::{
    interrupt_active_prompt, recover_prompt_ledger, retry_pending_exit_interruption,
};
use super::task_session_replacement::{
    discard_staged_client, finish_first_acquire, install_replaced_client, meta_model_for_persist,
    meta_model_from_config_options,
};
use super::transcript::{context_cleared_note, harness_switch_note, slot_must_replace};
use super::{apply_cancel_to_queue, SessionError, SessionServerEvent};
use crate::adapters::web_session_acp::{
    descriptors_from_neutral, persisted_model_id, triggers_model_persist, SessionClient,
    SessionConfigValue, SessionSpawnReport,
};
use crate::adapters::web_session_store::{self, StoredSession};
use ajax_core::adapters::{parse_model_selection, ModelSelection};
use ajax_core::models::AgentClient;
use std::path::Path;

fn apply_spawn_capabilities(state: &mut TaskSessionState, report: &SessionSpawnReport) {
    state.acp.clear_available_commands();
    state.acp.restore_advertised = report.restore_advertised;
    if let Some(options) = report.config_options.as_deref() {
        state.acp.session_config_options = Some(descriptors_from_neutral(options));
    }
    state.acp.session_prompt_capabilities = Some(report.prompt_capabilities.clone());
}

pub(super) async fn acquire(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<(), SessionError> {
    if let Err(message) = state.ownership.check() {
        return Err(SessionError::operator(message));
    }
    state.worktree_path = Some(worktree_path.to_path_buf());
    if state.acp.client.is_some() && state.agent != agent {
        reset_harness_context(state, worktree_path, model, agent).await?;
        state.acquire_holder();
        return Ok(());
    }
    state.agent = agent;

    if let Some(client) = state.acp.client.as_mut() {
        let host_exited = client.host_exited();
        if !slot_must_replace(state.acp.acp_alive, host_exited) {
            state.acquire_holder();
            return Ok(());
        }
        let resume_id = replace_resume_id(
            &state.storage,
            agent_client_name(state.agent),
            &state.qualified_handle,
        )?;
        release_live_client(state, resume_id.is_none(), "attach replaced a dead slot")?;
        reject_foreign_live_child(state)?;
        let (new_client, report) =
            spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await?;
        install_replaced_client(state, new_client, &report, model)?;
        state.acquire_holder();
        return Ok(());
    }

    state.acp.model = model.to_string();
    // Unreadable metadata must fail the attach, not start a fresh ACP context.
    let stored: StoredSession<SessionServerEvent> = web_session_store::try_load(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
    )
    .map_err(unreadable_session)?;
    state.evidence.transcript_corruption =
        web_session_store::corruption_warning(stored.corrupt_lines);
    let resume_id = stored
        .acp_session_id
        .as_deref()
        .filter(|_| stored.harness == agent_client_name(agent));
    reject_foreign_live_child(state)?;
    let (client, report) = spawn_acp(agent, worktree_path, model, resume_id).await?;

    state.acp.client = Some(client);
    if let Err(error) = state.record_child_identity() {
        if let Some(client) = state.acp.client.take() {
            discard_staged_client(client);
        }
        state.acp.acp_alive = false;
        return Err(error);
    }
    state.acp.model = model.to_string();
    state.acp.applied_model = report.applied_model.clone();
    apply_spawn_capabilities(state, &report);
    state.generation = 0;
    state.last_released = None;
    state.acp.acp_alive = false;
    match adopt_stored_session(state, stored.events, stored.dropped) {
        Ok(()) => finish_first_acquire(state, &report, model),
        Err(error) => {
            if let Some(client) = state.acp.client.take() {
                discard_staged_client(client);
            }
            state.acp.acp_alive = false;
            Err(error)
        }
    }
}

/// Adopts the stored transcript, normalizer seed and prompt ledger into a cold
/// actor. Acquire and Start fresh both go through here so neither appends to a
/// log that has not loaded the rows it would otherwise overwrite (#1432).
fn adopt_stored_session(
    state: &mut TaskSessionState,
    events: Vec<SessionServerEvent>,
    dropped: usize,
) -> Result<(), SessionError> {
    state.log = super::transcript::TranscriptLog::from_events(events, dropped);
    state.stream_normalizer = super::normalize::StreamNormalizer::seeded_from(&state.log.events);
    match recover_prompt_ledger(state) {
        Ok(()) => {
            state.stored_loaded = true;
            Ok(())
        }
        Err(error) => {
            // Stay cold so the next attempt loads the stored transcript again.
            state.log = super::transcript::TranscriptLog::default();
            state.stream_normalizer = super::normalize::StreamNormalizer::seeded_from(&[]);
            Err(error)
        }
    }
}

/// Start fresh on a cold actor: its first acquire failed before the stored
/// transcript loaded, so load it before anything is written to the log.
fn load_cold_session(state: &mut TaskSessionState, agent: AgentClient) -> Result<(), SessionError> {
    let stored = web_session_store::try_load::<SessionServerEvent>(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
    )
    .map_err(unreadable_session)?;
    state.evidence.transcript_corruption =
        web_session_store::corruption_warning(stored.corrupt_lines);
    adopt_stored_session(state, stored.events, stored.dropped)
}

pub(crate) struct ApplyConfigOptionResult {
    pub generation: u64,
    pub persist_model: Option<String>,
    pub persist_warning: Option<String>,
}

pub(super) async fn apply_config_option(
    state: &mut TaskSessionState,
    config_id: &str,
    value: SessionConfigValue,
) -> Result<ApplyConfigOptionResult, SessionError> {
    let Some(client) = state.acp.client.as_mut() else {
        return Err(SessionError::protocol("session slot missing"));
    };

    if client.host_exited() {
        let worktree_path = state
            .worktree_path
            .clone()
            .ok_or_else(|| SessionError::protocol("worktree path missing"))?;
        merge_config_into_desired_pin(state, config_id, &value);
        let model = state.acp.model.clone();
        let generation = respawn(state, &worktree_path, &model, true).await?;
        return Ok(ApplyConfigOptionResult {
            generation,
            persist_model: Some(model),
            persist_warning: None,
        });
    }

    let generation_before = state.generation;
    let apply_result = tokio::task::block_in_place(|| client.apply_config_option(config_id, value));
    match apply_result {
        Ok(outcome) if outcome.error.is_none() => {
            state.acp.applied_model = outcome.applied_model.clone();
            if let Some(options) = outcome.config_options.as_deref() {
                state.acp.session_config_options = Some(descriptors_from_neutral(options));
            }
            let persist_warning = web_session_store::try_save_meta(
                &state.storage.registry_store(),
                &state.storage.state_dir,
                &state.qualified_handle,
                agent_client_name(state.agent),
                Some(client.session_id()),
                &meta_model_from_config_options(
                    outcome.config_options.as_deref(),
                    &state.acp.model,
                ),
            )
            .err()
            .map(|error| format!("Model changed but session state was not saved — {error}"));
            state.acp.pending_model_snapshot = Some(outcome.applied_model);
            state.acp.pending_config_snapshot = state.acp.session_config_options.clone();
            let (persist_model, model_warning) = match outcome.config_options.as_deref() {
                Some(options) if triggers_model_persist(options, config_id) => {
                    match persisted_model_id(options) {
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
            if let Some(model) = persist_model.as_deref() {
                state.acp.model = model.to_string();
            }
            Ok(ApplyConfigOptionResult {
                generation: generation_before,
                persist_model,
                persist_warning: persist_warning.or(model_warning),
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
    if let Err(message) = state.ownership.check() {
        return Err(SessionError::operator(message));
    }
    if state.acp.client.is_none() {
        return Err(SessionError::protocol("session slot missing"));
    }
    let host_exited = state
        .acp
        .client
        .as_mut()
        .map(|client| client.host_exited())
        .unwrap_or(true);
    if !force && !slot_must_replace(state.acp.acp_alive, host_exited) {
        return Ok(state.generation);
    }
    let resume_id = replace_resume_id(
        &state.storage,
        agent_client_name(state.agent),
        &state.qualified_handle,
    )?;
    let agent = state.agent;
    release_live_client(state, resume_id.is_none(), "respawn")?;
    reject_foreign_live_child(state)?;
    let (new_client, report) = spawn_acp(agent, worktree_path, model, resume_id.as_deref()).await?;
    install_replaced_client(state, new_client, &report, model)?;
    Ok(state.generation)
}

pub(super) async fn reset_harness_context(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    agent: AgentClient,
) -> Result<u64, SessionError> {
    // Operator reset: retry transcript writes instead of staying blocked.
    state.evidence.transcript_durability_fault = None;
    state.commit_ledger_with_events(|ledger| ledger.remove_queued(), vec![])?;
    apply_cancel_to_queue(&mut state.prompts.queued, false);
    release_live_client(state, true, "harness switch")?;

    web_session_store::clear_acp_session_id(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
    )
    .map_err(|error| {
        SessionError::persist(format!("stored session id could not be cleared: {error}"))
    })?;

    reject_foreign_live_child(state)?;
    let (new_client, report) = spawn_acp(agent, worktree_path, model, None).await?;

    if let Err(error) = web_session_store::try_save_meta(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
        Some(new_client.session_id()),
        &meta_model_for_persist(&report, model),
    ) {
        discard_staged_client(new_client);
        return Err(SessionError::persist(format!(
            "new session id could not be saved: {error}"
        )));
    }

    let note = harness_switch_note(state.stream_normalizer.fresh_item_id());
    state.append_to_log(vec![note, SessionServerEvent::UsageReset])?;

    state.acp.client = Some(new_client);
    if let Err(error) = state.record_child_identity() {
        if let Some(client) = state.acp.client.take() {
            discard_staged_client(client);
        }
        state.acp.acp_alive = false;
        return Err(error);
    }
    state.acp.model = model.to_string();
    state.acp.applied_model = report.applied_model.clone();
    apply_spawn_capabilities(state, &report);
    state.agent = agent;
    state.generation = state.generation.saturating_add(1);
    state.acp.acp_alive = true;
    state.stream_normalizer = super::normalize::StreamNormalizer::seeded_from(&state.log.events);
    state.acp.usage_deduper = super::acp_usage::UsageDeduper::default();
    if let Some(error) = &report.model_apply_error {
        let _ = state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    Ok(state.generation)
}

pub(super) async fn clear_session_context(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    take_holder: bool,
) -> Result<u64, SessionError> {
    let model = state.acp.model.clone();
    let agent = state.agent;
    // Only the caller knows whether it still needs a holder: restore-recovery
    // Start fresh attaches afterwards, while an in-session /clear already runs
    // under the holder its attach took, and a second one would never be released.
    if !state.stored_loaded {
        load_cold_session(state, agent)?;
    }
    // Operator reset: retry transcript writes instead of staying blocked.
    state.evidence.transcript_durability_fault = None;
    state.commit_ledger_with_events(|ledger| ledger.remove_queued(), vec![])?;
    apply_cancel_to_queue(&mut state.prompts.queued, false);
    release_live_client(state, true, "clear context")?;

    web_session_store::clear_acp_session_id(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
    )
    .map_err(|error| {
        SessionError::persist(format!("stored session id could not be cleared: {error}"))
    })?;

    reject_foreign_live_child(state)?;
    let (new_client, report) = spawn_acp(agent, worktree_path, &model, None).await?;

    if let Err(error) = web_session_store::try_save_meta(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(agent),
        Some(new_client.session_id()),
        &meta_model_for_persist(&report, &model),
    ) {
        discard_staged_client(new_client);
        return Err(SessionError::persist(format!(
            "new session id could not be saved: {error}"
        )));
    }

    let note = context_cleared_note(state.stream_normalizer.fresh_item_id());
    state.append_to_log(vec![note, SessionServerEvent::UsageReset])?;

    state.acp.client = Some(new_client);
    if let Err(error) = state.record_child_identity() {
        if let Some(client) = state.acp.client.take() {
            discard_staged_client(client);
        }
        state.acp.acp_alive = false;
        return Err(error);
    }
    state.acp.applied_model = report.applied_model.clone();
    apply_spawn_capabilities(state, &report);
    state.generation = state.generation.saturating_add(1);
    state.acp.acp_alive = true;
    state.stream_normalizer = super::normalize::StreamNormalizer::seeded_from(&state.log.events);
    state.acp.usage_deduper = super::acp_usage::UsageDeduper::default();
    if let Some(error) = &report.model_apply_error {
        let _ = state.append_to_log(vec![SessionServerEvent::Error {
            message: error.clone(),
        }]);
    }
    if take_holder {
        state.acquire_holder();
    }
    Ok(state.generation)
}

fn release_live_client(
    state: &mut TaskSessionState,
    close_session: bool,
    reason: &'static str,
) -> Result<(), SessionError> {
    // This is the only place the host sends ACP `session/cancel` on its own.
    // Name the caller when it cuts a running turn short (#1189).
    if state.prompts.active_prompt.is_some() {
        tracing::warn!(
            handle = %state.qualified_handle,
            generation = state.generation,
            reason,
            "releasing the ACP child while a turn is in flight; the turn will be cancelled"
        );
    }
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
            if let Some(active) = state.prompts.active_prompt.as_ref() {
                let needs_turn_end = active.terminal.is_none();
                interrupt_active_prompt(state)?;
                if needs_turn_end {
                    state.append_to_log(vec![SessionServerEvent::TurnEnd {
                        stop_reason: Some("cancelled".to_string()),
                    }])?;
                }
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

fn unreadable_session(error: std::io::Error) -> SessionError {
    SessionError::persist(format!("stored session is unreadable: {error}"))
}

fn reject_foreign_live_child(state: &TaskSessionState) -> Result<(), SessionError> {
    let run = state
        .storage
        .registry_store()
        .latest_run_for_task(&state.qualified_handle)
        .map_err(|error| SessionError::persist(format!("run row could not be read: {error}")))?;
    let Some(run) = run else {
        return Ok(());
    };
    if foreign_live_child(
        run.child,
        run.child_started.as_deref(),
        run.owner_boot_id.as_deref(),
        state.ownership.boot_id(),
    )
    .is_some()
    {
        return Err(SessionError::persist(
            "previous host's agent for this chat is still shutting down; retry shortly",
        ));
    }
    Ok(())
}

fn replace_resume_id(
    storage: &SessionStorage,
    harness: &str,
    handle: &str,
) -> Result<Option<String>, SessionError> {
    web_session_store::try_load::<SessionServerEvent>(
        &storage.registry_store(),
        &storage.state_dir,
        handle,
        harness,
    )
    .map(|stored| stored.acp_session_id.filter(|_| stored.harness == harness))
    .map_err(unreadable_session)
}

async fn spawn_acp(
    agent: AgentClient,
    worktree_path: &Path,
    model: &str,
    resume_id: Option<&str>,
) -> Result<(SessionClient, SessionSpawnReport), SessionError> {
    let worktree = worktree_path.to_path_buf();
    let resume = resume_id.map(str::to_string);
    tokio::task::block_in_place(|| {
        SessionClient::spawn_with_operator_pin(agent, &worktree, model, resume.as_deref())
            .map_err(SessionError::classify_spawn)
    })
}

fn merge_config_into_desired_pin(
    state: &mut TaskSessionState,
    config_id: &str,
    value: &SessionConfigValue,
) {
    let wire = match value {
        SessionConfigValue::Select(value) => value.clone(),
        SessionConfigValue::Boolean(value) => value.to_string(),
    };
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
    } else if config_id == "model" {
        state.acp.model = wire;
    } else {
        state.acp.model = ModelSelection {
            model: state.acp.model.trim().to_string(),
            options: vec![(config_id.to_string(), wire)],
        }
        .encode();
    }
}
