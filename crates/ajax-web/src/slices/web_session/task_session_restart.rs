use super::prompt_content::PromptContentBlockWire;
use super::task_session::{TaskSessionCommand, TaskSessionState};
use super::task_session_directory::send_command;
use super::task_session_exit::{submit_prompt, try_dispatch_next_if_idle};
use super::task_session_spawn;
use super::{SessionActivity, SessionError, SessionServerEvent, TaskSessionDirectory};
use crate::adapters::web_session_store::prompt_ledger::PromptPhase;
use std::path::Path;

impl TaskSessionDirectory {
    pub(super) async fn restart_agent(
        &self,
        handle: &str,
        worktree_path: &Path,
    ) -> Result<bool, String> {
        if !worktree_path.exists() {
            return Err("worktree missing".to_string());
        }
        let tx = self.command_tx(handle)?;
        let worktree_path = worktree_path.to_path_buf();
        send_command(&tx, |reply| TaskSessionCommand::Restart {
            worktree_path,
            reply,
        })
        .await?
        .map_err(Into::into)
    }
}

struct InFlightPrompt {
    client_message_id: String,
    prompt_text: String,
    content_blocks: Vec<PromptContentBlockWire>,
}

pub(super) async fn restart_child(
    state: &mut TaskSessionState,
    worktree_path: &Path,
) -> Result<bool, SessionError> {
    let in_flight = in_flight_prompt(state);
    if state.acp.client.is_some() {
        let model = state.acp.model.clone();
        task_session_spawn::respawn(state, worktree_path, &model, true).await?;
    } else {
        let model = state.acp.model.clone();
        let agent = state.agent;
        let holders = state.holders.0;
        let acquired = task_session_spawn::acquire(state, worktree_path, &model, agent).await;
        state.holders.0 = holders;
        acquired?;
    }

    if let Some(prompt) = &in_flight {
        if phase(state, &prompt.client_message_id) == Some(PromptPhase::Dispatching) {
            let client_message_id = prompt.client_message_id.clone();
            state.commit_ledger_with_events(
                |ledger| {
                    ledger.mark_interrupted(&client_message_id);
                },
                Vec::new(),
            )?;
        }
    }
    let resend = in_flight
        .filter(|prompt| phase(state, &prompt.client_message_id) == Some(PromptPhase::Interrupted));
    let resent = resend.is_some();

    let mut notes = vec![host_note("Agent restarted.", state)];
    if resent {
        notes.push(host_note("Re-sent your last prompt.", state));
    }
    state.append_to_log(notes)?;

    try_dispatch_next_if_idle(state);
    if let Some(prompt) = resend {
        submit_prompt(
            state,
            format!("{}-restart", prompt.client_message_id),
            prompt.prompt_text,
            prompt.content_blocks,
        )?;
    }

    let activity = if resent || state.prompts.active_prompt.is_some() {
        Some(SessionActivity::TurnStarted)
    } else if state.evidence.activity_reporter.last_reported() == Some(SessionActivity::TurnFailed)
    {
        state.evidence.activity_reporter.reset_turn();
        Some(SessionActivity::TurnEnded)
    } else {
        None
    };
    if let Some(activity) = activity {
        state.evidence.report(&state.qualified_handle, activity);
    }
    Ok(resent)
}

fn in_flight_prompt(state: &TaskSessionState) -> Option<InFlightPrompt> {
    if state.holders.0 == 0 {
        return None;
    }
    let entry = state
        .prompts
        .prompt_ledger
        .entries
        .iter()
        .find(|entry| entry.phase == PromptPhase::Dispatching)?;
    let content_blocks = entry
        .content_blocks
        .iter()
        .map(|value| serde_json::from_value(value.clone()))
        .collect::<Result<Vec<PromptContentBlockWire>, _>>()
        .ok()?;
    Some(InFlightPrompt {
        client_message_id: entry.client_message_id.clone(),
        prompt_text: entry.prompt_text.clone(),
        content_blocks,
    })
}

fn phase(state: &TaskSessionState, client_message_id: &str) -> Option<PromptPhase> {
    state
        .prompts
        .prompt_ledger
        .entry(client_message_id)
        .map(|entry| entry.phase)
}

fn host_note(text: &str, state: &mut TaskSessionState) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "note".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: state.stream_normalizer.fresh_item_id(),
        message_id: None,
    }
}
