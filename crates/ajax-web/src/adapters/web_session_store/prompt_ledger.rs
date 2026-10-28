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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shim_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_cursor: Option<u64>,
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
            entry.shim_offset = None;
            entry.transcript_cursor = None;
            return;
        }
        self.entries.push(PromptLedgerEntry {
            client_message_id,
            phase: PromptPhase::Queued,
            transcript_text,
            prompt_text,
            content_blocks,
            shim_offset: None,
            transcript_cursor: None,
        });
    }

    pub fn record_dispatch_marks(
        &mut self,
        client_message_id: &str,
        shim_offset: Option<u64>,
        transcript_cursor: u64,
    ) {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.client_message_id == client_message_id)
        {
            entry.shim_offset = shim_offset;
            entry.transcript_cursor = Some(transcript_cursor);
        }
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
        entry.shim_offset = None;
        entry.transcript_cursor = None;
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

fn delete_legacy_file(state_dir: &Path, handle: &str) -> Result<(), LedgerLoadError> {
    match fs::remove_file(ledger_path(state_dir, handle)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(LedgerLoadError::Unreadable),
    }
}

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

pub fn try_persist_with_events<T: serde::Serialize>(
    store: &ajax_core::registry::SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
    events: &[T],
) -> Result<(), std::io::Error> {
    try_persist_with_events_and_rewrites(store, state_dir, handle, ledger, events, &[])
}

pub fn try_persist_with_events_and_rewrites<T: serde::Serialize>(
    store: &ajax_core::registry::SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    ledger: &PromptLedger,
    events: &[T],
    rewrites: &[(usize, T)],
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
    let event_rewrites = rewrites
        .iter()
        .map(|(cursor, event)| {
            serde_json::to_string(event).map(|payload| ajax_core::registry::EventRewrite {
                seq: *cursor as u64 + 1,
                payload,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(std::io::Error::other)?;
    match kernel::plan_commit(&run_id, &current, ledger).map_err(std::io::Error::other)? {
        Some(mut commit) => {
            commit.events = event_strings;
            commit.event_rewrites = event_rewrites;
            let receipt = store
                .commit_execution(&commit)
                .map_err(std::io::Error::other)?;
            if !receipt.duplicate_prompts.is_empty() {
                return Err(std::io::Error::other("prompt id already used"));
            }
        }
        None if !event_strings.is_empty() || !event_rewrites.is_empty() => {
            store
                .commit_execution(&ajax_core::registry::ExecutionCommit {
                    run_id: run_id.clone(),
                    events: event_strings,
                    event_rewrites,
                    ..Default::default()
                })
                .map_err(std::io::Error::other)?;
        }
        None => return Ok(()),
    }
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

#[cfg(test)]
mod dispatch_marks {
    use super::*;

    fn scratch_dir(label: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/prompt-ledger-dispatch-marks")
            .join(format!("{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn queued_ledger() -> PromptLedger {
        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "m1".to_string(),
            "hello".to_string(),
            "hello".to_string(),
            Vec::new(),
        );
        ledger
    }

    #[test]
    fn entry_round_trips_dispatch_marks() {
        let entry = PromptLedgerEntry {
            client_message_id: "m1".to_string(),
            phase: PromptPhase::Dispatching,
            transcript_text: String::new(),
            prompt_text: String::new(),
            content_blocks: Vec::new(),
            shim_offset: Some(4096),
            transcript_cursor: Some(12),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"shim_offset\":4096"));
        assert!(json.contains("\"transcript_cursor\":12"));
        let loaded: PromptLedgerEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, entry);
    }

    #[test]
    fn entry_without_marks_omits_them_and_old_json_loads() {
        let entry = queued_ledger().entries.remove(0);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(!json.contains("shim_offset"));
        assert!(!json.contains("transcript_cursor"));
        let old = r#"{"clientMessageId":"m1","phase":"queued","prompt_text":"p"}"#;
        let loaded: PromptLedgerEntry = serde_json::from_str(old).unwrap();
        assert_eq!(loaded.shim_offset, None);
        assert_eq!(loaded.transcript_cursor, None);
    }

    #[test]
    fn record_dispatch_marks_sets_only_the_matching_entry() {
        let mut ledger = queued_ledger();
        ledger.record_dispatch_marks("other", Some(1), 2);
        assert_eq!(ledger.entries[0].shim_offset, None);
        ledger.record_dispatch_marks("m1", None, 3);
        assert_eq!(ledger.entries[0].shim_offset, None);
        assert_eq!(ledger.entries[0].transcript_cursor, Some(3));
    }

    #[test]
    fn direct_dispatch_persists_marks_in_the_dispatching_update() {
        let dir = scratch_dir("persist-marks");
        let handle = "web/dispatch-marks";
        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "m1".to_string(),
            "hello".to_string(),
            "hello".to_string(),
            Vec::new(),
        );
        ledger.mark_dispatching("m1");
        ledger.record_dispatch_marks("m1", Some(4096), 12);
        persist(&dir, handle, &ledger).expect("persist dispatching");

        let loaded = load(&dir, handle).expect("load dispatching");
        let entry = loaded.entry("m1").expect("entry");
        assert_eq!(entry.phase, PromptPhase::Dispatching);
        assert_eq!(entry.shim_offset, Some(4096));
        assert_eq!(entry.transcript_cursor, Some(12));
    }

    #[test]
    fn queued_prompt_dispatch_rewrites_its_row_with_the_marks() {
        let dir = scratch_dir("queued-dispatch-marks");
        let handle = "web/queued-dispatch-marks";
        let mut ledger = queued_ledger();
        persist(&dir, handle, &ledger).expect("persist queued");

        ledger.mark_dispatching("m1");
        ledger.record_dispatch_marks("m1", Some(512), 9);
        persist(&dir, handle, &ledger).expect("persist dispatching");

        let loaded = load(&dir, handle).expect("load dispatching");
        let entry = loaded.entry("m1").expect("entry");
        assert_eq!(entry.phase, PromptPhase::Dispatching);
        assert_eq!(entry.shim_offset, Some(512));
        assert_eq!(entry.transcript_cursor, Some(9));
    }

    #[test]
    fn non_shim_direct_dispatch_persists_null_shim_offset_and_a_cursor() {
        let dir = scratch_dir("persist-non-shim");
        let handle = "web/dispatch-no-shim";
        let mut ledger = PromptLedger::default();
        ledger.upsert_queued(
            "m1".to_string(),
            "hello".to_string(),
            "hello".to_string(),
            Vec::new(),
        );
        ledger.mark_dispatching("m1");
        ledger.record_dispatch_marks("m1", None, 7);
        persist(&dir, handle, &ledger).expect("persist dispatching");

        let loaded = load(&dir, handle).expect("load dispatching");
        let entry = loaded.entry("m1").expect("entry");
        assert_eq!(entry.shim_offset, None);
        assert_eq!(entry.transcript_cursor, Some(7));
    }

    #[test]
    fn legacy_ledger_file_without_marks_still_loads() {
        let dir = scratch_dir("legacy-without-marks");
        let handle = "web/legacy";
        let path = ledger_path(&dir, handle);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            r#"{"kind":"prompt_ledger","v":1,"entries":[{"clientMessageId":"m1","phase":"queued","transcript_text":"t","prompt_text":"p"}]}"#,
        )
        .unwrap();
        let loaded = load(&dir, handle).expect("load legacy");
        let entry = loaded.entry("m1").expect("entry");
        assert_eq!(entry.phase, PromptPhase::Queued);
        assert_eq!(entry.shim_offset, None);
        assert_eq!(entry.transcript_cursor, None);
    }
}
