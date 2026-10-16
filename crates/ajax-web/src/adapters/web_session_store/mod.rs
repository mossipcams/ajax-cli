//! Chat transcript persistence through the core execution kernel.
//! Browser cursor n maps to execution sequence n + 1; the prompt ledger stays JSON.
use ajax_core::registry::{ExecutionCommit, NewRun, RunRecord, RunUpdate, SqliteRegistryStore};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub const MAX_LOG_EVENTS: usize = 2000;
pub(crate) const WEB_SESSION_DIR: &str = "web-session";
mod legacy;
pub mod prompt_ledger;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredSession<T> {
    pub harness: String,
    pub acp_session_id: Option<String>,
    pub model: String,
    pub events: Vec<T>,
    pub dropped: usize,
    /// Interior rows that could not be read back; the history is incomplete.
    pub corrupt_lines: usize,
}

impl<T> Default for StoredSession<T> {
    fn default() -> Self {
        Self {
            harness: "unknown".to_string(),
            acp_session_id: None,
            model: "auto".to_string(),
            events: Vec::new(),
            dropped: 0,
            corrupt_lines: 0,
        }
    }
}

fn new_run(handle: &str, harness: &str, model: &str) -> NewRun {
    NewRun {
        // Deterministic identity also prevents competing first writers creating two runs.
        run_id: format!("web-chat:{handle}"),
        task_id: handle.to_owned(),
        harness: harness.to_owned(),
        model: model.to_owned(),
        created_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(i64::MAX as u128) as i64,
    }
}

// ponytail: opens the registry per call; cache the run id per session if append latency shows up
pub(super) fn load_run(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
) -> io::Result<Option<RunRecord>> {
    let mut run = store
        .latest_run_for_task(handle)
        .map_err(io::Error::other)?;
    let path = session_path(state_dir, handle);
    if path.try_exists()? {
        if run.is_none() {
            let legacy = legacy::read::<serde_json::Value>(state_dir, handle)?;
            let events = legacy
                .events
                .iter()
                .map(serde_json::to_string)
                .collect::<Result<Vec<_>, _>>()
                .map_err(io::Error::other)?;
            let first_seq = u64::try_from(legacy.dropped)
                .ok()
                .and_then(|n| n.checked_add(1))
                .ok_or_else(|| io::Error::other("transcript cursor overflow"))?;
            store
                .create_run_with_history(
                    &new_run(handle, harness, &legacy.model),
                    legacy.acp_session_id.as_deref(),
                    first_seq,
                    &events,
                    legacy.corrupt_lines as u64,
                )
                .map_err(io::Error::other)?;
            if let Some(warning) = corruption_warning(legacy.corrupt_lines) {
                tracing::warn!(handle, %warning, "imported incomplete web session transcript");
            }
            run = store
                .latest_run_for_task(handle)
                .map_err(io::Error::other)?;
        }
        // Never overwrite an earlier preserved source; number the destination until one is free.
        let mut suffix = 0u32;
        loop {
            let name = if suffix == 0 {
                "jsonl.migrated".to_string()
            } else {
                format!("jsonl.migrated.{suffix}")
            };
            let migrated = path.with_extension(&name);
            if !migrated.try_exists()? {
                fs::rename(&path, &migrated)?;
                // The preserved transcript may contain prompt text; lock it
                // down to the owner only.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(error) =
                        fs::set_permissions(&migrated, fs::Permissions::from_mode(0o600))
                    {
                        tracing::warn!(
                            path = %migrated.display(),
                            ?error,
                            "failed to tighten migrated transcript permissions"
                        );
                    }
                }
                break;
            }
            suffix += 1;
        }
    }
    Ok(run)
}

pub fn try_load<T: DeserializeOwned + Serialize>(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
) -> io::Result<StoredSession<T>> {
    let Some(run) = load_run(store, state_dir, handle, harness)? else {
        return Ok(StoredSession::default());
    };
    let rows = store
        .execution_events_since(&run.run_id, 0)
        .map_err(io::Error::other)?;
    let dropped = rows.first().map_or(run.next_event_seq, |(seq, _)| *seq) - 1;
    let mut events = Vec::with_capacity(rows.len());
    let mut corrupt_lines = 0usize;
    for (_, payload) in rows {
        match serde_json::from_str::<T>(&payload) {
            Ok(event) => events.push(event),
            // One unreadable row must not hide the rest of the transcript.
            Err(_) => corrupt_lines += 1,
        }
    }
    Ok(StoredSession {
        harness: run.harness,
        acp_session_id: run.native_session_id,
        model: run.model,
        dropped: usize::try_from(dropped).map_err(io::Error::other)?,
        events,
        // Rows lost before or during the legacy import stay counted across reloads.
        corrupt_lines: run.unreadable_history_rows as usize + corrupt_lines,
    })
}

pub(super) fn ensure_run(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
) -> io::Result<String> {
    if let Some(run) = load_run(store, state_dir, handle, harness)? {
        return Ok(run.run_id);
    }
    let run = new_run(handle, harness, "auto");
    store.create_run(&run).map_err(io::Error::other)?;
    Ok(run.run_id)
}

/// Records the live agent child's identity (pid, start-time token) and the
/// owning host's boot id on the task's run row so a later host can find and
/// clean up orphans. Nothing is killed here; `pgid` is stored equal to `pid`
/// because the sweep does not use it. Passing `None` clears the child fields
/// but keeps the owner boot id.
pub fn record_child_identity(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
    child: Option<(i64, String)>,
    owner_boot_id: &str,
) -> io::Result<()> {
    let run_id = ensure_run(store, state_dir, handle, harness)?;
    store
        .commit_execution(&ExecutionCommit {
            run_id,
            run_update: Some(RunUpdate {
                child: Some(child.as_ref().map(|(pid, _)| (*pid, *pid))),
                child_started: Some(child.map(|(_, token)| token)),
                owner_boot_id: Some(Some(owner_boot_id.to_owned())),
                ..Default::default()
            }),
            ..Default::default()
        })
        .map(|_| ())
        .map_err(io::Error::other)
}

pub fn try_save_meta(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
    acp_session_id: Option<&str>,
    model: &str,
) -> io::Result<()> {
    #[cfg(test)]
    if force_meta_write_fail() {
        return Err(io::Error::other("forced session metadata write failure"));
    }
    let run_id = ensure_run(store, state_dir, handle, harness)?;
    store
        .commit_execution(&ExecutionCommit {
            run_id,
            run_update: Some(RunUpdate {
                harness: Some(harness.to_owned()),
                native_session_id: Some(acp_session_id.map(str::to_owned)),
                model: Some(model.to_owned()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .map_err(io::Error::other)?;
    Ok(())
}

pub fn clear_acp_session_id(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
) -> io::Result<()> {
    #[cfg(test)]
    if force_meta_write_fail() {
        return Err(io::Error::other("forced session metadata write failure"));
    }
    let Some(run) = load_run(store, state_dir, handle, harness)? else {
        return Ok(());
    };
    store
        .commit_execution(&ExecutionCommit {
            run_id: run.run_id,
            run_update: Some(RunUpdate {
                native_session_id: Some(None),
                ..Default::default()
            }),
            ..Default::default()
        })
        .map_err(io::Error::other)?;
    Ok(())
}

pub fn try_append_events<T: Serialize + DeserializeOwned>(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    harness: &str,
    new_events: &[T],
) -> io::Result<()> {
    if new_events.is_empty() {
        return Ok(());
    }
    let events = new_events
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<_, _>>()
        .map_err(io::Error::other)?;
    let run_id = ensure_run(store, state_dir, handle, harness)?;
    store
        .commit_execution(&ExecutionCommit {
            run_id: run_id.clone(),
            events,
            ..Default::default()
        })
        .map_err(io::Error::other)?;
    // A retention failure must not make a committed append look unsuccessful.
    if let Err(error) = store.prune_execution_events(&run_id, MAX_LOG_EVENTS as u64) {
        tracing::warn!(%error, handle, "failed to prune web session transcript");
    }
    Ok(())
}

/// Operator-facing warning when stored rows could not be read back.
pub fn corruption_warning(corrupt_lines: usize) -> Option<String> {
    (corrupt_lines > 0).then(|| {
        format!(
            "{corrupt_lines} stored transcript rows were unreadable; this history is incomplete"
        )
    })
}

pub(crate) fn encode_handle(handle: &str) -> String {
    handle.replace('%', "%25").replace('/', "%2F")
}

fn decode_handle(encoded: &str) -> String {
    encoded.replace("%2F", "/").replace("%25", "%")
}

pub fn list_persisted_handles(store: &SqliteRegistryStore, state_dir: &Path) -> Vec<String> {
    let mut handles = match store.list_run_task_ids() {
        Ok(handles) => handles,
        Err(error) => {
            tracing::warn!(%error, "failed to list web session runs");
            Vec::new()
        }
    };
    if let Ok(entries) = fs::read_dir(state_dir.join(WEB_SESSION_DIR)) {
        handles.extend(entries.filter_map(Result::ok).filter_map(|entry| {
            let name = entry.file_name();
            transcript_entry_handle(&name.to_string_lossy())
        }));
    }
    handles.sort();
    handles.dedup();
    handles
}

/// A transcript directory entry names exactly one persisted handle: `<x>.jsonl`,
/// `<x>.jsonl.migrated`, or a numbered migration leftover `<x>.jsonl.migrated.<digits>`.
fn transcript_entry_handle(name: &str) -> Option<String> {
    if let Some(stem) = name.strip_suffix(".jsonl") {
        return Some(decode_handle(stem));
    }
    if let Some((stem, digits)) = name.rsplit_once(".migrated.") {
        if stem.ends_with(".jsonl")
            && !digits.is_empty()
            && digits.bytes().all(|b| b.is_ascii_digit())
        {
            return Some(decode_handle(&stem[..stem.len() - ".jsonl".len()]));
        }
        // A `.migrated.` tail that is not numeric is not a transcript entry.
        return None;
    }
    name.strip_suffix(".jsonl.migrated").map(decode_handle)
}

pub fn delete_session(store: &SqliteRegistryStore, state_dir: &Path, handle: &str) -> bool {
    let mut deleted = false;
    // Every step is attempted even when an earlier one fails; a failed database
    // delete makes the whole session not fully deleted so a later prune retries.
    let database_failed = match store.delete_runs_for_task(handle) {
        Ok(count) => {
            deleted = count > 0;
            false
        }
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete web session runs");
            true
        }
    };

    // Remove every stored copy of the transcript: the live file and any numbered migration leftovers.
    let encoded = encode_handle(handle);
    let path = session_path(state_dir, handle);
    match fs::remove_file(&path) {
        Ok(()) => deleted = true,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, handle, "failed to delete legacy web session transcript")
        }
    }
    let migrated_base = format!("{encoded}.jsonl.migrated");
    let numbered_prefix = format!("{migrated_base}.");
    if let Ok(entries) = fs::read_dir(state_dir.join(WEB_SESSION_DIR)) {
        for entry in entries.filter_map(Result::ok) {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            // Exact base name only: never touches a longer handle that shares this prefix.
            if name != migrated_base && !name.starts_with(numbered_prefix.as_str()) {
                continue;
            }
            match fs::remove_file(entry.path()) {
                Ok(()) => deleted = true,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    tracing::warn!(%error, handle, "failed to delete legacy web session transcript")
                }
            }
        }
    }

    let ledger_deleted = prompt_ledger::delete_ledger(state_dir, handle);
    if database_failed {
        false
    } else {
        deleted || ledger_deleted
    }
}

pub fn session_path(state_dir: &Path, handle: &str) -> PathBuf {
    state_dir
        .join(WEB_SESSION_DIR)
        .join(format!("{}.jsonl", encode_handle(handle)))
}

#[cfg(test)]
pub fn load<T: DeserializeOwned + Serialize>(state_dir: &Path, handle: &str) -> StoredSession<T> {
    try_load(
        &SqliteRegistryStore::new(state_dir.join("ajax.db")),
        state_dir,
        handle,
        "unknown",
    )
    .unwrap_or_default()
}

#[cfg(test)]
pub fn save_meta(state_dir: &Path, handle: &str, id: Option<&str>, model: &str) {
    try_save_meta(
        &SqliteRegistryStore::new(state_dir.join("ajax.db")),
        state_dir,
        handle,
        "unknown",
        id,
        model,
    )
    .unwrap();
}

#[cfg(test)]
pub fn append_events<T: Serialize + DeserializeOwned>(
    state_dir: &Path,
    handle: &str,
    events: &[T],
) {
    try_append_events(
        &SqliteRegistryStore::new(state_dir.join("ajax.db")),
        state_dir,
        handle,
        "unknown",
        events,
    )
    .unwrap();
}

#[cfg(test)]
mod tests;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static FORCE_META_WRITE_FAIL: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
pub fn set_force_meta_write_fail(enabled: bool) {
    FORCE_META_WRITE_FAIL.store(enabled, Ordering::SeqCst);
}

#[cfg(test)]
fn force_meta_write_fail() -> bool {
    FORCE_META_WRITE_FAIL.load(Ordering::SeqCst)
}
