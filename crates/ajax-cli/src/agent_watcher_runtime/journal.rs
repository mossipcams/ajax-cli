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

    /// Returns whether this read recovered a truncated/replaced journal.
    pub(super) fn read_journal(&mut self, task: &mut WatchedTask) -> io::Result<bool> {
        let file = File::open(self.journal_path(&task.state.task_id))?;
        let replaced = file.metadata()?.len() < task.offset;
        if replaced {
            task.offset = 0;
            task.state.pending_checkpoint = None;
            task.restoring.get_or_insert_with(|| task.state.persisted());
        }
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(task.offset))?;
        let mut line = Vec::new();
        loop {
            line.clear();
            let bytes = reader.read_until(b'\n', &mut line)?;
            if bytes == 0 || !line.ends_with(b"\n") {
                task.finish_restore();
                break; // Retry partial appends on the next wake.
            }
            task.offset += bytes as u64;
            let Ok(line) = std::str::from_utf8(&line) else {
                continue;
            };
            let Ok(canonical) = serde_json::from_str::<ParsedEnvelope>(line) else {
                continue;
            };
            let Ok(timestamp) = serde_json::from_str::<Timestamp>(line) else {
                continue;
            };
            let envelope = Envelope {
                canonical,
                occurred_at_unix_millis: timestamp.occurred_at_unix_millis,
            };
            let canonical = &envelope.canonical;
            let run = canonical
                .run_id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or("primary");
            if canonical.task_id.as_deref() != Some(&task.state.task_id)
                || canonical.parent_run_id.is_some()
                || run != "primary"
            {
                continue;
            }
            let Some(event) = translate(&envelope) else {
                continue;
            };
            let replay = replaced
                || task.initial_read
                || task.restoring.is_some()
                || event.occurred_at_ms < self.started_at_ms;
            let fresh = !task.state.has_seen_event_id(&event.event_id);
            if fresh {
                task.newest_event_at_ms = Some(event.occurred_at_ms);
            }
            self.step(&mut task.state, &event, event.occurred_at_ms, replay);
            if task
                .restoring
                .as_ref()
                .is_some_and(|state| state.last_seen_event_id.as_deref() == Some(&event.event_id))
            {
                task.finish_restore();
            }
            // New evidence can cancel a nudge still waiting for registry refresh.
            if !replay
                && fresh
                && (matches!(
                    event.kind,
                    WatcherEventKind::TurnStarted | WatcherEventKind::Attention
                ) || task.state.pending_attention.is_some()
                    || matches!(
                        task.state.phase,
                        WatcherPhase::Healthy
                            | WatcherPhase::Escalated
                            | WatcherPhase::WaitingOnUser
                    ))
            {
                if let Ok(mut shared) = self.shared.lock() {
                    shared.cancelled.insert(task.state.task_id.clone());
                    shared
                        .outbox
                        .retain(|nudge| nudge.task_id != task.state.task_id);
                }
            }
        }
        task.initial_read = false;
        Ok(replaced)
    }
}

impl WatchedTask {
    fn finish_restore(&mut self) {
        if let Some(mut persisted) = self.restoring.take() {
            // A missing marker must not replace the cursor we actually folded.
            persisted.last_seen_event_id = self.state.seen_event_ids.back().cloned();
            self.state.apply_persisted(&persisted);
            // Older metadata omitted phase; retain its recovery window without
            // overriding a persisted operator handoff.
            if self.state.grace_deadline_ms.is_some()
                && self.state.pending_attention.is_none()
                && !matches!(
                    self.state.phase,
                    WatcherPhase::Escalated | WatcherPhase::WaitingOnUser
                )
            {
                self.state.phase = WatcherPhase::Recovering;
            }
        }
    }
}

fn translate(envelope: &Envelope) -> Option<WatcherEvent> {
    let canonical = &envelope.canonical;
    let event_id = canonical.event_id.clone().filter(|id| !id.is_empty())?;
    let kind = match canonical.kind {
        CanonicalEventKind::TurnStarted | CanonicalEventKind::SessionOpened => {
            WatcherEventKind::TurnStarted
        }
        CanonicalEventKind::SessionClosed => WatcherEventKind::SessionClosed,
        CanonicalEventKind::AttentionCleared => WatcherEventKind::AttentionCleared,
        CanonicalEventKind::ActivityStarted => WatcherEventKind::ActivityStarted,
        CanonicalEventKind::ActivityFinished => WatcherEventKind::ActivityFinished,
        CanonicalEventKind::AttentionRequested => WatcherEventKind::Attention,
        CanonicalEventKind::TurnSettled => WatcherEventKind::TurnSettled,
        CanonicalEventKind::ChildStarted => WatcherEventKind::ChildStarted,
        CanonicalEventKind::ChildSettled => WatcherEventKind::ChildSettled,
        CanonicalEventKind::Heartbeat => return None,
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

#[cfg(test)]
mod regression_tests {
    use super::*;
    use crate::agent_watcher_runtime::tests::{completed, line, stuck, wait_until, Fixture, Judge};
    use ajax_core::{
        agent_notification::{AgentNotificationDelivery, AgentNotificationDeliveryStatus},
        agent_watcher::{pending_watcher_nudge, record_watcher_delivery},
    };
    use serde_json::{json, Value};

    #[test]
    fn two_episodes_queue_distinct_nudges_after_budget_reset() {
        let mut fixture = Fixture::new(Judge(stuck));
        fixture.send(&completed("first"));
        fixture.processed("first");
        fixture.runtime.refresh(&mut fixture.context);
        let first = pending_watcher_nudge(fixture.task()).unwrap();
        record_watcher_delivery(
            fixture
                .context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap(),
            AgentNotificationDelivery {
                notification_id: first.id().into(),
                status: AgentNotificationDeliveryStatus::Accepted,
                detail: None,
            },
        );
        fixture.send(&line("progress", "activity_finished", json!({"activity": {"activity": "tool", "activity_id": "edit", "signature": "edit", "success": true}})));
        fixture.processed("progress");
        for i in 0..3 {
            let id = format!("repeat-{i}");
            // This second episode is outside the per-task judge cooldown.
            let mut shared = fixture.runtime.shared.lock().unwrap();
            shared.tick = Some(now_ms() + JUDGE_COOLDOWN_MS);
            fixture.send(&line(&id, "activity_finished", json!({"activity": {"activity": "tool", "activity_id": id, "signature": "loop", "success": false}})));
            drop(shared);
            fixture.processed(&id);
        }
        fixture.runtime.refresh(&mut fixture.context);
        let second =
            pending_watcher_nudge(fixture.task()).expect("second episode must queue a nudge");
        assert_ne!(second.id(), first.id());
        assert_eq!(
            load_watcher_state(fixture.task())
                .unwrap()
                .intervention_count,
            1
        );
        assert_eq!(load_watcher_state(fixture.task()).unwrap().nudge_seq, 2);
    }

    #[test]
    fn new_turn_and_attention_cancel_nudges_in_both_queues() {
        for kind in ["turn_started", "attention_requested"] {
            for moved_to_metadata in [false, true] {
                let mut fixture = Fixture::new(Judge(stuck));
                fixture.send(&completed("done"));
                fixture.processed("done");
                assert_eq!(fixture.runtime.shared.lock().unwrap().outbox.len(), 1);
                if moved_to_metadata {
                    fixture.runtime.refresh(&mut fixture.context);
                    assert!(pending_watcher_nudge(fixture.task()).is_some());
                }
                let detail = if kind == "attention_requested" {
                    json!({"attention": {"attention": "permission"}})
                } else {
                    Value::Null
                };
                fixture.send(&line("moved-on", kind, detail));
                fixture.processed("moved-on");
                assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
                fixture.runtime.refresh(&mut fixture.context);
                assert!(pending_watcher_nudge(fixture.task()).is_none());
            }
        }
    }

    #[test]
    fn restored_terminal_phases_never_renudge_and_cancel_pending_metadata() {
        for phase in [WatcherPhase::Escalated, WatcherPhase::WaitingOnUser] {
            let mut fixture = Fixture::new(Judge(stuck));
            fixture.send(&completed("done"));
            fixture.processed("done");
            fixture.runtime.refresh(&mut fixture.context);
            let mut persisted = load_watcher_state(fixture.task()).unwrap();
            persisted.phase = phase;
            persisted.grace_deadline_ms = Some(0); // Old persisted state may retain this.
            store_watcher_state(
                fixture
                    .context
                    .registry
                    .get_task_mut(&TaskId::new("task-1"))
                    .unwrap(),
                &persisted,
            );
            let restarted =
                WatcherRuntime::start(fixture.events_dir(), Arc::new(Judge(stuck))).unwrap();
            restarted.refresh(&mut fixture.context);
            wait_until(|| {
                restarted
                    .shared
                    .lock()
                    .unwrap()
                    .states
                    .contains_key("task-1")
            });
            {
                let shared = restarted.shared.lock().unwrap();
                assert_eq!(shared.states["task-1"].phase, phase);
                assert!(shared.outbox.is_empty());
            }
            restarted.refresh(&mut fixture.context);
            assert!(pending_watcher_nudge(fixture.task()).is_none());
            let event = line(
                "repeat-after-restart",
                "activity_finished",
                json!({"activity": {"activity": "tool", "signature": "loop", "success": false}}),
            );
            fixture.append(&event);
            restarted.sink().send(event).unwrap();
            fixture.append(&completed("stop-after-restart"));
            restarted
                .sink()
                .send(completed("stop-after-restart"))
                .unwrap();
            wait_until(|| {
                restarted
                    .shared
                    .lock()
                    .unwrap()
                    .states
                    .get("task-1")
                    .is_some_and(|s| s.last_seen_event_id.as_deref() == Some("stop-after-restart"))
            });
            if phase == WatcherPhase::Escalated {
                assert!(restarted.shared.lock().unwrap().outbox.is_empty());
            }
        }
    }

    #[test]
    fn attention_cleared_preserves_existing_recovery_episode() {
        let mut fixture = Fixture::new(Judge(stuck));
        fixture.send(&completed("done"));
        fixture.processed("done");
        fixture.runtime.refresh(&mut fixture.context);
        let deadline = load_watcher_state(fixture.task())
            .unwrap()
            .grace_deadline_ms;
        fixture.send(&line(
            "attention",
            "attention_requested",
            json!({"attention": {"attention": "question"}}),
        ));
        fixture.processed("attention");
        fixture.send(&line("cleared", "attention_cleared", Value::Null));
        fixture.processed("cleared");
        let shared = fixture.runtime.shared.lock().unwrap();
        assert_eq!(shared.states["task-1"].phase, WatcherPhase::Recovering);
        assert_eq!(shared.states["task-1"].grace_deadline_ms, deadline);
        assert_eq!(shared.states["task-1"].intervention_count, 1);
    }
}
