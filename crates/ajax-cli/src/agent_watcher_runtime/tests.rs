use super::*;
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec},
    agent_notification::AgentNotification,
    agent_watcher::{pending_watcher_nudge, ProgressState},
    config::{Config, RuntimePathRequest},
    models::AgentClient,
    runtime_refresh::RefreshTier,
};
use ajax_web::runtime::RuntimeBridge;
use serde_json::{json, Value};
use std::{
    fs::OpenOptions,
    io::Write,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct Judge<F>(F);

impl<F: Fn(&WatcherSnapshot) -> Result<WatcherVerdict, JudgeError>> AgentProgressJudge
    for Judge<F>
{
    fn evaluate(&self, snapshot: &WatcherSnapshot) -> Result<WatcherVerdict, JudgeError> {
        (self.0)(snapshot)
    }
}

fn stuck(_: &WatcherSnapshot) -> Result<WatcherVerdict, JudgeError> {
    Ok(WatcherVerdict {
        state: ProgressState::Stuck,
        confidence: 0.95,
    })
}

struct Fixture {
    root: PathBuf,
    context: CommandContext<InMemoryRegistry>,
    runtime: Arc<WatcherRuntime>,
}

impl Fixture {
    fn new(judge: impl AgentProgressJudge + Send + Sync + 'static) -> Self {
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        let root = PathBuf::from(format!(
            "/tmp/ajax-watcher-{}-{}-{}",
            std::process::id(),
            now_ms(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let paths = RuntimePathRequest::new(&root)
            .with_cli_home(&root)
            .resolve();
        let events_dir = paths.cache_dir.join("agent-events");
        std::fs::create_dir_all(&events_dir).unwrap();
        let mut context = CommandContext::with_runtime_paths(
            Config::default(),
            InMemoryRegistry::default(),
            paths,
        );
        context
            .registry
            .create_task(Task::new(
                TaskId::new("task-1"),
                "repo",
                "ordinary",
                "Fix the parser",
                "fix/parser",
                "main",
                root.join("worktree"),
                "repo-ordinary",
                "task",
                AgentClient::Cursor,
            ))
            .unwrap();
        let runtime = WatcherRuntime::start_with_timeout(
            events_dir,
            Arc::new(judge),
            Duration::from_millis(250),
        )
        .unwrap();
        runtime.refresh(&mut context);
        Self {
            root,
            context,
            runtime,
        }
    }

    fn events_dir(&self) -> PathBuf {
        self.context.runtime_paths.cache_dir.join("agent-events")
    }

    fn append(&self, line: &str) {
        let path = self.events_dir().join(format!(
            "{}.jsonl",
            crate::agent_runtime::task_file_stem("task-1")
        ));
        writeln!(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap(),
            "{line}"
        )
        .unwrap();
    }

    fn send(&self, line: &str) {
        self.append(line);
        self.runtime.sink().send(line.into()).unwrap();
    }

    fn processed(&self, event_id: &str) {
        wait_until(|| {
            self.runtime
                .shared
                .lock()
                .unwrap()
                .states
                .get("task-1")
                .is_some_and(|state| state.last_seen_event_id.as_deref() == Some(event_id))
        });
    }

    fn task(&self) -> &Task {
        self.context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "watcher did not process evidence"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn line(id: &str, kind: &str, detail: Value) -> String {
    json!({"event_id": id, "task_id": "task-1", "run_id": "primary", "kind": kind, "detail": detail, "occurred_at_unix_millis": now_ms(), "received_at_unix_millis": now_ms()}).to_string()
}

fn completed(id: &str) -> String {
    line(
        id,
        "turn_settled",
        json!({"outcome": {"outcome": "completed"}}),
    )
}

#[test]
fn normal_progress_produces_no_nudge() {
    let fixture = Fixture::new(Judge(stuck));
    fixture.send(&line("start", "turn_started", Value::Null));
    fixture.send(&line("tool", "activity_finished", json!({"activity": {"activity": "tool", "activity_id": "t1", "signature": "edit:parser", "success": true}})));
    fixture.send(&completed("done"));
    fixture.processed("done");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn permission_and_question_events_never_nudge() {
    for attention in ["permission", "question"] {
        let fixture = Fixture::new(Judge(stuck));
        fixture.send(&line(
            "attention",
            "attention_requested",
            json!({"attention": {"attention": attention}}),
        ));
        fixture.send(&completed("done"));
        fixture.processed("done");
        assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
    }
}

#[test]
fn suspicious_completion_nudges_once_despite_duplicate_socket_and_hook_lines() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let fixture = Fixture::new(Judge(move |snapshot: &WatcherSnapshot| {
        assert_eq!(snapshot.objective, "Fix the parser");
        assert_eq!(snapshot.harness, "cursor");
        observed.fetch_add(1, Ordering::Relaxed);
        stuck(snapshot)
    }));
    let notify = completed("done");
    fixture.send(&notify);
    fixture.processed("done");
    fixture.runtime.sink().send(notify.clone()).unwrap();
    fixture.send(&notify);
    fixture.send(&line("after", "turn_started", Value::Null));
    fixture.processed("after");
    let shared = fixture.runtime.shared.lock().unwrap();
    assert_eq!(shared.outbox.len(), 1);
    assert_eq!(shared.outbox[0].nudge_id, "watcher-task-1-primary-1");
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn judge_errors_timeout_and_panic_fail_open() {
    for error in [
        JudgeError::Unavailable,
        JudgeError::Timeout,
        JudgeError::Malformed,
    ] {
        let fixture = Fixture::new(Judge(move |_: &WatcherSnapshot| Err(error)));
        fixture.send(&completed("done"));
        fixture.processed("done");
        assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
    }
    let fixture = Fixture::new(Judge(
        |_: &WatcherSnapshot| -> Result<WatcherVerdict, JudgeError> { panic!("judge failure") },
    ));
    fixture.send(&completed("panic"));
    fixture.processed("panic");
    fixture.send(&line("after", "turn_started", Value::Null));
    fixture.processed("after");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn host_timeout_keeps_consuming_without_spawning_more_judges() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let (release, blocked) = mpsc::channel();
    let blocked = Mutex::new(blocked);
    let fixture = Fixture::new(Judge(move |snapshot: &WatcherSnapshot| {
        observed.fetch_add(1, Ordering::Relaxed);
        let _ = blocked.lock().unwrap().recv_timeout(Duration::from_secs(2));
        stuck(snapshot)
    }));
    fixture.send(&completed("first"));
    fixture.processed("first");
    fixture.send(&completed("second"));
    fixture.processed("second");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    release.send(()).unwrap();
}

struct NoCommands;

impl CommandRunner for NoCommands {
    fn run(&mut self, _: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        panic!("created tasks need no substrate commands");
    }
}

#[test]
fn ordinary_task_bridge_refresh_enqueues_notification_without_changing_status() {
    let mut fixture = Fixture::new(Judge(stuck));
    assert!(fixture.task().metadata.is_empty());
    let before = fixture.task().clone();
    fixture.runtime.shared.lock().unwrap().frames.clear();
    let mut bridge =
        crate::web_backend::CliRuntimeBridge::for_context(None, &fixture.context).unwrap();
    bridge
        .refresh_cockpit(
            &mut fixture.context,
            &mut NoCommands,
            RefreshTier::Live,
            false,
        )
        .unwrap();
    fixture.send(&completed("done"));
    fixture.processed("done");
    assert!(bridge
        .refresh_cockpit(
            &mut fixture.context,
            &mut NoCommands,
            RefreshTier::Live,
            false
        )
        .unwrap());
    assert!(matches!(
        pending_watcher_nudge(fixture.task()),
        Some(AgentNotification::WatcherNudge { .. })
    ));
    assert_eq!(fixture.task().lifecycle_status, before.lifecycle_status);
    assert_eq!(fixture.task().agent_status, before.agent_status);
    assert_eq!(fixture.task().live_status, before.live_status);
    assert_eq!(
        load_watcher_state(fixture.task())
            .unwrap()
            .intervention_count,
        1
    );
}

#[test]
fn permission_arriving_during_judgment_invalidates_the_verdict() {
    let (entered, waiting) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let blocked = Mutex::new(blocked);
    let fixture = Fixture::new(Judge(move |snapshot: &WatcherSnapshot| {
        entered.send(()).unwrap();
        blocked
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        stuck(snapshot)
    }));
    fixture.send(&completed("done"));
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    fixture.send(&line(
        "permission",
        "attention_requested",
        json!({"attention": {"attention": "permission"}}),
    ));
    release.send(()).unwrap();
    fixture.processed("permission");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn duplicate_appended_during_judgment_still_produces_one_nudge() {
    let (entered, waiting) = mpsc::channel();
    let (release, blocked) = mpsc::channel();
    let blocked = Mutex::new(blocked);
    let fixture = Fixture::new(Judge(move |snapshot: &WatcherSnapshot| {
        entered.send(()).unwrap();
        blocked
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        stuck(snapshot)
    }));
    let notify = completed("done");
    fixture.send(&notify);
    waiting.recv_timeout(Duration::from_secs(2)).unwrap();
    fixture.send(&notify);
    release.send(()).unwrap();
    fixture.processed("done");
    assert_eq!(fixture.runtime.shared.lock().unwrap().outbox.len(), 1);
}

#[test]
fn refresh_tick_checks_recovery_grace_without_new_socket_evidence() {
    let mut fixture = Fixture::new(Judge(stuck));
    fixture.send(&completed("done"));
    fixture.processed("done");
    fixture.runtime.refresh(&mut fixture.context);
    let mut persisted = load_watcher_state(fixture.task()).unwrap();
    persisted.grace_deadline_ms = Some(0);
    store_watcher_state(
        fixture
            .context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap(),
        &persisted,
    );
    let restarted = WatcherRuntime::start(fixture.events_dir(), Arc::new(Judge(stuck))).unwrap();
    restarted.refresh(&mut fixture.context);
    wait_until(|| !restarted.shared.lock().unwrap().outbox.is_empty());
    let shared = restarted.shared.lock().unwrap();
    assert_eq!(shared.outbox.len(), 1);
    assert_eq!(shared.outbox[0].reason, WatcherReason::GraceExpired);
    assert_eq!(shared.outbox[0].persisted_state.intervention_count, 2);
    assert_eq!(
        shared.outbox[0]
            .persisted_state
            .last_seen_event_id
            .as_deref(),
        Some("done")
    );
}

#[test]
fn child_completion_never_becomes_a_parent_checkpoint() {
    let fixture = Fixture::new(Judge(stuck));
    let mut child: Value = serde_json::from_str(&completed("child-done")).unwrap();
    child["run_id"] = json!("child");
    child["parent_run_id"] = json!("primary");
    fixture.send(&child.to_string());
    fixture.send(&line("barrier", "turn_started", Value::Null));
    fixture.processed("barrier");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn partial_journal_append_is_retried_after_the_newline_arrives() {
    let fixture = Fixture::new(Judge(stuck));
    fixture.send(&line("ready", "turn_started", Value::Null));
    fixture.processed("ready");
    let notify = completed("done");
    let path = fixture.events_dir().join(format!(
        "{}.jsonl",
        crate::agent_runtime::task_file_stem("task-1")
    ));
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    write!(file, "{notify}").unwrap();
    fixture.runtime.sink().send(notify.clone()).unwrap();
    writeln!(file).unwrap();
    fixture.runtime.sink().send(notify).unwrap();
    fixture.processed("done");
    assert_eq!(fixture.runtime.shared.lock().unwrap().outbox.len(), 1);
}

#[cfg(unix)]
#[test]
fn real_socket_triggers_durable_evidence_and_duplicate_notification_is_harmless() {
    use std::os::unix::net::UnixStream;
    struct SocketOverride;
    impl Drop for SocketOverride {
        fn drop(&mut self) {
            crate::agent_event::set_test_notify_socket_override(None);
        }
    }
    let fixture = Fixture::new(Judge(stuck));
    fixture.send(&line("ready", "turn_started", Value::Null));
    fixture.processed("ready");
    let socket = fixture.root.join("notify.sock");
    crate::agent_event::set_test_notify_socket_override(Some(socket.clone()));
    let _override = SocketOverride;
    crate::agent_event_notify::start_agent_event_notify_listener_with_sink(
        fixture.events_dir(),
        fixture.runtime.sink(),
    )
    .unwrap();
    let notify = completed("done");
    fixture.append(&notify);
    for _ in 0..2 {
        writeln!(UnixStream::connect(&socket).unwrap(), "{notify}").unwrap();
    }
    fixture.processed("done");
    assert_eq!(fixture.runtime.shared.lock().unwrap().outbox.len(), 1);
}

#[test]
fn refresh_recovers_missed_notifications_and_restart_does_not_repeat_nudge() {
    let mut fixture = Fixture::new(Judge(stuck));
    fixture.append(&completed("done"));
    fixture.runtime.refresh(&mut fixture.context);
    fixture.processed("done");
    fixture.runtime.refresh(&mut fixture.context);
    let restarted = WatcherRuntime::start(fixture.events_dir(), Arc::new(Judge(stuck))).unwrap();
    restarted.refresh(&mut fixture.context);
    wait_until(|| {
        restarted
            .shared
            .lock()
            .unwrap()
            .states
            .contains_key("task-1")
    });
    let shared = restarted.shared.lock().unwrap();
    assert!(shared.outbox.is_empty());
    assert_eq!(shared.states["task-1"].intervention_count, 1);
    assert_eq!(
        shared.states["task-1"].last_seen_event_id.as_deref(),
        Some("done")
    );
}

#[test]
fn socket_payload_without_journal_evidence_and_unknown_tasks_are_ignored() {
    let fixture = Fixture::new(Judge(stuck));
    fixture
        .runtime
        .sink()
        .send(completed("socket-only"))
        .unwrap();
    let mut unknown: Value = serde_json::from_str(&completed("unknown")).unwrap();
    unknown["task_id"] = json!("unknown-task");
    fixture.append(&unknown.to_string());
    fixture.runtime.sink().send(unknown.to_string()).unwrap();
    fixture.send(&line("barrier", "turn_started", Value::Null));
    fixture.processed("barrier");
    assert!(fixture.runtime.shared.lock().unwrap().outbox.is_empty());
}

#[test]
fn removed_task_cannot_receive_a_queued_nudge() {
    let mut fixture = Fixture::new(Judge(stuck));
    fixture.send(&completed("done"));
    fixture.processed("done");
    fixture
        .context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Removed;
    fixture.runtime.refresh(&mut fixture.context);
    assert!(pending_watcher_nudge(fixture.task()).is_none());
}
