//! Journal robustness regressions for #1197 (review C2).
use super::replay_tests::{at, tick, worker};
use super::tests::{completed, line, Fixture};
use super::*;
use ajax_core::agent_watcher::ProgressState;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
};

fn activity(id: &str, kind: &str) -> String {
    line(
        id,
        kind,
        json!({"activity": {"activity": "tool", "activity_id": id}}),
    )
}

#[test]
fn invalid_utf8_line_does_not_hide_a_completed_stop() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    OpenOptions::new()
        .append(true)
        .open(worker.journal_path("task-1"))
        .unwrap()
        .write_all(b"\xff\xfe\n")
        .unwrap();
    fixture.append(&at(&completed("after-garbage"), now));
    tick(&mut worker, now);
    assert!(worker.tasks["task-1"]
        .state
        .has_seen_event_id("after-garbage"));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(worker.shared.lock().unwrap().outbox.len(), 1);
}

#[test]
fn truncated_journal_replays_then_accepts_live_events() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    let path = worker.journal_path("task-1");
    let replacement = format!("{}\n", at(&completed("new"), now));
    // Make the old offset strictly longer than the replacement.
    fixture.append(&line("old-padding", "turn_started", Value::Null));
    tick(&mut worker, now);
    assert!(worker.tasks["task-1"].offset > replacement.len() as u64);
    std::fs::write(&path, &replacement).unwrap();
    tick(&mut worker, now);
    assert_eq!(worker.tasks["task-1"].offset, replacement.len() as u64);
    assert!(worker.tasks["task-1"].state.has_seen_event_id("new"));
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
    fixture.append(&at(&completed("live"), now + 1));
    tick(&mut worker, now + 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn truncation_preserves_caps_and_suppresses_the_same_refresh_grace_checkpoint() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    fixture.append(&line("padding", "turn_started", Value::Null));
    tick(&mut worker, now);
    let state = &mut worker.tasks.get_mut("task-1").unwrap().state;
    state.record_intervention(now - 120_001);
    state.grace_deadline_ms = Some(now - 1);
    let budget = state.persisted();
    std::fs::write(
        worker.journal_path("task-1"),
        format!("{}\n", at(&completed("new"), now)),
    )
    .unwrap();
    tick(&mut worker, now);
    let state = &worker.tasks["task-1"].state;
    assert!(state.has_seen_event_id("new"));
    assert_eq!(state.intervention_count, budget.intervention_count);
    assert_eq!(state.phase, budget.phase);
    assert_eq!(state.nudge_seq, budget.nudge_seq);
    assert!(state.pending_checkpoint.is_none());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn duplicate_only_journal_replacement_during_judgment_invalidates_the_verdict() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    let stop = at(&completed("stop"), now);
    fixture.append(&stop);
    let path = worker.journal_path("task-1");
    let judge = worker.judge.clone();
    worker.judge = Arc::new(move |snapshot, checkpoint| {
        std::fs::write(&path, format!("{stop}\n")).unwrap();
        judge(snapshot, checkpoint)
    });
    tick(&mut worker, now);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
    assert_eq!(worker.tasks["task-1"].state.intervention_count, 0);
}

#[test]
fn missing_marker_at_partial_eof_releases_restore_without_consuming_partial_bytes() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    worker
        .shared
        .lock()
        .unwrap()
        .frames
        .get_mut("task-1")
        .unwrap()
        .persisted = Some(WatcherPersistedState {
        intervention_count: 2,
        phase: WatcherPhase::Escalated,
        last_seen_event_id: Some("missing".into()),
        ..Default::default()
    });
    let path = worker.journal_path("task-1");
    let complete_len = std::fs::metadata(&path).unwrap().len();
    let partial = at(&line("partial", "turn_started", Value::Null), now);
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(partial.as_bytes()).unwrap();
    tick(&mut worker, now);
    let task = &worker.tasks["task-1"];
    assert!(task.restoring.is_none());
    assert_eq!(task.offset, complete_len);
    assert_eq!(task.state.intervention_count, 2);
    assert!(!task.state.has_seen_event_id("partial"));
    file.write_all(b"\n").unwrap();
    tick(&mut worker, now);
    assert!(worker.tasks["task-1"].state.has_seen_event_id("partial"));
}

#[test]
fn missing_restore_marker_preserves_caps_and_releases_replay() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    let persisted = WatcherPersistedState {
        intervention_count: 2,
        premature_stop_nudges: 1,
        loop_nudges: 1,
        phase: WatcherPhase::Escalated,
        nudge_seq: 7,
        last_seen_event_id: Some("missing-marker".into()),
        ..Default::default()
    };
    worker
        .shared
        .lock()
        .unwrap()
        .frames
        .get_mut("task-1")
        .unwrap()
        .persisted = Some(persisted);
    fixture.append(&at(&activity("folded-tool", "activity_started"), now));
    tick(&mut worker, now);
    let task = &worker.tasks["task-1"];
    assert!(task.restoring.is_none());
    assert_eq!(task.state.intervention_count, 2);
    assert_eq!(task.state.premature_stop_nudges, 1);
    assert_eq!(task.state.loop_nudges, 1);
    assert_eq!(task.state.phase, WatcherPhase::Escalated);
    assert_eq!(task.state.nudge_seq, 7);
    assert_eq!(task.state.open_tools, ["folded-tool"]);
    assert_eq!(
        task.state.persisted().last_seen_event_id.as_deref(),
        Some("folded-tool")
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    fixture.append(&at(&line("resume", "turn_started", Value::Null), now + 1));
    fixture.append(&at(&line("closed", "session_closed", Value::Null), now + 1));
    fixture.append(&at(&completed("live"), now + 1));
    tick(&mut worker, now + 1);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn stray_child_run_and_normalized_primary_ids_preserve_intervention() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    fixture.append(&at(&completed("nudge"), now));
    tick(&mut worker, now);
    let before = worker.tasks["task-1"].state.persisted();
    assert_eq!(before.intervention_count, 1);
    for (index, run) in [Some("run-2"), Some(" primary "), Some(" \t"), None]
        .into_iter()
        .enumerate()
    {
        let id = format!("run-event-{index}");
        let mut event: Value =
            serde_json::from_str(&at(&activity(&id, "activity_started"), now)).unwrap();
        if let Some(run) = run {
            event["run_id"] = json!(run);
        } else {
            event.as_object_mut().unwrap().remove("run_id");
        }
        fixture.append(&event.to_string());
        tick(&mut worker, now);
        let state = &worker.tasks["task-1"].state;
        assert_eq!(state.run_id, "primary");
        assert_eq!(state.intervention_count, before.intervention_count);
        assert_eq!(state.phase, before.phase);
        assert_eq!(state.nudge_seq, before.nudge_seq);
        assert_eq!(state.has_seen_event_id(&id), index != 0);
    }
}

#[test]
fn session_close_and_orphan_expiry_unblock_judgment() {
    for closed in [false, true] {
        let fixture = Fixture::new(NullJudge);
        let calls = Arc::new(AtomicUsize::new(0));
        let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
        let now = worker.started_at_ms;
        tick(&mut worker, now);
        fixture.append(&at(&line("opened", "session_opened", Value::Null), now));
        fixture.append(&at(&activity("orphan", "activity_started"), now));
        if closed {
            fixture.append(&at(&line("child", "child_started", Value::Null), now));
            fixture.append(&at(&line("closed", "session_closed", Value::Null), now + 1));
        }
        let later = now + if closed { 1 } else { 600_001 };
        fixture.append(&at(&completed("done"), later));
        tick(&mut worker, later);
        let state = &worker.tasks["task-1"].state;
        assert!(state.has_seen_event_id("opened"));
        assert!(state.open_tools.is_empty());
        assert_eq!(state.open_children, 0);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(worker.shared.lock().unwrap().outbox.len(), 1);
    }
}

#[test]
fn cursor_only_registry_flushes_are_coalesced_per_task_for_a_minute() {
    let mut fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    let (wake, _inbox) = mpsc::channel();
    let runtime = WatcherRuntime {
        shared: worker.shared.clone(),
        wake,
    };
    assert!(runtime.refresh_at(&mut fixture.context, now));
    let first = load_watcher_state(fixture.task()).unwrap();
    for index in 1..=20 {
        fixture.append(&at(
            &activity(&format!("tool-{index}"), "activity_started"),
            now + index,
        ));
        tick(&mut worker, now + index);
        assert!(!runtime.refresh_at(&mut fixture.context, now + index));
    }
    assert_eq!(load_watcher_state(fixture.task()).unwrap(), first);
    tick(&mut worker, now + 59_999);
    assert!(!runtime.refresh_at(&mut fixture.context, now + 59_999));
    tick(&mut worker, now + 60_000);
    assert!(runtime.refresh_at(&mut fixture.context, now + 60_000));
    assert_eq!(
        load_watcher_state(fixture.task())
            .unwrap()
            .last_seen_event_id
            .as_deref(),
        Some("tool-20")
    );
    // A non-cursor change must flush immediately, even inside the next window.
    worker
        .tasks
        .get_mut("task-1")
        .unwrap()
        .state
        .record_intervention(now + 60_001);
    tick(&mut worker, now + 60_001);
    assert!(runtime.refresh_at(&mut fixture.context, now + 60_001));
    assert_eq!(
        load_watcher_state(fixture.task())
            .unwrap()
            .intervention_count,
        1
    );
    tick(&mut worker, now + 60_002);
    assert!(!runtime.refresh_at(&mut fixture.context, now + 60_002));
    // An idempotent refresh after the interval is not a flush.
    tick(&mut worker, now + 120_001);
    assert!(!runtime.refresh_at(&mut fixture.context, now + 120_001));
    fixture.append(&at(&activity("later", "activity_started"), now + 120_002));
    tick(&mut worker, now + 120_002);
    assert!(runtime.refresh_at(&mut fixture.context, now + 120_002));
}

#[test]
fn session_open_preserves_escalation_but_user_turn_resets_episode() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    let state = &mut worker.tasks.get_mut("task-1").unwrap().state;
    state.record_intervention(now);
    state.phase = WatcherPhase::Escalated;
    state.premature_stop_nudges = 1;
    state.loop_nudges = 1;
    state.pending_attention = Some(ajax_core::canonical_agent_event::AttentionReason::Permission);
    let before = state.persisted();
    fixture.append(&at(
        &line("session", "session_opened", Value::Null),
        now + 1,
    ));
    tick(&mut worker, now + 1);
    let state = &worker.tasks["task-1"].state;
    assert_eq!(state.phase, WatcherPhase::Escalated);
    assert_eq!(state.intervention_count, before.intervention_count);
    assert_eq!(state.premature_stop_nudges, before.premature_stop_nudges);
    assert_eq!(state.loop_nudges, before.loop_nudges);
    assert!(state.pending_attention.is_some());
    assert_eq!(state.recent_events.back().unwrap(), "session_opened");
    fixture.append(&at(&line("user", "turn_started", Value::Null), now + 2));
    tick(&mut worker, now + 2);
    let state = &worker.tasks["task-1"].state;
    assert_eq!(state.phase, WatcherPhase::Healthy);
    assert_eq!(state.intervention_count, 0);
    assert_eq!(state.premature_stop_nudges, 0);
    assert_eq!(state.loop_nudges, 0);
    assert_eq!(state.lifetime_nudges, before.lifetime_nudges);
    assert!(state.pending_attention.is_none());
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

#[test]
fn child_tracking_is_bounded_fifo_and_expires_from_event_time() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::ProbablyDone);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    for i in 0..40 {
        fixture.append(&at(
            &line(&format!("child-{i}"), "child_started", Value::Null),
            now + i,
        ));
    }
    tick(&mut worker, now + 40);
    assert_eq!(worker.tasks["task-1"].state.open_children, 32);
    fixture.append(&at(
        &line("finished", "child_settled", Value::Null),
        now + 41,
    ));
    tick(&mut worker, now + 41);
    assert_eq!(worker.tasks["task-1"].state.open_children, 31);
    // The oldest remaining start is +9: it survives exactly ten minutes.
    fixture.append(&at(
        &line("boundary", "session_opened", Value::Null),
        now + 600_009,
    ));
    tick(&mut worker, now + 600_009);
    assert_eq!(worker.tasks["task-1"].state.open_children, 31);
    fixture.append(&at(
        &line("expired", "session_opened", Value::Null),
        now + 600_010,
    ));
    tick(&mut worker, now + 600_010);
    assert_eq!(worker.tasks["task-1"].state.open_children, 30);
    fixture.append(&at(&completed("done"), now + 660_000));
    tick(&mut worker, now + 660_000);
    assert_eq!(worker.tasks["task-1"].state.open_children, 0);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn completed_reply_inside_grace_never_requests_a_second_judgment() {
    let fixture = Fixture::new(NullJudge);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut worker = worker(&fixture, &calls, ProgressState::Stuck);
    let now = worker.started_at_ms;
    tick(&mut worker, now);
    fixture.append(&at(&completed("nudge"), now));
    tick(&mut worker, now);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    worker.shared.lock().unwrap().outbox.clear();
    fixture.append(&at(&completed("reply"), now + 30_000));
    tick(&mut worker, now + 30_000);
    tick(&mut worker, now + 120_001);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(worker.tasks["task-1"].state.phase, WatcherPhase::Escalated);
    assert_eq!(worker.tasks["task-1"].state.intervention_count, 1);
    assert!(worker.shared.lock().unwrap().outbox.is_empty());
}
