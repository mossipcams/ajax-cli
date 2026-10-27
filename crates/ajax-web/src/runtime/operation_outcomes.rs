use super::state::OperationCoordinator;
use crate::adapters::http::Response;
use crate::util::write_private_file;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const OUTCOMES_FILE: &str = "operation-outcomes.json";
pub(crate) const MAX_OUTCOME_AGE_SECS: u64 = 24 * 60 * 60;
const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

#[derive(Serialize, Deserialize)]
struct StoredOutcomes {
    entries: Vec<StoredOutcome>,
}

#[derive(Serialize, Deserialize)]
struct StoredOutcome {
    request_id: String,
    completed_at: u64,
    status_code: u16,
    content_type: String,
    body: String,
}

pub(crate) fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl OperationCoordinator {
    /// Loads completed request_id outcomes persisted under `state_dir`. A
    /// missing file starts empty; an unreadable or corrupt one is reported
    /// once and also starts empty, so startup never fails on it.
    pub(crate) fn load_persistent(state_dir: &Path) -> Self {
        let path = state_dir.join(OUTCOMES_FILE);
        let mut coordinator = Self {
            store_path: Some(path.clone()),
            ..Self::default()
        };
        let raw = match fs::read(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == ErrorKind::NotFound => return coordinator,
            Err(error) => {
                eprintln!("could not read {OUTCOMES_FILE}: {error}; no outcomes replayed");
                return coordinator;
            }
        };
        let stored = match serde_json::from_slice::<StoredOutcomes>(&raw) {
            Ok(stored) => stored,
            Err(error) => {
                eprintln!("invalid {OUTCOMES_FILE} ({error}); no outcomes replayed");
                return coordinator;
            }
        };
        let now = now_unix_secs();
        for entry in stored.entries {
            if now.saturating_sub(entry.completed_at) > MAX_OUTCOME_AGE_SECS {
                continue;
            }
            if entry.content_type != JSON_CONTENT_TYPE {
                continue;
            }
            let Ok(body) = STANDARD.decode(&entry.body) else {
                continue;
            };
            coordinator.remember_completed_response(
                entry.request_id,
                Response {
                    status_code: entry.status_code,
                    content_type: JSON_CONTENT_TYPE,
                    body,
                },
                entry.completed_at,
            );
        }
        coordinator
    }

    /// Captures every completed outcome as the bytes to write to the state
    /// file. The caller persists them after releasing the operations lock.
    pub(crate) fn pending_write(&self) -> Option<PendingOutcomeWrite> {
        let path = self.store_path.clone()?;
        let entries = self
            .completed_request_ids
            .iter()
            .filter_map(|request_id| {
                let response = self.completed.get(request_id)?;
                Some(StoredOutcome {
                    request_id: request_id.clone(),
                    completed_at: *self.completed_at.get(request_id)?,
                    status_code: response.status_code,
                    content_type: response.content_type.to_string(),
                    body: STANDARD.encode(&response.body),
                })
            })
            .collect();
        match serde_json::to_vec(&StoredOutcomes { entries }) {
            Ok(bytes) => Some(PendingOutcomeWrite { path, bytes }),
            Err(error) => {
                eprintln!("could not serialize {OUTCOMES_FILE}: {error}");
                None
            }
        }
    }
}

/// Outcome bytes captured under the operations lock, written once it is released.
pub(crate) struct PendingOutcomeWrite {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl PendingOutcomeWrite {
    /// A failed write is reported and leaves the in-memory outcomes in place.
    pub(crate) fn persist(self) {
        if let Err(error) = write_private_file(&self.path, &self.bytes) {
            eprintln!("could not write {OUTCOMES_FILE}: {error}");
        }
    }
}
