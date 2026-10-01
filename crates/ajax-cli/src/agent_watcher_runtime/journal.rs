//! Incremental JSONL reads and canonical-to-watcher translation.

use std::{
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::PathBuf,
};

use ajax_core::canonical_agent_event::{CanonicalEventDetail, CanonicalEventKind, ParsedEnvelope};
use serde::Deserialize;

use super::*;

struct Envelope {
    canonical: ParsedEnvelope,
    occurred_at_unix_millis: Option<u64>,
}

#[derive(Deserialize)]
struct Timestamp {
    #[serde(default)]
    occurred_at_unix_millis: Option<u64>,
}

impl Worker {
    pub(super) fn journal_path(&self, task_id: &str) -> PathBuf {
        self.events_dir.join(format!(
            "{}.jsonl",
            crate::agent_runtime::task_file_stem(task_id)
        ))
    }

    pub(super) fn read_journal(&mut self, task: &mut WatchedTask) -> io::Result<()> {
        let file = File::open(self.journal_path(&task.state.task_id))?;
        // JSONL is append-only. A replaced/truncated log cannot justify replaying
        // interventions; stop at the old cursor until new evidence catches up.
        if file.metadata()?.len() < task.offset {
            return Ok(());
        }
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(task.offset))?;
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 || !line.ends_with('\n') {
                break; // Retry partial appends on the next wake.
            }
            task.offset += bytes as u64;
            let Ok(canonical) = serde_json::from_str::<ParsedEnvelope>(&line) else {
                continue;
            };
            let Ok(timestamp) = serde_json::from_str::<Timestamp>(&line) else {
                continue;
            };
            let envelope = Envelope {
                canonical,
                occurred_at_unix_millis: timestamp.occurred_at_unix_millis,
            };
            let canonical = &envelope.canonical;
            if canonical.task_id.as_deref() != Some(&task.state.task_id)
                || canonical.parent_run_id.is_some()
            {
                continue;
            }
            let Some(event) = translate(&envelope) else {
                continue;
            };
            let run = canonical.run_id.as_deref().unwrap_or("primary");
            if run != task.state.run_id {
                task.state = WatcherState::new(
                    &TaskFrame {
                        objective: task.state.objective.clone(),
                    },
                    &task.state.task_id,
                    run,
                    &task.state.harness,
                );
            }
            let replay = task.restoring.is_some();
            self.step(&mut task.state, &event, event.occurred_at_ms, replay);
            if task
                .restoring
                .as_ref()
                .is_some_and(|state| state.last_seen_event_id.as_deref() == Some(&event.event_id))
            {
                if let Some(persisted) = task.restoring.take() {
                    task.state.apply_persisted(&persisted);
                    if task.state.grace_deadline_ms.is_some()
                        && task.state.pending_attention.is_none()
                    {
                        task.state.phase = WatcherPhase::Recovering;
                    }
                }
            }
            // New evidence can cancel a nudge still waiting for registry refresh.
            if task.state.pending_attention.is_some() || task.state.phase == WatcherPhase::Healthy {
                if let Ok(mut shared) = self.shared.lock() {
                    shared
                        .outbox
                        .retain(|nudge| nudge.task_id != task.state.task_id);
                }
            }
        }
        Ok(())
    }
}

fn translate(envelope: &Envelope) -> Option<WatcherEvent> {
    let canonical = &envelope.canonical;
    let event_id = canonical.event_id.clone().filter(|id| !id.is_empty())?;
    let kind = match canonical.kind {
        CanonicalEventKind::TurnStarted | CanonicalEventKind::AttentionCleared => {
            WatcherEventKind::TurnStarted
        }
        CanonicalEventKind::ActivityStarted => WatcherEventKind::ActivityStarted,
        CanonicalEventKind::ActivityFinished => WatcherEventKind::ActivityFinished,
        CanonicalEventKind::AttentionRequested => WatcherEventKind::Attention,
        CanonicalEventKind::TurnSettled => WatcherEventKind::TurnSettled,
        CanonicalEventKind::ChildStarted => WatcherEventKind::ChildStarted,
        CanonicalEventKind::ChildSettled => WatcherEventKind::ChildSettled,
        CanonicalEventKind::Heartbeat
        | CanonicalEventKind::SessionOpened
        | CanonicalEventKind::SessionClosed => return None,
    };
    let detail = match &canonical.detail {
        Some(CanonicalEventDetail::Activity {
            activity_id,
            signature,
            success,
            ..
        }) => WatcherEventDetail::Activity {
            activity_id: activity_id.clone(),
            signature: signature.clone(),
            success: *success,
        },
        Some(CanonicalEventDetail::Attention { attention }) => WatcherEventDetail::Attention {
            attention: attention.clone(),
        },
        Some(CanonicalEventDetail::Outcome { outcome }) => WatcherEventDetail::TurnSettled {
            outcome: outcome.clone(),
        },
        None => WatcherEventDetail::None,
    };
    Some(WatcherEvent {
        kind,
        detail,
        occurred_at_ms: envelope
            .occurred_at_unix_millis
            .or_else(|| u64::try_from(canonical.received_at_unix_millis).ok())?,
        event_id,
    })
}
