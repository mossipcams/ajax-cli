//! Versioned sidecar prompt ownership ledger (separate from transcript JSONL).

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

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

    /// On restart, promote orphaned dispatching rows to interrupted and return
    /// queued rows in FIFO order.
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

pub fn load(state_dir: &Path, handle: &str) -> Result<PromptLedger, LedgerLoadError> {
    let path = ledger_path(state_dir, handle);
    if !path.is_file() {
        return Ok(PromptLedger::default());
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
    Ok(PromptLedger {
        entries: disk.entries,
    })
}

pub fn persist(
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
) -> Result<(), std::io::Error> {
    #[cfg(test)]
    if force_persist_fail() {
        return Err(std::io::Error::other(
            "forced prompt ledger persist failure",
        ));
    }

    let dir = state_dir.join(super::WEB_SESSION_DIR);
    fs::create_dir_all(&dir)?;
    let path = ledger_path(state_dir, handle);
    let tmp_path = path.with_extension("prompt-ledger.tmp");
    let disk = DiskLedger {
        kind: LEDGER_KIND.to_string(),
        v: LEDGER_VERSION,
        entries: ledger.entries.clone(),
    };
    let body = serde_json::to_string_pretty(&disk).map_err(std::io::Error::other)?;
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp_path, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(&tmp_path, &path)?;
    sync_parent_dir(&path)?;
    Ok(())
}

fn sync_parent_dir(path: &Path) -> Result<(), std::io::Error> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let dir = fs::File::open(parent)?;
    dir.sync_all()
}

pub fn delete_ledger(state_dir: &Path, handle: &str) -> bool {
    let path = ledger_path(state_dir, handle);
    match fs::remove_file(&path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete web session prompt ledger");
            false
        }
    }
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

/// Test-scoped persist failure injection; restores the prior flag on drop.
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
