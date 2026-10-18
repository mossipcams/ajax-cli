use super::{
    execution_ownership::ExecutionOwnership,
    protocol::SessionChrome,
    task_session::{
        spawn_task_session, AttachSnapshot, EvictionSnapshot, OutboundBatch, TaskSessionCommand,
        TaskSessionSender,
    },
    task_session_spawn,
    transcript::MAX_IDLE_SESSIONS,
    PersistSessionModel, ReportSessionActivity, SessionClientMessage, SessionError,
    SessionServerEvent, SessionStorage,
};

async fn send_command<T>(
    tx: &TaskSessionSender,
    build: impl FnOnce(tokio::sync::oneshot::Sender<T>) -> TaskSessionCommand,
) -> Result<T, SessionError> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    tx.send(build(reply))
        .await
        .map_err(|_| SessionError::protocol("session task stopped"))?;
    rx.await
        .map_err(|_| SessionError::protocol("session task dropped reply"))
}
use crate::adapters::web_session_store::{self, prompt_ledger::PromptPhase};
use ajax_core::models::AgentClient;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

struct SessionEntry {
    command_tx: TaskSessionSender,
    join_handle: tokio::task::JoinHandle<()>,
    last_released: Option<Instant>,
}

pub(crate) struct TaskSessionDirectory {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    ownership: std::sync::Arc<ExecutionOwnership>,
    storage: SessionStorage,
    report_activity: Mutex<Option<ReportSessionActivity>>,
    headless_scanned: std::sync::atomic::AtomicBool,
    headless_handed_out: Mutex<HashSet<String>>,
}

impl TaskSessionDirectory {
    pub fn new(storage: SessionStorage) -> Arc<Self> {
        let ownership = std::sync::Arc::new(ExecutionOwnership::new(storage.clone()));
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            ownership,
            storage,
            report_activity: Mutex::new(None),
            headless_scanned: std::sync::atomic::AtomicBool::new(false),
            headless_handed_out: Mutex::new(HashSet::new()),
        })
    }

    pub(crate) fn set_report_session_activity(&self, report: ReportSessionActivity) {
        *self.report_activity.lock().unwrap() = Some(report);
    }

    /// Claims the execution lease (running the one-time orphan sweep and startup recovery) and returns, once per process, the handles whose queued prompts should be dispatched without a browser attached. An incomplete scan leaves the directory rescannable: previously handed-out handles are filtered out of every later result, and after a complete scan every later call returns an empty list. Fails like `ensure_owner` when a live foreign host holds the lease.
    pub(crate) fn startup_dispatch_handles(&self) -> Result<Vec<String>, String> {
        self.ownership.ensure_owner()?;
        self.report_recovered_turns();
        if self
            .headless_scanned
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Ok(Vec::new());
        }
        let scan = super::startup_recovery::queued_prompt_handles(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            self.ownership.boot_id(),
        );
        let mut handed_out = self.headless_handed_out.lock().unwrap();
        let fresh: Vec<String> = scan
            .handles
            .into_iter()
            .filter(|handle| handed_out.insert(handle.clone()))
            .collect();
        if scan.complete {
            self.headless_scanned
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(fresh)
    }

    /// True once a complete startup dispatch scan has run; the headless runtime checks this to stop rescan attempts.
    pub(crate) fn headless_scan_complete(&self) -> bool {
        self.headless_scanned
            .load(std::sync::atomic::Ordering::SeqCst)
    }
    /// Counts the chat's prompts that are not yet done (ledger entries in the
    /// Queued or Dispatching phase). A ledger load error reports 1 so a headless
    /// runner keeps waiting until its deadline instead of releasing a session it
    /// cannot verify.
    pub(crate) fn unfinished_prompt_count(&self, handle: &str) -> usize {
        match crate::adapters::web_session_store::prompt_ledger::try_load(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            handle,
        ) {
            Ok(ledger) => ledger
                .entries
                .iter()
                .filter(|entry| {
                    matches!(entry.phase, PromptPhase::Queued | PromptPhase::Dispatching)
                })
                .count(),
            Err(_) => 1,
        }
    }

    /// Renews the chat execution owner lease; called from the runtime heartbeat task.
    pub(crate) fn heartbeat_execution_ownership(&self) {
        self.ownership.heartbeat();
        self.report_recovered_turns();
    }

    /// Test accessor for the owning host's boot id.
    #[cfg(test)]
    pub(crate) fn ownership_boot_id(&self) -> String {
        self.ownership.boot_id().to_owned()
    }

    /// Test seed: put recovered handles back so heartbeat reporting can be exercised.
    #[cfg(test)]
    pub(crate) fn requeue_recovered_handles_for_test(&self, handles: Vec<String>) {
        self.ownership.requeue_recovered_handles(handles);
    }

    /// Report the turns that startup recovery interrupted as failed, so a restart never leaves a task on "Agent working" (#1176).
    /// A handle whose report returns false is requeued and retried on the next heartbeat.
    fn report_recovered_turns(&self) {
        let Some(report) = self.report_activity_for_spawn() else {
            return;
        };
        let mut failed = Vec::new();
        for handle in self.ownership.take_recovered_handles() {
            if !report(&handle, super::SessionActivity::TurnFailed) {
                failed.push(handle);
            }
        }
        self.ownership.requeue_recovered_handles(failed);
    }

    fn report_activity_for_spawn(&self) -> Option<ReportSessionActivity> {
        self.report_activity.lock().unwrap().clone()
    }

    #[cfg(test)]
    async fn ensure_entry(self: &Arc<Self>, handle: &str) -> Result<TaskSessionSender, String> {
        {
            let sessions = self.sessions.lock().unwrap();
            if let Some(entry) = sessions.get(handle) {
                return Ok(entry.command_tx.clone());
            }
        }
        self.evict_idle_over_limit().await;
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(entry) = sessions.get(handle) {
            return Ok(entry.command_tx.clone());
        }
        let (tx, join) = spawn_task_session(
            handle.to_string(),
            self.storage.clone(),
            self.report_activity_for_spawn(),
            self.ownership.clone(),
        );
        sessions.insert(
            handle.to_string(),
            SessionEntry {
                command_tx: tx.clone(),
                join_handle: join,
                last_released: None,
            },
        );
        Ok(tx)
    }

    async fn ensure_entry_for_acquire(
        self: &Arc<Self>,
        handle: &str,
    ) -> Result<TaskSessionSender, String> {
        self.ownership.ensure_owner()?;
        self.report_recovered_turns();
        {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(entry) = sessions.get_mut(handle) {
                entry.last_released = None;
                return Ok(entry.command_tx.clone());
            }
        }
        self.evict_idle_over_limit().await;
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(entry) = sessions.get_mut(handle) {
            entry.last_released = None;
            return Ok(entry.command_tx.clone());
        }
        let (tx, join) = spawn_task_session(
            handle.to_string(),
            self.storage.clone(),
            self.report_activity_for_spawn(),
            self.ownership.clone(),
        );
        sessions.insert(
            handle.to_string(),
            SessionEntry {
                command_tx: tx.clone(),
                join_handle: join,
                last_released: None,
            },
        );
        Ok(tx)
    }

    async fn evict_idle_over_limit(&self) {
        let grace = super::transcript::idle_release_grace();
        let candidates: Vec<(String, Instant, TaskSessionSender)> = {
            let sessions = self.sessions.lock().unwrap();
            sessions
                .iter()
                .filter_map(|(handle, entry)| {
                    let released = entry.last_released?;
                    if released.elapsed() < grace {
                        return None;
                    }
                    Some((handle.clone(), released, entry.command_tx.clone()))
                })
                .collect()
        };

        let mut idle = Vec::new();
        for (handle, released, tx) in candidates {
            if let Ok(snapshot) = eviction_snapshot(&tx).await {
                if snapshot.evictable {
                    idle.push((handle, released));
                }
            }
        }
        if idle.len() < MAX_IDLE_SESSIONS {
            return;
        }
        idle.sort_by_key(|(_, released)| *released);
        let to_evict: Vec<String> = idle
            .iter()
            .take(idle.len() - MAX_IDLE_SESSIONS + 1)
            .map(|(handle, _)| handle.clone())
            .collect();

        for handle in to_evict {
            let tx = {
                let sessions = self.sessions.lock().unwrap();
                sessions
                    .get(&handle)
                    .and_then(|entry| entry.last_released.map(|_| entry.command_tx.clone()))
            };
            let Some(tx) = tx else {
                continue;
            };

            if let Ok(snapshot) = eviction_snapshot(&tx).await {
                if !snapshot.evictable {
                    continue;
                }
            } else {
                continue;
            }

            let removed = {
                let mut sessions = self.sessions.lock().unwrap();
                match sessions.get(&handle) {
                    Some(entry) if entry.last_released.is_some() => {
                        let entry = sessions.remove(&handle).expect("entry present");
                        Some((entry.command_tx, entry.join_handle))
                    }
                    _ => None,
                }
            };

            if let Some((tx, join)) = removed {
                tokio::spawn(async move {
                    let _ = tx.send(TaskSessionCommand::Shutdown { close: false }).await;
                    let _ = join.await;
                });
            }
        }
    }

    pub async fn acquire(
        self: &Arc<Self>,
        qualified_handle: &str,
        worktree_path: &Path,
        model: &str,
        agent: AgentClient,
    ) -> Result<(), String> {
        self.acquire_typed(qualified_handle, worktree_path, model, agent)
            .await
            .map_err(Into::into)
    }

    pub async fn acquire_typed(
        self: &Arc<Self>,
        qualified_handle: &str,
        worktree_path: &Path,
        model: &str,
        agent: AgentClient,
    ) -> Result<(), SessionError> {
        let tx = Arc::clone(self)
            .ensure_entry_for_acquire(qualified_handle)
            .await
            .map_err(SessionError::protocol)?;
        let worktree_path = worktree_path.to_path_buf();
        let model = model.to_string();
        send_command(&tx, |reply| TaskSessionCommand::Acquire {
            worktree_path,
            model,
            agent,
            reply,
        })
        .await??;
        Ok(())
    }

    pub async fn release(&self, handle: &str) {
        let tx = {
            let sessions = self.sessions.lock().unwrap();
            sessions.get(handle).map(|entry| entry.command_tx.clone())
        };
        if let Some(tx) = tx {
            let _ = send_command(&tx, |reply| TaskSessionCommand::Release { reply }).await;
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(entry) = sessions.get_mut(handle) {
                entry.last_released = Some(Instant::now());
            }
        }
    }

    pub async fn drop_session(&self, handle: &str) {
        let removed = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(handle)
        };
        if let Some(entry) = removed {
            let _ = entry
                .command_tx
                .send(TaskSessionCommand::Shutdown { close: true })
                .await;
            let _ = entry.join_handle.await;
        }
        if let Err(error) = web_session_store::clear_acp_session_id(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            handle,
            "unknown",
        ) {
            tracing::warn!(%error, handle, "failed to clear stored ACP session id on drop");
        }
    }

    #[cfg(test)]
    pub async fn detach_session(&self, handle: &str) {
        let removed = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(handle)
        };
        if let Some(entry) = removed {
            let _ = entry
                .command_tx
                .send(TaskSessionCommand::Shutdown { close: false })
                .await;
            let _ = entry.join_handle.await;
        }
    }

    pub async fn cleanup_session(&self, handle: &str) {
        self.drop_session(handle).await;
        web_session_store::delete_session(
            &self.storage.registry_store(),
            &self.storage.state_dir,
            handle,
        );
    }

    pub fn prune_stale_persisted(&self, owned: &std::collections::HashSet<String>) {
        super::session_cleanup::prune_stale_persisted_sessions(&self.storage, owned);
    }

    pub async fn submit_prompt_with_id(
        &self,
        handle: &str,
        client_message_id: String,
        text: String,
        content_blocks: Vec<super::prompt_content::PromptContentBlockWire>,
    ) -> Result<(), String> {
        let tx = self.command_tx(handle)?;
        send_command(&tx, |reply| TaskSessionCommand::SubmitPrompt {
            client_message_id,
            text,
            content_blocks,
            reply,
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn cancel(&self, handle: &str, keep_queue: bool) -> Result<(), String> {
        let tx = self.command_tx(handle)?;
        send_command(&tx, |reply| TaskSessionCommand::Cancel {
            keep_queue,
            reply,
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn answer_permission(
        &self,
        handle: &str,
        request_id: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), String> {
        let tx = self.command_tx(handle)?;
        send_command(&tx, |reply| TaskSessionCommand::AnswerPermission {
            request_id: request_id.to_string(),
            approved,
            reason: reason.map(str::to_string),
            reply,
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn answer_elicitation(
        &self,
        handle: &str,
        request_id: &str,
        action: &str,
        content: Option<serde_json::Value>,
    ) -> Result<(), String> {
        let tx = self.command_tx(handle)?;
        send_command(&tx, |reply| TaskSessionCommand::AnswerElicitation {
            request_id: request_id.to_string(),
            action: action.to_string(),
            content,
            reply,
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn apply_config_option(
        &self,
        handle: &str,
        config_id: &str,
        value: crate::adapters::web_session_acp::SessionConfigValue,
    ) -> Result<task_session_spawn::ApplyConfigOptionResult, String> {
        let tx = self.command_tx(handle)?;
        let config_id = config_id.to_string();
        send_command(&tx, |reply| TaskSessionCommand::ApplyConfigOption {
            config_id,
            value,
            reply,
        })
        .await?
        .map_err(Into::into)
    }

    pub async fn reset_harness_context(
        &self,
        handle: &str,
        worktree_path: &Path,
        agent: AgentClient,
        model: &str,
    ) -> Result<(), String> {
        if self.has_live_entry(handle) {
            let tx = self.command_tx(handle)?;
            let worktree_path = worktree_path.to_path_buf();
            let model = model.to_string();
            send_command(&tx, |reply| TaskSessionCommand::ResetHarness {
                worktree_path,
                model,
                agent,
                reply,
            })
            .await??;
            Ok(())
        } else {
            discard_queued_prompts(&self.storage, handle)?;
            clear_stored_session_id(&self.storage, handle)
        }
    }

    pub async fn clear_context(&self, handle: &str, worktree_path: &Path) -> Result<u64, String> {
        if self.has_live_entry(handle) {
            let tx = self.command_tx(handle)?;
            let worktree_path = worktree_path.to_path_buf();
            send_command(&tx, |reply| TaskSessionCommand::ClearContext {
                worktree_path,
                reply,
            })
            .await?
            .map_err(Into::into)
        } else {
            clear_stored_session_id(&self.storage, handle).map(|()| 0)
        }
    }

    pub async fn start_fresh(&self, handle: &str, worktree_path: &Path) -> Result<(), String> {
        self.clear_context(handle, worktree_path).await.map(|_| ())
    }

    pub async fn attach_snapshot(
        &self,
        handle: &str,
        fallback_model: String,
        client_cursor: Option<usize>,
    ) -> AttachSnapshot {
        if let Ok(tx) = self.command_tx(handle) {
            let model = fallback_model.clone();
            if let Ok(snapshot) = send_command(&tx, |reply| TaskSessionCommand::AttachSnapshot {
                model,
                client_cursor,
                reply,
            })
            .await
            {
                return snapshot;
            }
        }
        let (stored, transcript_error) = load_detached(&self.storage, handle);
        let log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
        let (mut snapshot, replayed) = super::replay::build_attach(
            &log,
            fallback_model,
            false,
            client_cursor,
            SessionChrome::default(),
        );
        snapshot.transcript_error = transcript_error;
        AttachSnapshot {
            generation: 0,
            snapshot,
            replayed,
        }
    }

    pub async fn collect_outbound(
        &self,
        handle: &str,
        cursor: usize,
        generation: u64,
    ) -> OutboundBatch {
        if let Ok(tx) = self.command_tx(handle) {
            if let Ok(batch) = send_command(&tx, |reply| TaskSessionCommand::CollectOutbound {
                cursor,
                generation,
                reply,
            })
            .await
            {
                return batch;
            }
        }
        let (stored, _) = load_detached(&self.storage, handle);
        let log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
        let (events, next) = log.read_from_enveloped(cursor);
        OutboundBatch {
            generation: 0,
            cursor: next,
            snapshot: None,
            events,
        }
    }

    /// One page of older retained transcript events for a handle, read the
    /// same way the detached attach fallback loads history (load_detached +
    /// `TranscriptLog::from_events`), then sliced by the pure `replay::read_before`
    /// walker. Read-only: never spawns or touches a live session actor.
    pub(crate) fn retained_transcript_page(
        &self,
        handle: &str,
        before: Option<usize>,
        limit: Option<usize>,
    ) -> super::replay::TranscriptPage {
        let (stored, _) = load_detached(&self.storage, handle);
        let log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
        // A missing `before` walks from the newest retained event; a missing
        // `limit` uses the same tail bound as attach snapshots.
        let before = before.unwrap_or(usize::MAX);
        let limit = limit.unwrap_or(super::replay::TAIL_EVENT_LIMIT);
        super::replay::read_before(&log, before, limit)
    }

    #[cfg(test)]
    pub async fn read_from(&self, handle: &str, cursor: usize) -> (Vec<SessionServerEvent>, usize) {
        if let Ok(tx) = self.command_tx(handle) {
            if let Ok(result) =
                send_command(&tx, |reply| TaskSessionCommand::ReadFrom { cursor, reply }).await
            {
                return result;
            }
        }
        super::task_session_outbound::disk_read_from(&self.storage.state_dir, handle, cursor)
    }

    #[cfg(test)]
    pub async fn record(self: &Arc<Self>, handle: &str, event: SessionServerEvent) {
        if let Ok(tx) = self.ensure_entry(handle).await {
            let _ = send_command(&tx, |reply| TaskSessionCommand::Record { event, reply }).await;
        }
    }

    #[cfg(test)]
    pub async fn child_id(&self, handle: &str) -> Option<u32> {
        let tx = self.command_tx(handle).ok()?;
        send_command(&tx, |reply| TaskSessionCommand::ChildId { reply })
            .await
            .ok()
            .flatten()
    }

    #[cfg(test)]
    pub async fn kill_host_for_test(&self, handle: &str) {
        if let Ok(tx) = self.command_tx(handle) {
            let _ = send_command(&tx, |reply| TaskSessionCommand::KillHostForTest { reply }).await;
        }
    }

    #[cfg(test)]
    pub async fn generation(&self, handle: &str) -> u64 {
        self.attach_snapshot(handle, "auto".to_string(), None)
            .await
            .generation
    }

    #[cfg(test)]
    pub async fn pump(&self, handle: &str) {
        if let Ok(tx) = self.command_tx(handle) {
            let _ = tx.send(TaskSessionCommand::Pump).await;
        }
    }

    #[cfg(test)]
    pub async fn submit_prompt(&self, handle: &str, text: String) -> Result<(), String> {
        self.submit_prompt_with_id(handle, String::new(), text, Vec::new())
            .await
    }

    #[cfg(test)]
    pub async fn eviction_snapshot(&self, handle: &str) -> Option<EvictionSnapshot> {
        let tx = self.command_tx(handle).ok()?;
        eviction_snapshot(&tx).await.ok()
    }

    #[cfg(test)]
    pub fn is_marked_idle_release(&self, handle: &str) -> Option<bool> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(handle)
            .map(|entry| entry.last_released.is_some())
    }

    fn command_tx(&self, handle: &str) -> Result<TaskSessionSender, String> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(handle)
            .map(|entry| entry.command_tx.clone())
            .ok_or_else(|| "session slot missing".to_string())
    }

    pub(crate) fn has_live_entry(&self, handle: &str) -> bool {
        self.sessions.lock().unwrap().contains_key(handle)
    }
}

async fn eviction_snapshot(tx: &TaskSessionSender) -> Result<EvictionSnapshot, String> {
    send_command(tx, |reply| TaskSessionCommand::EvictionSnapshot { reply })
        .await
        .map_err(Into::into)
}

#[derive(Debug, PartialEq)]
pub(crate) enum ApplyClientMessageOutcome {
    Applied,
    ModelChanged { persist_warning: Option<String> },
}

pub(crate) async fn apply_client_message(
    directory: &TaskSessionDirectory,
    handle: &str,
    worktree_path: &Path,
    message: SessionClientMessage,
    generation: &mut u64,
    persist_session_model: Option<PersistSessionModel>,
) -> Result<ApplyClientMessageOutcome, String> {
    match message {
        SessionClientMessage::Prompt {
            text,
            content_blocks,
            client_message_id,
        } => {
            if client_message_id.trim().is_empty() {
                return Err("prompt clientMessageId is required".to_string());
            }
            directory
                .submit_prompt_with_id(handle, client_message_id, text, content_blocks)
                .await?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::Cancel { keep_queue } => {
            directory.cancel(handle, keep_queue).await?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::Clear => {
            *generation = directory.clear_context(handle, worktree_path).await?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::SetConfigOption { config_id, value } => {
            let config_id = config_id.trim().to_string();
            if config_id.is_empty() {
                return Err("configId is required".to_string());
            }
            if !directory.has_live_entry(handle) {
                return Err("session slot missing".to_string());
            }
            let outcome = directory
                .apply_config_option(handle, &config_id, value)
                .await?;
            *generation = outcome.generation;
            let persistence_warning = outcome
                .persist_model
                .as_deref()
                .and_then(|model| {
                    persist_session_model
                        .as_ref()
                        .and_then(|persist| persist(model).err())
                })
                .map(|warn| format!("Model changed but could not save to task — {warn}"));
            let persist_warning = outcome.persist_warning.or(persistence_warning);
            Ok(ApplyClientMessageOutcome::ModelChanged { persist_warning })
        }
        SessionClientMessage::Permission {
            request_id,
            approved,
            reason,
        } => {
            directory
                .answer_permission(handle, &request_id, approved, reason.as_deref())
                .await?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::Elicitation {
            request_id,
            action,
            content,
        } => {
            directory
                .answer_elicitation(handle, &request_id, &action, content)
                .await?;
            Ok(ApplyClientMessageOutcome::Applied)
        }
        SessionClientMessage::RetryRestore | SessionClientMessage::StartFresh => {
            Err("restore recovery is only available before session attach".to_string())
        }
    }
}

fn clear_stored_session_id(storage: &SessionStorage, handle: &str) -> Result<(), String> {
    web_session_store::clear_acp_session_id(
        &storage.registry_store(),
        &storage.state_dir,
        handle,
        "unknown",
    )
    .map_err(|error| format!("stored session id could not be cleared: {error}"))
}

/// Discard the chat's SQLite-queued prompts on an idle harness switch (#1324),
/// so headless startup dispatch never runs them under the new harness. Fails
/// closed: a ledger load or persist error is returned to the caller.
fn discard_queued_prompts(storage: &SessionStorage, handle: &str) -> Result<(), String> {
    let mut ledger = web_session_store::prompt_ledger::try_load(
        &storage.registry_store(),
        &storage.state_dir,
        handle,
    )
    .map_err(|error| format!("stored prompt ledger could not be loaded: {error}"))?;
    ledger.remove_queued();
    web_session_store::prompt_ledger::try_persist(
        &storage.registry_store(),
        &storage.state_dir,
        handle,
        &ledger,
    )
    .map_err(|error| format!("queued prompts could not be discarded: {error}"))
}

/// Stored transcript for a handle with no live slot, plus what to tell the
/// operator when it could not be read back whole.
fn load_detached(
    storage: &SessionStorage,
    handle: &str,
) -> (
    web_session_store::StoredSession<SessionServerEvent>,
    Option<String>,
) {
    match web_session_store::try_load::<SessionServerEvent>(
        &storage.registry_store(),
        &storage.state_dir,
        handle,
        "unknown",
    ) {
        Ok(stored) => {
            let warning = web_session_store::corruption_warning(stored.corrupt_lines);
            (stored, warning)
        }
        Err(error) => {
            tracing::warn!(%error, handle, "stored web session is unreadable");
            (
                Default::default(),
                Some(format!("stored session is unreadable: {error}")),
            )
        }
    }
}
