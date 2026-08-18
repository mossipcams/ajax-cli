//! ACP child spawn, replace, and first attach for per-task session slots.

use super::task_session::TaskSessionState;
use super::transcript::{
    already_noted, context_reset_needed, context_reset_note, slot_must_replace,
};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{AcpStdioClient, SpawnReport};
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
        let _ = client.cancel();
        state.acquire_holder();
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
        model,
    );
    state.client = Some(client);
    state.model = model.to_string();
    state.generation = 0;
    state.reset_holders_to_one();
    state.log = log;
    state.queued.clear();
    state.last_released = None;
    state.acp_alive = true;
    Ok(())
}

pub(super) async fn respawn(
    state: &mut TaskSessionState,
    worktree_path: &Path,
    model: &str,
    force: bool,
) -> Result<u64, String> {
    let Some(client) = state.client.as_mut() else {
        return Err("session slot missing".to_string());
    };
    let host_exited = client.host_exited();
    if !force && !slot_must_replace(state.acp_alive, &state.model, model, host_exited) {
        return Ok(state.generation);
    }
    let resume_id = replace_resume_id(
        &state.model,
        model,
        &state.state_dir,
        &state.qualified_handle,
    );
    let _ = client.cancel();
    let (new_client, report) =
        spawn_acp(state.agent, worktree_path, model, resume_id.as_deref()).await?;
    install_replaced_client(state, new_client, &report, model)?;
    Ok(state.generation)
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
        model,
    );
    state.client = Some(new_client);
    state.model = model.to_string();
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

fn spawn_model_arg(model: &str) -> Option<&str> {
    if model.is_empty() || model == "auto" {
        None
    } else {
        Some(model)
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
    let model = model.to_string();
    let resume = resume_id.map(str::to_string);
    tokio::task::block_in_place(|| {
        AcpStdioClient::spawn(agent, &worktree, spawn_model_arg(&model), resume.as_deref())
    })
}
