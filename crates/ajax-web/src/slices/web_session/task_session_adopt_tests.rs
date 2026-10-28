use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ajax_core::models::AgentClient;
use ajax_core::registry::SqliteRegistryStore;
use serde_json::{json, Value};

use super::process_identity::start_token;
use super::task_session_adopt::{adoptable_handles, REFUSED_ATTACH_FOR_TEST};
use super::test_support::{fake_acp_fixture, BlockingSessionDirectory};
use super::{run_headless_dispatch, HeadlessTarget, SessionServerEvent};
use crate::adapters::session_shim::shim_paths;
use crate::adapters::session_shim::spawn::{probe_live, spawn_test_shim};
use crate::adapters::web_session_acp::with_test_acp_program;
use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger, PromptPhase};
use crate::adapters::web_session_store::{self, record_child_identity};

const SCRATCH_ROOT: &str = "../../target/web-adopt-directory-tests";
const INTERRUPTED_NOTE: &str = "was interrupted by host restart";
const WAIT: Duration = Duration::from_secs(5);

struct LiveShim {
    state_dir: PathBuf,
    worktree: String,
    serve: Option<JoinHandle<io::Result<i32>>>,
}

impl LiveShim {
    fn start(state_dir: &Path, worktree: &str) -> Self {
        let serve = spawn_test_shim(state_dir, worktree);
        let deadline = Instant::now() + WAIT;
        while !probe_live(state_dir, Path::new(worktree)) {
            assert!(Instant::now() < deadline, "shim never listened");
            thread::sleep(Duration::from_millis(10));
        }
        Self {
            state_dir: state_dir.to_path_buf(),
            worktree: worktree.to_owned(),
            serve: Some(serve),
        }
    }

    fn log_len(&self) -> u64 {
        std::fs::metadata(shim_paths(&self.state_dir, &self.worktree).log)
            .map_or(0, |metadata| metadata.len())
    }

    fn seed_outbound(&self, lines: &[Value]) {
        let socket = shim_paths(&self.state_dir, &self.worktree).socket;
        let mut stream = UnixStream::connect(socket).expect("connect writer");
        stream.write_all(b"REPLAY 0\n").expect("write hello");
        let mut expected = 0;
        for line in lines {
            let text = format!("{line}\n");
            expected += text.len() as u64;
            stream.write_all(text.as_bytes()).expect("write line");
        }
        drop(stream);
        let deadline = Instant::now() + WAIT;
        while self.log_len() < expected {
            assert!(Instant::now() < deadline, "harness output never logged");
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for LiveShim {
    fn drop(&mut self) {
        let Some(serve) = self.serve.take() else {
            return;
        };
        let socket = shim_paths(&self.state_dir, &self.worktree).socket;
        if let Ok(mut stream) = UnixStream::connect(socket) {
            let _ = stream.set_read_timeout(Some(WAIT));
            if stream.write_all(b"REPLAY 0\nSHUTDOWN\n").is_ok() {
                let _ = stream.read_to_end(&mut Vec::new());
            }
        }
        let _ = serve.join();
    }
}

struct Scratch {
    dir: PathBuf,
    worktree: PathBuf,
    store: SqliteRegistryStore,
    handle: String,
}

fn scratch(label: &str) -> Scratch {
    let dir = PathBuf::from(SCRATCH_ROOT).join(format!("{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let worktree = std::env::current_dir().expect("cwd").join(&dir);
    let store = SqliteRegistryStore::new(dir.join("ajax.db"));
    let mut task = crate::test_support::fix_login_task();
    task.worktree_path = worktree.clone();
    let handle = task.qualified_handle();
    let registry = crate::test_support::context_with_tasks(&["web"], vec![task]).registry;
    store.save(&registry).expect("seed registry");
    Scratch {
        dir,
        worktree,
        store,
        handle,
    }
}

impl Scratch {
    fn persist_dispatching(&self) {
        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "p1".to_owned(),
            "transcript p1".to_owned(),
            "prompt p1".to_owned(),
            Vec::new(),
        );
        assert!(ledger.mark_dispatching("p1"));
        ledger.record_dispatch_marks("p1", Some(0), 1);
        prompt_ledger::try_persist(&self.store, &self.dir, &self.handle, &ledger)
            .expect("persist ledger");
    }

    fn phase_of_p1(&self) -> PromptPhase {
        prompt_ledger::try_load(&self.store, &self.dir, &self.handle)
            .expect("load ledger")
            .entry("p1")
            .expect("p1 exists")
            .phase
    }

    fn interruption_notes(&self) -> usize {
        let session =
            web_session_store::try_load::<Value>(&self.store, &self.dir, &self.handle, "cursor")
                .expect("load transcript");
        session
            .events
            .iter()
            .filter(|event| event.to_string().contains(INTERRUPTED_NOTE))
            .count()
    }
}

struct SleepChild {
    child: Child,
}

impl SleepChild {
    fn spawn() -> Self {
        let child = Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        Self { child }
    }

    fn pid(&self) -> i64 {
        i64::from(self.child.id())
    }
}

impl Drop for SleepChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn session_new_count(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join(".fake-acp-session-new-count"))
        .ok()
        .and_then(|text| text.trim().parse().ok())
        .unwrap_or(0)
}

fn agent_text_count(directory: &BlockingSessionDirectory, handle: &str, needle: &str) -> usize {
    let (events, _) = directory.read_from(handle, 0);
    events
        .iter()
        .filter(|event| match event {
            SessionServerEvent::Message { text, .. } => text == needle,
            _ => false,
        })
        .count()
}

fn pump_until_turn_end(directory: &BlockingSessionDirectory, handle: &str) {
    let deadline = Instant::now() + WAIT;
    loop {
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        if events
            .iter()
            .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. }))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "adopted turn never ended; events={events:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn agent_message(text: &str, item_id: &str) -> SessionServerEvent {
    SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: item_id.to_string(),
        message_id: None,
    }
}

fn chunk_line() -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {
            "sessionId": "sess-1",
            "update": {
                "sessionUpdate": "agent_message_chunk",
                "content": { "type": "text", "text": "hello" }
            }
        }
    })
}

fn stop_line() -> Value {
    json!({ "jsonrpc": "2.0", "id": 0, "result": { "stopReason": "end_turn" } })
}

#[test]
fn live_shim_is_adopted_not_interrupted() {
    let scratch = scratch("adopt-ok");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    let seed = agent_message("seed", "seed-agent");
    web_session_store::try_append_events(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        &[seed],
    )
    .expect("seed transcript");
    web_session_store::try_append_events(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        &[agent_message("hello", "chunk-after-cursor")],
    )
    .expect("commit event after cursor");
    web_session_store::try_save_meta(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        Some("sess-1"),
        "auto",
    )
    .expect("save session");
    record_child_identity(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        start_token(shim_child.pid()).map(|token| (shim_child.pid(), token)),
        "foreign-boot",
    )
    .expect("record shim identity");
    scratch.persist_dispatching();
    shim.seed_outbound(&[chunk_line(), stop_line()]);
    let log_before = shim.log_len();

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("acquire adopts the live shim");
    });
    pump_until_turn_end(&directory, &scratch.handle);

    assert_eq!(
        session_new_count(&scratch.worktree),
        0,
        "adoption must not spawn"
    );
    assert_eq!(
        shim.log_len(),
        log_before,
        "adoption must not write a prompt"
    );
    assert_eq!(agent_text_count(&directory, &scratch.handle, "hello"), 1);
    assert_eq!(agent_text_count(&directory, &scratch.handle, "seed"), 1);
    assert_eq!(scratch.phase_of_p1(), PromptPhase::Completed);

    drop(directory);
    drop(shim);
    drop(shim_child);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

#[test]
fn adoption_failure_interrupts_like_today() {
    let scratch = scratch("adopt-fail");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    scratch.persist_dispatching();

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Cursor,
            )
            .expect("acquire falls back to the normal spawn path");
    });

    let deadline = Instant::now() + WAIT;
    while probe_live(&scratch.dir, Path::new(&worktree)) {
        assert!(Instant::now() < deadline, "shim was not told to shut down");
        thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(scratch.phase_of_p1(), PromptPhase::Interrupted);
    assert_eq!(scratch.interruption_notes(), 1);
    assert_eq!(
        session_new_count(&scratch.worktree),
        1,
        "normal spawn continued"
    );

    drop(directory);
    drop(shim);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

fn permission_request_line() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "session/request_permission",
        "params": { "sessionId": "sess-1" }
    })
}

fn seed_live_turn(scratch: &Scratch, shim_child: &SleepChild) {
    web_session_store::try_append_events(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        &[agent_message("seed", "seed-agent")],
    )
    .expect("seed transcript");
    web_session_store::try_append_events(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        &[agent_message("hello", "chunk-after-cursor")],
    )
    .expect("commit event after cursor");
    web_session_store::try_save_meta(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        Some("fake-sess-1"),
        "auto",
    )
    .expect("save session");
    std::fs::write(
        scratch.worktree.join(".fake-acp-sessions"),
        r#"["fake-sess-1"]"#,
    )
    .expect("fixture knows the stored session");
    record_child_identity(
        &scratch.store,
        &scratch.dir,
        &scratch.handle,
        "codex",
        start_token(shim_child.pid()).map(|token| (shim_child.pid(), token)),
        "foreign-boot",
    )
    .expect("record shim identity");
    scratch.persist_dispatching();
}

fn event_text_count(directory: &BlockingSessionDirectory, handle: &str, needle: &str) -> usize {
    let (events, _) = directory.read_from(handle, 0);
    events
        .iter()
        .filter(|event| format!("{event:?}").contains(needle))
        .count()
}

fn pump_until_interrupted(directory: &BlockingSessionDirectory, scratch: &Scratch) {
    let deadline = Instant::now() + WAIT;
    while scratch.phase_of_p1() != PromptPhase::Interrupted {
        directory.pump(&scratch.handle);
        assert!(
            Instant::now() < deadline,
            "adopted prompt was never interrupted"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn refuse_attach(worktree: &str) {
    REFUSED_ATTACH_FOR_TEST
        .lock()
        .expect("refusal list lock")
        .push(worktree.to_owned());
}

fn accept_attach(worktree: &str) {
    REFUSED_ATTACH_FOR_TEST
        .lock()
        .expect("refusal list lock")
        .retain(|refused| refused != worktree);
}

#[test]
fn adopted_turn_end_shuts_shim_then_next_acquire_resumes() {
    let scratch = scratch("adopt-release");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    seed_live_turn(&scratch, &shim_child);
    shim.seed_outbound(&[chunk_line(), stop_line()]);

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("acquire adopts the live shim");
    });
    pump_until_turn_end(&directory, &scratch.handle);

    assert_eq!(
        directory.child_id(&scratch.handle),
        None,
        "the adopted slot must be empty once its turn ends"
    );
    let deadline = Instant::now() + WAIT;
    while probe_live(&scratch.dir, Path::new(&worktree)) {
        assert!(Instant::now() < deadline, "shim never received SHUTDOWN");
        thread::sleep(Duration::from_millis(10));
    }
    drop(shim_child);
    for needle in ["interrupted", "ACP process exited", "Model context reset"] {
        assert_eq!(
            event_text_count(&directory, &scratch.handle, needle),
            0,
            "release must not write a `{needle}` note"
        );
    }

    let spawned_before = session_new_count(&scratch.worktree);
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("next acquire resumes the stored session");
    });
    assert_eq!(
        session_new_count(&scratch.worktree),
        spawned_before,
        "the next prompt must resume, not start a new session"
    );
    assert!(directory.child_id(&scratch.handle).is_some());

    drop(directory);
    drop(shim);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

#[test]
fn adopted_eof_mid_turn_interrupts() {
    let scratch = scratch("adopt-eof");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    seed_live_turn(&scratch, &shim_child);
    shim.seed_outbound(&[chunk_line(), permission_request_line()]);

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("acquire adopts the live shim");
    });
    pump_until_interrupted(&directory, &scratch);

    assert_eq!(
        event_text_count(
            &directory,
            &scratch.handle,
            "was interrupted because the ACP process exited"
        ),
        1
    );

    drop(directory);
    drop(shim);
    drop(shim_child);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

#[test]
fn adoption_attach_failure_keeps_transcript() {
    let scratch = scratch("adopt-attach-refused");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    seed_live_turn(&scratch, &shim_child);
    drop(shim_child);
    refuse_attach(&worktree);

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("acquire falls back to the normal resume path");
    });
    accept_attach(&worktree);

    assert_eq!(scratch.phase_of_p1(), PromptPhase::Interrupted);
    assert_eq!(
        event_text_count(&directory, &scratch.handle, INTERRUPTED_NOTE),
        1
    );
    assert_eq!(
        event_text_count(&directory, &scratch.handle, "hello"),
        1,
        "events committed after the cursor must survive a failed attach"
    );

    drop(directory);
    drop(shim);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

fn phase_of(scratch: &Scratch, id: &str) -> Option<PromptPhase> {
    prompt_ledger::try_load(&scratch.store, &scratch.dir, &scratch.handle)
        .expect("load ledger")
        .entry(id)
        .map(|entry| entry.phase)
}

#[test]
fn adoptable_handles_lists_only_live_dispatching_handles() {
    let scratch = scratch("adoptable-handles");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let mut queued_only = PromptLedger::default();
    queued_only.upsert_queued(
        "p1".to_owned(),
        "transcript p1".to_owned(),
        "prompt p1".to_owned(),
        Vec::new(),
    );
    prompt_ledger::try_persist(&scratch.store, &scratch.dir, &scratch.handle, &queued_only)
        .expect("persist queued ledger");
    assert!(
        adoptable_handles(&scratch.store, &scratch.dir).is_empty(),
        "a queued prompt has no dispatch offsets to adopt from"
    );

    scratch.persist_dispatching();
    assert_eq!(
        adoptable_handles(&scratch.store, &scratch.dir),
        vec![scratch.handle.clone()]
    );

    drop(shim);
    assert!(
        adoptable_handles(&scratch.store, &scratch.dir).is_empty(),
        "a shim that stopped listening is not adoptable"
    );
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

#[test]
fn startup_dispatch_finishes_adopted_turn_without_a_browser() {
    let scratch = scratch("adopt-startup");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    seed_live_turn(&scratch, &shim_child);
    shim.seed_outbound(&[chunk_line(), stop_line()]);

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    let handles = adoptable_handles(&scratch.store, &scratch.dir);
    let targets = handles
        .iter()
        .map(|handle| HeadlessTarget {
            handle: handle.clone(),
            worktree_path: scratch.worktree.clone(),
            model: "auto".to_owned(),
            agent: AgentClient::Codex,
        })
        .collect();
    let results = with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .runtime_handle()
            .block_on(run_headless_dispatch(directory.inner(), targets, WAIT))
    });

    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok(), "startup dispatch failed: {results:?}");
    assert_eq!(scratch.phase_of_p1(), PromptPhase::Completed);
    assert_eq!(agent_text_count(&directory, &scratch.handle, "hello"), 1);
    assert_eq!(
        session_new_count(&scratch.worktree),
        0,
        "startup adoption must not spawn"
    );

    directory.drop_session(&scratch.handle);
    drop(directory);
    drop(shim);
    drop(shim_child);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}

#[test]
fn adopted_turn_with_queued_prompt_dispatches_after_release() {
    let scratch = scratch("adopt-queued");
    let worktree = scratch.worktree.to_str().expect("utf-8 path").to_owned();
    let shim = LiveShim::start(&scratch.dir, &worktree);
    let shim_child = SleepChild::spawn();
    seed_live_turn(&scratch, &shim_child);
    shim.seed_outbound(&[chunk_line(), stop_line()]);

    let directory = BlockingSessionDirectory::new(scratch.dir.clone());
    with_test_acp_program(&fake_acp_fixture(), || {
        directory
            .acquire(
                &scratch.handle,
                &scratch.worktree,
                "auto",
                AgentClient::Codex,
            )
            .expect("acquire adopts the live shim");
        directory
            .submit_prompt_with_id(&scratch.handle, "p2".to_owned(), "queued prompt".to_owned())
            .expect("queue a prompt behind the adopted turn");
        let spawned_before = session_new_count(&scratch.worktree);
        let deadline = Instant::now() + WAIT;
        while phase_of(&scratch, "p2") != Some(PromptPhase::Completed) {
            directory.pump(&scratch.handle);
            assert!(
                Instant::now() < deadline,
                "queued prompt was never dispatched after the adopted turn ended"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            session_new_count(&scratch.worktree),
            spawned_before,
            "the queued prompt must resume the stored session, not start a new one"
        );
        assert!(directory.child_id(&scratch.handle).is_some());
    });
    assert_eq!(scratch.phase_of_p1(), PromptPhase::Completed);

    directory.drop_session(&scratch.handle);
    drop(directory);
    drop(shim);
    drop(shim_child);
    let _ = std::fs::remove_dir_all(&scratch.dir);
}
