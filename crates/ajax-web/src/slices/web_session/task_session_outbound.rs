use super::task_session::{AttachSnapshot, OutboundBatch, TaskSessionState};
use super::{
    protocol::{SessionChrome, SessionSnapshot},
    replay::{build_attach, pending_elicitation, pending_permission},
};

pub(super) fn attach_snapshot(
    state: &mut TaskSessionState,
    client_cursor: Option<usize>,
) -> AttachSnapshot {
    state.pump();
    let (snapshot, replayed) = build_attach(
        &state.log,
        state.applied_model.clone(),
        state.busy(),
        client_cursor,
        SessionChrome {
            session_config_options: state.session_config_options.clone(),
            available_commands: state.session_available_commands.clone(),
            prompt_capabilities: state.session_prompt_capabilities.clone(),
            session_title: state.session_title.clone(),
        },
    );
    AttachSnapshot {
        generation: state.generation,
        snapshot,
        replayed,
    }
}

pub(super) fn collect_outbound(
    state: &mut TaskSessionState,
    cursor: usize,
    generation: u64,
) -> OutboundBatch {
    let current_generation = state.generation;
    let generation_changed = current_generation != generation;
    let read_from = if generation_changed {
        state.log.dropped
    } else {
        cursor
    };
    let snapshot = if generation_changed {
        Some(snapshot(state, state.applied_model.clone(), true, None))
    } else if let Some(model) = state.pending_model_snapshot.take() {
        let config = state.pending_config_snapshot.take();
        let _ = state.pending_commands_snapshot.take();
        let _ = state.pending_capabilities_snapshot.take();
        state.pending_title_snapshot = false;
        Some(snapshot(state, model, false, config))
    } else if state.pending_title_snapshot
        || state.pending_commands_snapshot.is_some()
        || state.pending_capabilities_snapshot.is_some()
    {
        let _ = state.pending_commands_snapshot.take();
        let _ = state.pending_capabilities_snapshot.take();
        state.pending_title_snapshot = false;
        Some(snapshot(state, state.applied_model.clone(), false, None))
    } else {
        None
    };
    state.pump();
    let (events, next) = state.log.read_from_enveloped(read_from);
    OutboundBatch {
        generation: current_generation,
        cursor: next,
        snapshot,
        events,
    }
}

fn snapshot(
    state: &TaskSessionState,
    model: String,
    reset: bool,
    config: Option<Vec<crate::adapters::web_session_acp::ConfigOptionDescriptor>>,
) -> SessionSnapshot {
    SessionSnapshot::new(
        state.log.absolute_next_cursor(),
        model,
        state.busy(),
        reset,
        pending_permission(&state.log),
        pending_elicitation(&state.log),
        SessionChrome {
            session_config_options: config.or_else(|| state.session_config_options.clone()),
            available_commands: state.session_available_commands.clone(),
            prompt_capabilities: state.session_prompt_capabilities.clone(),
            session_title: state.session_title.clone(),
        },
    )
}

#[cfg(test)]
pub(crate) fn disk_read_from(
    state_dir: &std::path::Path,
    handle: &str,
    cursor: usize,
) -> (Vec<super::SessionServerEvent>, usize) {
    let stored: crate::adapters::web_session_store::StoredSession<super::SessionServerEvent> =
        crate::adapters::web_session_store::load(state_dir, handle);
    if stored.events.is_empty() {
        (Vec::new(), cursor)
    } else {
        super::transcript::TranscriptLog::from_events(stored.events, stored.dropped)
            .read_from(cursor)
    }
}
