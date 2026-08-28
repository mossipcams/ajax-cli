//! Blocking test helpers over the async task-session directory.

use super::task_session_directory::TaskSessionDirectory;
use ajax_core::models::AgentClient;
use std::{path::Path, sync::Arc};
use tokio::runtime::Runtime;

pub(crate) struct BlockingSessionDirectory {
    inner: Arc<TaskSessionDirectory>,
    rt: Runtime,
}

impl BlockingSessionDirectory {
    pub fn new(state_dir: std::path::PathBuf) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("test runtime");
        let inner = TaskSessionDirectory::new(state_dir);
        Self { inner, rt }
    }

    pub fn inner(&self) -> &Arc<TaskSessionDirectory> {
        &self.inner
    }

    pub fn acquire(
        &self,
        handle: &str,
        worktree: &Path,
        model: &str,
        agent: AgentClient,
    ) -> Result<(), String> {
        self.rt
            .block_on(self.inner.acquire(handle, worktree, model, agent))
    }

    pub fn release(&self, handle: &str) {
        self.rt.block_on(self.inner.release(handle));
    }

    pub fn drop_session(&self, handle: &str) {
        self.rt.block_on(self.inner.drop_session(handle));
    }

    pub fn detach_session(&self, handle: &str) {
        self.rt.block_on(self.inner.detach_session(handle));
    }

    pub fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.rt.handle().clone()
    }

    pub fn submit_prompt(&self, handle: &str, text: String) -> Result<(), String> {
        self.rt.block_on(self.inner.submit_prompt(handle, text))
    }

    pub fn submit_prompt_with_id(
        &self,
        handle: &str,
        client_message_id: String,
        text: String,
    ) -> Result<(), String> {
        self.rt.block_on(self.inner.submit_prompt_with_id(
            handle,
            client_message_id,
            text,
            Vec::new(),
        ))
    }

    pub fn cancel(&self, handle: &str, keep_queue: bool) -> Result<(), String> {
        self.rt.block_on(self.inner.cancel(handle, keep_queue))
    }

    pub fn cleanup_session(&self, handle: &str) {
        self.rt.block_on(self.inner.cleanup_session(handle));
    }

    pub fn read_from(
        &self,
        handle: &str,
        cursor: usize,
    ) -> (Vec<super::SessionServerEvent>, usize) {
        self.rt.block_on(self.inner.read_from(handle, cursor))
    }

    pub fn pump(&self, handle: &str) {
        self.rt.block_on(self.inner.pump(handle));
    }

    pub fn record(&self, handle: &str, event: super::SessionServerEvent) {
        self.rt.block_on(self.inner.clone().record(handle, event));
    }

    pub fn child_id(&self, handle: &str) -> Option<u32> {
        self.rt.block_on(self.inner.child_id(handle))
    }

    pub fn kill_host_for_test(&self, handle: &str) {
        self.rt.block_on(self.inner.kill_host_for_test(handle));
    }

    pub fn generation(&self, handle: &str) -> u64 {
        self.rt.block_on(self.inner.generation(handle))
    }

    pub fn eviction_snapshot(&self, handle: &str) -> Option<super::task_session::EvictionSnapshot> {
        self.rt.block_on(self.inner.eviction_snapshot(handle))
    }

    pub fn is_marked_idle_release(&self, handle: &str) -> Option<bool> {
        self.inner.is_marked_idle_release(handle)
    }

    pub fn attach_snapshot(
        &self,
        handle: &str,
        model: &str,
    ) -> super::task_session::AttachSnapshot {
        self.rt
            .block_on(self.inner.attach_snapshot(handle, model.to_string(), None))
    }

    pub fn collect_outbound(
        &self,
        handle: &str,
        cursor: usize,
        generation: u64,
    ) -> super::task_session::OutboundBatch {
        self.rt
            .block_on(self.inner.collect_outbound(handle, cursor, generation))
    }

    pub fn stored_acp_session_id(&self, state_dir: &Path, handle: &str) -> Option<String> {
        crate::adapters::web_session_store::load::<super::SessionServerEvent>(state_dir, handle)
            .acp_session_id
    }
}

pub(crate) fn scratch_dir(label: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dir = std::env::temp_dir().join(format!(
        "ajax-web-session-tests-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn fake_acp_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js")
}

pub(crate) fn note(text: &str) -> super::SessionServerEvent {
    super::SessionServerEvent::Message {
        role: "agent".to_string(),
        text: text.to_string(),
        content_blocks: Vec::new(),
        item_id: format!("note-{text}"),
        message_id: None,
    }
}

pub(crate) fn has_message(events: &[super::SessionServerEvent], role: &str, text: &str) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            super::SessionServerEvent::Message { role: actual_role, text: actual_text, .. }
                if actual_role == role && actual_text == text
        )
    })
}

pub(crate) fn pump_until<F>(
    directory: &BlockingSessionDirectory,
    handle: &str,
    timeout: std::time::Duration,
    mut done: F,
) where
    F: FnMut(&[super::SessionServerEvent]) -> bool,
{
    use std::thread;
    use std::time::Instant;
    let deadline = Instant::now() + timeout;
    loop {
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        if done(&events) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out; events={events:?}");
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
}

pub(crate) fn agent_pong_count(events: &[super::SessionServerEvent]) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(
                event,
                super::SessionServerEvent::Message { role, text, .. }
                    if role == "agent" && text == "pong"
            )
        })
        .count()
}

pub(crate) fn fake_context_memory_path(worktree: &Path) -> std::path::PathBuf {
    worktree.join(".fake-acp-context-memory")
}

pub(crate) fn read_fake_context_memory(worktree: &Path) -> Option<String> {
    std::fs::read_to_string(fake_context_memory_path(worktree)).ok()
}

pub(crate) fn read_fake_acp_methods(worktree: &Path) -> Vec<String> {
    let path = worktree.join(".fake-acp-methods");
    let raw = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&raw).unwrap_or_default()
}

pub(crate) fn remember_context(
    directory: &BlockingSessionDirectory,
    handle: &str,
    worktree: &Path,
    nonce: &str,
) {
    directory
        .submit_prompt_with_id(
            handle,
            format!("remember-{nonce}"),
            format!("remember:{nonce}"),
        )
        .expect("remember prompt");
    pump_until(
        directory,
        handle,
        std::time::Duration::from_secs(5),
        |events| has_message(events, "agent", &format!("stored:{nonce}")),
    );
    assert_eq!(
        read_fake_context_memory(worktree).as_deref(),
        Some(nonce),
        "fake ACP must persist remembered context under the worktree state dir"
    );
}

pub(crate) const CONTEXT_RESET_NOTE: &str =
    "Model context reset after restart. Prior turns are still visible here.";

pub(crate) fn log_contains_text(
    directory: &BlockingSessionDirectory,
    handle: &str,
    needle: &str,
) -> bool {
    let (events, _) = directory.read_from(handle, 0);
    events.iter().any(|event| match event {
        super::SessionServerEvent::Message { text, .. } => text.contains(needle),
        _ => false,
    })
}

pub(crate) fn pump_until_pong_or_turn_end(
    directory: &BlockingSessionDirectory,
    handle: &str,
    timeout: std::time::Duration,
) {
    use std::thread;
    use std::time::Instant;
    let deadline = Instant::now() + timeout;
    loop {
        directory.pump(handle);
        let (events, _) = directory.read_from(handle, 0);
        let done = events.iter().any(|event| match event {
            super::SessionServerEvent::TurnEnd { .. } => true,
            super::SessionServerEvent::Message { text, .. } => text == "pong",
            _ => false,
        });
        if done {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for pong or turn_end; events={events:?}");
        }
        thread::sleep(std::time::Duration::from_millis(10));
    }
}

pub(crate) fn recall_context(directory: &BlockingSessionDirectory, handle: &str, nonce: &str) {
    let expected = format!("recalled:{nonce}");
    directory
        .submit_prompt_with_id(handle, format!("recall-{nonce}"), "recall".into())
        .expect("recall prompt");
    pump_until(
        directory,
        handle,
        std::time::Duration::from_secs(5),
        |events| {
            events.iter().any(|event| {
                matches!(
                    event,
                    super::SessionServerEvent::Message { role, text, .. }
                        if role == "agent"
                            && (text == &expected
                                || text.ends_with(&expected)
                                || text.contains(&expected))
                )
            })
        },
    );
}
