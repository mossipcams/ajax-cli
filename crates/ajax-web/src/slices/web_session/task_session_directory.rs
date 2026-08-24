//! Process-wide map of qualified handles to per-task session command loops.

use super::{
    normalize_session_model,
    protocol::SessionChrome,
    task_session::{
        send_command, spawn_task_session, AttachSnapshot, EvictionSnapshot, OutboundBatch,
        TaskSessionCommand, TaskSessionSender,
    },
    task_session_spawn,
    transcript::MAX_IDLE_SESSIONS,
    PersistSessionModel, SessionClientMessage, SessionServerEvent,
};
use crate::adapters::web_session_acp::wire_value_to_session_value;
use crate::adapters::web_session_store;
use ajax_core::models::AgentClient;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
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
    state_dir: PathBuf,
}

impl TaskSessionDirectory {
    pub fn new(state_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            state_dir,
        })
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
        let (tx, join) = spawn_task_session(handle.to_string(), self.state_dir.clone());
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
        let (tx, join) = spawn_task_session(handle.to_string(), self.state_dir.clone());
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
        let tx = Arc::clone(self)
            .ensure_entry_for_acquire(qualified_handle)
            .await?;
        let worktree_path = worktree_path.to_path_buf();
        let model = model.to_string();
        send_command(&tx, |reply| TaskSessionCommand::Acquire {
            worktree_path,
            model,
            agent,
            reply,
        })
        .await?
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
            let _ = entry.command_tx.send(TaskSessionCommand::Shutdown { close: true }).await;
            let _ = entry.join_handle.await;
        }
    }

    /// Tear down the live child without ACP `session/close` so resume/load can succeed.
    #[cfg(test)]
    pub async fn detach_session(&self, handle: &str) {
        let removed = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.remove(handle)
        };
        if let Some(entry) = removed {
            let _ = entry.command_tx.send(TaskSessionCommand::Shutdown { close: false }).await;
            let _ = entry.join_handle.await;
        }
    }

    /// Shut down any live slot and delete the persisted transcript for `handle`.
    pub async fn cleanup_session(&self, handle: &str) {
        self.drop_session(handle).await;
        web_session_store::delete_session(&self.state_dir, handle);
    }

    /// Delete persisted transcripts with no registry owner at process start.
    pub fn prune_stale_persisted(&self, owned: &std::collections::HashSet<String>) {
        super::session_cleanup::prune_stale_persisted_sessions(&self.state_dir, owned);
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
    }

    pub async fn cancel(&self, handle: &str, keep_queue: bool) -> Result<(), String> {
        let tx = self.command_tx(handle)?;
        send_command(&tx, |reply| TaskSessionCommand::Cancel {
            keep_queue,
            reply,
        })
        .await?
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
    }

    pub async fn apply_model(
        &self,
        handle: &str,
        worktree_path: &Path,
        model: &str,
    ) -> Result<u64, String> {
        let tx = match self.command_tx(handle) {
            Ok(tx) => tx,
            Err(_) => return Ok(0),
        };
        let worktree_path = worktree_path.to_path_buf();
        let model = model.to_string();
        send_command(&tx, |reply| TaskSessionCommand::ApplyModel {
            worktree_path,
            model,
            reply,
        })
        .await?
    }

    pub async fn apply_config_option(
        &self,
        handle: &str,
        config_id: &str,
        value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
    ) -> Result<task_session_spawn::ApplyConfigOptionResult, String> {
        let tx = self.command_tx(handle)?;
        let config_id = config_id.to_string();
        send_command(&tx, |reply| TaskSessionCommand::ApplyConfigOption {
            config_id,
            value,
            reply,
        })
        .await?
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
            web_session_store::clear_acp_session_id(&self.state_dir, handle);
            Ok(())
        }
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
        let stored = web_session_store::load::<SessionServerEvent>(&self.state_dir, handle);
        let log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
        let (snapshot, replayed) = super::replay::build_attach(
            &log,
            fallback_model,
            false,
            client_cursor,
            SessionChrome::default(),
        );
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
        let stored = web_session_store::load::<SessionServerEvent>(&self.state_dir, handle);
        let log = super::transcript::TranscriptLog::from_events(stored.events, stored.dropped);
        let (events, next) = log.read_from_enveloped(cursor);
        OutboundBatch {
            generation: 0,
            cursor: next,
            snapshot: None,
            events,
        }
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
        super::task_session::disk_read_from(&self.state_dir, handle, cursor)
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

    pub(crate) fn has_resumable_session(&self, handle: &str) -> bool {
        web_session_store::load::<SessionServerEvent>(&self.state_dir, handle)
            .acp_session_id
            .is_some()
    }
}

async fn eviction_snapshot(tx: &TaskSessionSender) -> Result<EvictionSnapshot, String> {
    send_command(tx, |reply| TaskSessionCommand::EvictionSnapshot { reply }).await
}

#[derive(Debug, PartialEq)]
pub(crate) enum ApplyClientMessageOutcome {
    Applied,
    ModelChanged {
        /// WS-only warning when live apply succeeded but task persist failed.
        persist_warning: Option<String>,
    },
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
        SessionClientMessage::SetModel { model } => {
            let model = normalize_session_model(&model)?;
            if let Some(persist) = persist_session_model {
                persist(&model)?;
            }
            if !directory.has_live_entry(handle) {
                return Err("session slot missing".to_string());
            }
            let next_generation = directory.apply_model(handle, worktree_path, &model).await?;
            *generation = next_generation;
            Ok(ApplyClientMessageOutcome::ModelChanged {
                persist_warning: None,
            })
        }
        SessionClientMessage::SetConfigOption { config_id, value } => {
            let config_id = config_id.trim().to_string();
            if config_id.is_empty() {
                return Err("configId is required".to_string());
            }
            let wire = wire_value_to_session_value(value);
            if !directory.has_live_entry(handle) {
                return Err("session slot missing".to_string());
            }
            let outcome = directory
                .apply_config_option(handle, &config_id, wire)
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
    }
}
