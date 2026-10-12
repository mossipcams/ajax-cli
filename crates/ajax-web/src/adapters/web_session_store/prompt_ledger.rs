use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod kernel;

pub const LEDGER_VERSION: u32 = 1;
const LEDGER_KIND: &str = "prompt_ledger";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerLoadError {
    Unreadable,
    Malformed,
    UnsupportedVersion { found: u32 },
    WrongKind { found: String },
}

impl std::fmt::Display for LedgerLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable => write!(f, "prompt ledger is unreadable"),
            Self::Malformed => write!(f, "prompt ledger is malformed"),
            Self::UnsupportedVersion { found } => {
                write!(
                    f,
                    "prompt ledger version {found} is newer than supported version {LEDGER_VERSION}"
                )
            }
            Self::WrongKind { found } => {
                write!(f, "prompt ledger kind {found:?} is invalid")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptPhase {
    Queued,
    Dispatching,
    Interrupted,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptLedgerEntry {
    #[serde(rename = "clientMessageId")]
    pub client_message_id: String,
    pub phase: PromptPhase,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transcript_text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_blocks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptLedger {
    #[serde(default)]
    pub entries: Vec<PromptLedgerEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiskLedger {
    kind: String,
    v: u32,
    #[serde(default)]
    entries: Vec<PromptLedgerEntry>,
}

impl PromptLedger {
    pub fn entry(&self, client_message_id: &str) -> Option<&PromptLedgerEntry> {
        self.entries
            .iter()
            .find(|entry| entry.client_message_id == client_message_id)
    }

    pub fn owns_prompt(&self, client_message_id: &str) -> bool {
        !client_message_id.is_empty() && self.entry(client_message_id).is_some()
    }

    pub fn queued_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.phase == PromptPhase::Queued)
            .count()
    }

    pub fn upsert_queued(
        &mut self,
        client_message_id: String,
        transcript_text: String,
        prompt_text: String,
        content_blocks: Vec<serde_json::Value>,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.client_message_id == client_message_id)
        {
            entry.phase = PromptPhase::Queued;
            entry.transcript_text = transcript_text;
            entry.prompt_text = prompt_text;
            entry.content_blocks = content_blocks;
            return;
        }
        self.entries.push(PromptLedgerEntry {
            client_message_id,
            phase: PromptPhase::Queued,
            transcript_text,
            prompt_text,
            content_blocks,
        });
    }

    pub fn mark_dispatching(&mut self, client_message_id: &str) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.client_message_id == client_message_id)
        else {
            return false;
        };
        entry.phase = PromptPhase::Dispatching;
        true
    }

    pub fn mark_completed(&mut self, client_message_id: &str) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.client_message_id == client_message_id)
        else {
            return false;
        };
        entry.phase = PromptPhase::Completed;
        entry.transcript_text.clear();
        entry.prompt_text.clear();
        entry.content_blocks.clear();
        true
    }

    pub fn mark_interrupted(&mut self, client_message_id: &str) -> bool {
        let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.client_message_id == client_message_id)
        else {
            return false;
        };
        entry.phase = PromptPhase::Interrupted;
        true
    }

    pub fn remove_queued(&mut self) {
        self.entries
            .retain(|entry| entry.phase != PromptPhase::Queued);
    }

    pub fn remove_entry(&mut self, client_message_id: &str) {
        self.entries
            .retain(|entry| entry.client_message_id != client_message_id);
    }

    pub fn recover_after_restart(&mut self) -> (Vec<PromptLedgerEntry>, Vec<String>) {
        let mut interrupted = Vec::new();
        for entry in &mut self.entries {
            if entry.phase == PromptPhase::Dispatching {
                entry.phase = PromptPhase::Interrupted;
                interrupted.push(entry.client_message_id.clone());
            }
        }
        let queued = self
            .entries
            .iter()
            .filter(|entry| entry.phase == PromptPhase::Queued)
            .cloned()
            .collect();
        (queued, interrupted)
    }
}

fn read_legacy(state_dir: &Path, handle: &str) -> Result<Option<PromptLedger>, LedgerLoadError> {
    let path = ledger_path(state_dir, handle);
    if !path.is_file() {
        return Ok(None);
    }
    let contents = fs::read_to_string(&path).map_err(|_| LedgerLoadError::Unreadable)?;
    let disk: DiskLedger =
        serde_json::from_str(&contents).map_err(|_| LedgerLoadError::Malformed)?;
    if disk.kind != LEDGER_KIND {
        return Err(LedgerLoadError::WrongKind { found: disk.kind });
    }
    if disk.v > LEDGER_VERSION {
        return Err(LedgerLoadError::UnsupportedVersion { found: disk.v });
    }
    if disk.v != LEDGER_VERSION {
        return Err(LedgerLoadError::Malformed);
    }
    Ok(Some(PromptLedger {
        entries: disk.entries,
    }))
}

pub fn try_load(
    store: &ajax_core::registry::SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
) -> Result<PromptLedger, LedgerLoadError> {
    let legacy = read_legacy(state_dir, handle)?;
    if let Some(legacy) = legacy {
        let run_id = super::ensure_run(store, state_dir, handle, "unknown")
            .map_err(|_| LedgerLoadError::Unreadable)?;
        let current = store
            .prompts_for_run(&run_id)
            .map_err(|_| LedgerLoadError::Unreadable)?;
        if current.is_empty() {
            // The ledger content is now durable in the database: delete the
            // legacy file instead of preserving it.
            let commit = kernel::plan_commit(&run_id, &[], &legacy)
                .map_err(|_| LedgerLoadError::Unreadable)?;
            if legacy.entries.is_empty() || commit.is_some() {
                if let Some(commit) = commit {
                    store
                        .commit_execution(&commit)
                        .map_err(|_| LedgerLoadError::Unreadable)?;
                }
                delete_legacy_file(state_dir, handle)?;
            } else {
                migrate_legacy_file(state_dir, handle)?;
            }
        } else {
            // The database already has prompts for the run; the sidecar was not
            // imported, so preserve it under a `.migrated` name.
            migrate_legacy_file(state_dir, handle)?;
        }
    }
    let run = super::load_run(store, state_dir, handle, "unknown")
        .map_err(|_| LedgerLoadError::Unreadable)?;
    match run {
        None => Ok(PromptLedger::default()),
        Some(run) => store
            .prompts_for_run(&run.run_id)
            .map_err(|_| LedgerLoadError::Unreadable)
            .map(|prompts| kernel::ledger_from_prompts(&prompts)),
    }
}

/// Delete a legacy ledger file whose content was imported into the database in
/// this call. `NotFound` is fine; any other failure is `Unreadable`.
fn delete_legacy_file(state_dir: &Path, handle: &str) -> Result<(), LedgerLoadError> {
    match fs::remove_file(ledger_path(state_dir, handle)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LedgerLoadError::Unreadable),
    }
}

/// Rename the legacy ledger file to the first free `.migrated` name; never
/// overwrite. Used only when an import was skipped because the database already
/// has prompts for the run and the sidecar content was not trusted into it.
fn migrate_legacy_file(state_dir: &Path, handle: &str) -> Result<(), LedgerLoadError> {
    let path = ledger_path(state_dir, handle);
    if !path.try_exists().map_err(|_| LedgerLoadError::Unreadable)? {
        return Ok(());
    }
    let base = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(LedgerLoadError::Unreadable)?;
    let dir = path.parent().ok_or(LedgerLoadError::Unreadable)?;
    let mut suffix = 0u32;
    loop {
        let name = if suffix == 0 {
            format!("{base}.migrated")
        } else {
            format!("{base}.migrated.{suffix}")
        };
        let migrated = dir.join(name);
        if !migrated
            .try_exists()
            .map_err(|_| LedgerLoadError::Unreadable)?
        {
            fs::rename(&path, &migrated).map_err(|_| LedgerLoadError::Unreadable)?;
            return Ok(());
        }
        suffix += 1;
    }
}

pub fn try_persist(
    store: &ajax_core::registry::SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
) -> Result<(), std::io::Error> {
    try_persist_with_events::<serde_json::Value>(store, state_dir, handle, ledger, &[])
}

/// Persist the prompt ledger and transcript events in one database transaction.
pub fn try_persist_with_events<T: serde::Serialize>(
    store: &ajax_core::registry::SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
    events: &[T],
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if force_persist_fail() {
        return Err(std::io::Error::other(
            "forced prompt ledger persist failure",
        ));
    }

    let run_id = super::ensure_run(store, state_dir, handle, "unknown")?;
    let current = store
        .prompts_for_run(&run_id)
        .map_err(std::io::Error::other)?;
    let event_strings = events
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(std::io::Error::other)?;
    match kernel::plan_commit(&run_id, &current, ledger).map_err(std::io::Error::other)? {
        Some(mut commit) => {
            commit.events = event_strings;
            let receipt = store
                .commit_execution(&commit)
                .map_err(std::io::Error::other)?;
            if !receipt.duplicate_prompts.is_empty() {
                return Err(std::io::Error::other("prompt id already used"));
            }
        }
        None if !event_strings.is_empty() => {
            store
                .commit_execution(&ajax_core::registry::ExecutionCommit {
                    run_id: run_id.clone(),
                    events: event_strings,
                    ..Default::default()
                })
                .map_err(std::io::Error::other)?;
        }
        None => return Ok(()),
    }
    // A retention failure must not make a committed append look unsuccessful.
    if let Err(error) = store.prune_execution_events(&run_id, super::MAX_LOG_EVENTS as u64) {
        tracing::warn!(%error, handle, "failed to prune web session transcript");
    }
    Ok(())
}

#[cfg(test)]
pub fn load(state_dir: &Path, handle: &str) -> Result<PromptLedger, LedgerLoadError> {
    let store = ajax_core::registry::SqliteRegistryStore::new(state_dir.join("ajax.db"));
    try_load(&store, state_dir, handle)
}

#[cfg(test)]
pub fn persist(
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
) -> Result<(), std::io::Error> {
    let store = ajax_core::registry::SqliteRegistryStore::new(state_dir.join("ajax.db"));
    try_persist(&store, state_dir, handle, ledger)
}

pub fn delete_ledger(state_dir: &Path, handle: &str) -> bool {
    let path = ledger_path(state_dir, handle);
    let mut removed = false;
    match fs::remove_file(&path) {
        Ok(()) => removed = true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete web session prompt ledger");
            return false;
        }
    }
    let base = match path.file_name().and_then(|name| name.to_str()) {
        Some(base) => base.to_owned(),
        None => return removed,
    };
    let Some(dir) = path.parent() else {
        return removed;
    };
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(name) => name,
                Err(_) => continue,
            };
            let is_migration = name == format!("{base}.migrated")
                || name.starts_with(&format!("{base}.migrated."));
            if is_migration && fs::remove_file(entry.path()).is_ok() {
                removed = true;
            }
        }
    }
    removed
}

pub fn ledger_path(state_dir: &Path, handle: &str) -> PathBuf {
    state_dir.join(super::WEB_SESSION_DIR).join(format!(
        "{}.prompt-ledger.json",
        super::encode_handle(handle)
    ))
}

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static FORCE_PERSIST_FAIL: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub struct ForcePersistFailGuard {
    previous: bool,
}

#[cfg(test)]
impl ForcePersistFailGuard {
    pub fn enable() -> Self {
        let previous = FORCE_PERSIST_FAIL.swap(true, Ordering::SeqCst);
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for ForcePersistFailGuard {
    fn drop(&mut self) {
        FORCE_PERSIST_FAIL.store(self.previous, Ordering::SeqCst);
    }
}

#[cfg(test)]
pub fn set_force_persist_fail(enabled: bool) {
    FORCE_PERSIST_FAIL.store(enabled, Ordering::SeqCst);
}

#[cfg(test)]
fn force_persist_fail() -> bool {
    FORCE_PERSIST_FAIL.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests;
