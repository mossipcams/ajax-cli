use super::tests::{completed, line, Fixture};
use super::*;
use ajax_core::{agent_watcher::ProgressState, canonical_agent_event::AttentionReason};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};

fn worker(fixture: &Fixture, calls: &Arc<AtomicUsize>, progress: ProgressState) -> Worker {
    let observed = calls.clone();
    let shared = Arc::new(Mutex::new(Mailbox {
        frames: fixture.runtime.shared.lock().unwrap().frames.clone(),
        ..Mailbox::default()
    }));
    Worker {
        shared,
        events_dir: fixture.events_dir(),
        tasks: HashMap::new(),
        config: WatcherConfig::default(),
        judge: Arc::new(move |_, _| {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(WatcherVerdict {
                state: progress,
                confidence: 1.0,
            })
        }),
        judge_thread: None,
        timeout: Duration::from_secs(1),
        started_at_ms: now_ms(),
    }
}

fn at(line: &str, now: u64) -> String {
    let mut value: Value = serde_json::from_str(line).unwrap();
    value["occurred_at_unix_millis"] = json!(now);
    value["received_at_unix_millis"] = json!(now);
    value.to_string()
}

fn tick(worker: &mut Worker, now: u64) {
    worker.shared.lock().unwrap().tick = Some(now);
    worker.consume("");
}

#[test]
fn first_read_without_metadata_only_rebuilds_state_even_for_recent_stops() {
    for ancient in [true, false] {
        let fixture = Fixture::new(NullJudge);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
        let stamp = if ancient { 1 } else { worker.started_at_ms };
        fixture.append(&at(&completed("old-stop"), stamp));
        tick(&mut worker, now_ms());
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert!(worker.shared.lock().unwrap().outbox.is_empty());
        let state = &worker.tasks["task-1"].state;
        assert!(state.has_seen_event_id("old-stop"));
        assert_eq!(state.pending_checkpoint, None);
        assert_eq!(state.intervention_count, 0);
        fixture.append(&completed("live-stop"));
        tick(&mut worker, now_ms());
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }
}

#[test]
fn events_after_restored_cursor_from_downtime_are_replay() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let now = now_ms();
    fixture.append(&at(
        &line("cursor", "turn_started", Value::Null),
        now - 1000,
    ));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    worker
        .shared
        .lock()
        .unwrap()
        .frames
        .get_mut("task-1")
        .unwrap()
        .persisted = Some(WatcherPersistedState {
        last_seen_event_id: Some("cursor".into()),
        ..Default::default()
    });
    fixture.append(&at(&completed("first-downtime-stop"), now - 500));
    tick(&mut worker, now_ms());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    // Also exercise late discovery of an event stamped before runtime startup.
    fixture.append(&at(&completed("downtime-stop"), now - 100));
    tick(&mut worker, now_ms());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
    assert_eq!(worker.tasks["task-1"].state.pending_checkpoint, None);
}

#[test]
fn stale_checkpoints_fail_open_even_when_refresh_ticks_are_fresh() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    fixture.append(&at(&completed("ancient"), 1));
    tick(&mut worker, now_ms());
    calls.store(0, Ordering::Relaxed);
    worker.shared.lock().unwrap().outbox.clear();
    let state = &mut worker.tasks.get_mut("task-1").unwrap().state;
    state.record_intervention(1);
    state.grace_deadline_ms = Some(2);
    tick(&mut worker, now_ms());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
    assert_eq!(worker.tasks["task-1"].state.pending_checkpoint, None);
}

#[test]
fn ten_same_signature_finishes_only_call_judge_once() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Uncertain);
    fixture.append(&line("ready", "turn_started", Value::Null));
    let now = now_ms();
    tick(&mut worker, now);
    for index in 1..=10 {
        let stamp = now + index * 31_000;
        fixture.append(&at(
            &line(
                &format!("repeat-{index}"),
                "activity_finished",
                json!({"activity": {"activity": "tool", "signature": "repeat", "success": false}}),
            ),
            stamp,
        ));
        tick(&mut worker, stamp);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn judge_is_not_called_for_attention_terminal_phases_or_active_grace() {
    for phase in [
        WatcherPhase::Healthy,
        WatcherPhase::Escalated,
        WatcherPhase::WaitingOnUser,
        WatcherPhase::Recovering,
    ] {
        let fixture = Fixture::new(NullJudge);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
        fixture.append(&line("ready", "turn_started", Value::Null));
        let now = now_ms();
        tick(&mut worker, now);
        let state = &mut worker.tasks.get_mut("task-1").unwrap().state;
        state.phase = phase;
        state.pending_checkpoint = Some(PendingCheckpoint::Loop);
        if phase == WatcherPhase::Healthy {
            state.pending_attention = Some(AttentionReason::Permission);
        }
        if phase == WatcherPhase::Recovering {
            state.grace_deadline_ms = Some(now + 120_000);
        }
        tick(&mut worker, now);
        assert_eq!(calls.load(Ordering::Relaxed), 0, "{phase:?}");
        assert_eq!(worker.tasks["task-1"].state.pending_checkpoint, None);
    }
}

#[test]
fn uncertain_grace_verdict_defers_refresh_judgment() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Uncertain);
    fixture.append(&line("ready", "turn_started", Value::Null));
    let now = now_ms();
    tick(&mut worker, now);
    let state = &mut worker.tasks.get_mut("task-1").unwrap().state;
    state.record_intervention(now - 120_000);
    state.grace_deadline_ms = Some(now);
    for index in 0..=6 {
        tick(&mut worker, now + index * 10_000);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        worker.tasks["task-1"].state.grace_deadline_ms,
        Some(now + 120_000)
    );
}

#[test]
fn per_task_judge_cooldown_uses_injected_time() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Uncertain);
    fixture.append(&line("ready", "turn_started", Value::Null));
    let now = now_ms();
    tick(&mut worker, now);
    for index in 0..10 {
        fixture.append(&at(&completed(&format!("stop-{index}")), now + index));
        tick(&mut worker, now + index);
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    fixture.append(&at(&completed("after-cooldown"), now + 30_000));
    tick(&mut worker, now + 30_000);
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn enabled_watcher_without_judge_starts_no_runtime_and_writes_no_metadata() {
    let mut fixture = Fixture::new(NullJudge);
    fixture.context.runtime_paths.cache_dir = fixture.events_dir().join("unconfigured");
    assert!(fixture.context.config.watcher.enabled);
    assert!(fixture.context.config.watcher.laya_command.is_none());
    assert!(crate::web_backend::start_agent_watcher(&mut fixture.context).is_none());
    assert!(WatcherRuntime::for_events_dir(&fixture.events_dir()).is_none());
    assert!(fixture.task().metadata.is_empty());
}
