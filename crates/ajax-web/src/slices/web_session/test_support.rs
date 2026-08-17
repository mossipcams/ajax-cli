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

    pub fn submit_prompt(&self, handle: &str, text: String) -> Result<(), String> {
        self.rt.block_on(self.inner.submit_prompt(handle, text))
    }

    pub fn submit_prompt_with_id(
        &self,
        handle: &str,
        client_message_id: String,
        text: String,
    ) -> Result<(), String> {
        self.rt.block_on(
            self.inner
                .submit_prompt_with_id(handle, client_message_id, text),
        )
    }

    pub fn cancel(&self, handle: &str, keep_queue: bool) -> Result<(), String> {
        self.rt.block_on(self.inner.cancel(handle, keep_queue))
    }

    pub fn answer_permission(
        &self,
        handle: &str,
        request_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), String> {
        self.rt.block_on(
            self.inner
                .answer_permission(handle, request_id, approved, reason),
        )
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
        message_id: None,
    }
}

pub(crate) fn user_msg(text: &str) -> super::SessionServerEvent {
    super::SessionServerEvent::Message {
        role: "user".to_string(),
        text: text.to_string(),
        message_id: None,
    }
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
