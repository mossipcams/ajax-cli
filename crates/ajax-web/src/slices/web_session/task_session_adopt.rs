use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::Path;

use crate::adapters::session_shim::shim_paths;
use crate::adapters::web_session_acp::{SessionClient, ADOPTED_PROMPT_REQUEST_ID};
use crate::adapters::web_session_store::prompt_ledger::{self as ledger_module, PromptPhase};
use crate::adapters::web_session_store::{self, StoredSession};

use super::normalize::StreamNormalizer;
use super::prompt_queue::ActivePrompt;
use super::shim_adoption::{adoptable, Adoptable};
use super::startup_recovery::interrupted_by_restart_events;
use super::task_session::{agent_client_name, TaskSessionState};
use super::task_session_exit::restore_queued_prompts;
use super::transcript::TranscriptLog;
use super::{SessionError, SessionServerEvent};

pub(super) fn try_adopt(state: &mut TaskSessionState) -> Result<bool, SessionError> {
    let store = state.storage.registry_store();
    let Some(adoptable) = adoptable(&store, &state.storage.state_dir, &state.qualified_handle)
    else {
        return Ok(false);
    };
    if let Err(error) = adopt(state, &adoptable) {
        tracing::warn!(
            handle = %state.qualified_handle,
            %error,
            "live session shim could not be adopted; interrupting its prompt"
        );
        send_shutdown(&state.storage.state_dir, &adoptable.worktree);
        interrupt_adopted_prompt(state, &adoptable.client_message_id)?;
        return Ok(false);
    }
    Ok(true)
}

fn adopt(state: &mut TaskSessionState, adoptable: &Adoptable) -> Result<(), SessionError> {
    let store = state.storage.registry_store();
    let harness = agent_client_name(state.agent);
    let stored = load_stored(state)?;
    let session_id = stored
        .acp_session_id
        .clone()
        .filter(|_| stored.harness == harness)
        .ok_or_else(|| SessionError::persist("live session shim has no stored session id"))?;
    let shim_pid = shim_pid(state)?;

    let stream = attach_stream(
        &state.storage.state_dir,
        &adoptable.worktree,
        adoptable.shim_offset,
    )?;
    let client = SessionClient::adopt(state.agent, stream, shim_pid, session_id)
        .map_err(SessionError::protocol)?;

    web_session_store::try_truncate_events_after(
        &store,
        &state.qualified_handle,
        adoptable.transcript_cursor,
    )
    .map_err(|error| {
        SessionError::persist(format!("transcript could not be rolled back: {error}"))
    })?;
    let stored = load_stored(state)?;
    state.evidence.transcript_corruption =
        web_session_store::corruption_warning(stored.corrupt_lines);
    state.log = TranscriptLog::from_events(stored.events, stored.dropped);
    state.stream_normalizer = StreamNormalizer::seeded_from(&state.log.events);

    let ledger = ledger_module::try_load(&store, &state.storage.state_dir, &state.qualified_handle)
        .map_err(|error| SessionError::persist(error.to_string()))?;
    let queued = ledger
        .entries
        .iter()
        .filter(|entry| entry.phase == PromptPhase::Queued)
        .cloned()
        .collect();
    restore_queued_prompts(state, queued)?;

    state.acp.client = Some(client);
    if let Err(error) = state.record_child_identity() {
        state.acp.client = None;
        return Err(error);
    }

    state.prompts.prompt_ledger = ledger;
    state.prompts.ledger_unusable = None;
    state.prompts.active_prompt = Some(ActivePrompt::new(
        ADOPTED_PROMPT_REQUEST_ID,
        Some(adoptable.client_message_id.clone()),
    ));
    state.stored_loaded = true;
    state.acp.acp_alive = true;
    Ok(())
}

fn load_stored(
    state: &TaskSessionState,
) -> Result<StoredSession<SessionServerEvent>, SessionError> {
    web_session_store::try_load::<SessionServerEvent>(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
        agent_client_name(state.agent),
    )
    .map_err(|error| SessionError::persist(format!("stored session is unreadable: {error}")))
}

fn shim_pid(state: &TaskSessionState) -> Result<u32, SessionError> {
    let run = state
        .storage
        .registry_store()
        .latest_run_for_task(&state.qualified_handle)
        .map_err(|error| SessionError::persist(format!("run row could not be read: {error}")))?;
    run.and_then(|run| run.child)
        .and_then(|(pid, _)| u32::try_from(pid).ok())
        .ok_or_else(|| SessionError::persist("live session shim has no recorded pid"))
}

#[cfg(test)]
pub(crate) static REFUSED_ATTACH_FOR_TEST: std::sync::Mutex<Vec<String>> =
    std::sync::Mutex::new(Vec::new());

fn attach_stream(
    state_dir: &Path,
    worktree: &Path,
    shim_offset: u64,
) -> Result<UnixStream, SessionError> {
    let handle = worktree
        .to_str()
        .ok_or_else(|| SessionError::protocol("worktree path is not valid UTF-8"))?;
    #[cfg(test)]
    if REFUSED_ATTACH_FOR_TEST
        .lock()
        .expect("refusal list lock")
        .iter()
        .any(|refused| refused == handle)
    {
        return Err(SessionError::protocol(
            "session shim is unreachable: refused by test",
        ));
    }
    let mut stream = UnixStream::connect(shim_paths(state_dir, handle).socket)
        .map_err(|error| SessionError::protocol(format!("session shim is unreachable: {error}")))?;
    stream
        .write_all(format!("REPLAY {shim_offset}\n").as_bytes())
        .map_err(|error| SessionError::protocol(format!("session shim replay failed: {error}")))?;
    Ok(stream)
}

pub(super) fn release_finished_adopted_client(state: &mut TaskSessionState) {
    if state.prompts.active_prompt.is_some() {
        return;
    }
    let finished = state
        .acp
        .client
        .as_ref()
        .is_some_and(|client| client.is_adopted() && !client.prompt_in_flight());
    if !finished {
        return;
    }
    if let Some(mut client) = state.acp.client.take() {
        let _ = client.shutdown();
    }
    state.acp.acp_alive = false;
}

fn send_shutdown(state_dir: &Path, worktree: &Path) {
    let Some(handle) = worktree.to_str() else {
        return;
    };
    if let Ok(mut stream) = UnixStream::connect(shim_paths(state_dir, handle).socket) {
        let _ = stream.write_all(b"REPLAY 0\nSHUTDOWN\n");
    }
}

fn interrupt_adopted_prompt(
    state: &mut TaskSessionState,
    client_message_id: &str,
) -> Result<(), SessionError> {
    state.prompts.prompt_ledger = ledger_module::try_load(
        &state.storage.registry_store(),
        &state.storage.state_dir,
        &state.qualified_handle,
    )
    .map_err(|error| SessionError::persist(error.to_string()))?;
    state.commit_ledger_with_events(
        |ledger| {
            ledger.mark_interrupted(client_message_id);
        },
        interrupted_by_restart_events(client_message_id),
    )
}
