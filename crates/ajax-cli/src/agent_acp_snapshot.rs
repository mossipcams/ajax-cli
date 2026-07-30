use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

use ajax_core::{
    acp_status::{AcpSessionState, AcpStatusObservation, ObservedAcpStatus},
    models::TaskId,
};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};

use crate::CliError;

pub(crate) const SCHEMA_VERSION: u8 = 1;
pub(crate) const HEARTBEAT_INTERVAL_MILLIS: u128 = 5_000;
pub(crate) const SNAPSHOT_STALE_AFTER_MILLIS: u128 = 15_000;

static TEMP_SUFFIX: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct AcpRuntimeSnapshot {
    pub(crate) schema_version: u8,
    pub(crate) task_id: String,
    pub(crate) generation: String,
    pub(crate) session_id: Option<String>,
    pub(crate) heartbeat_unix_millis: u128,
    pub(crate) observation: AcpStatusObservation,
}

pub(crate) struct AcpSnapshotPublisher {
    state_root: PathBuf,
    last_published: AcpRuntimeSnapshot,
}

impl AcpSnapshotPublisher {
    pub(crate) fn claim(
        state_root: &Path,
        task_id: &str,
        generation: &str,
        session_id: Option<String>,
        observation: AcpStatusObservation,
        now_millis: u128,
    ) -> Result<Self, CliError> {
        let snapshot = AcpRuntimeSnapshot {
            schema_version: SCHEMA_VERSION,
            task_id: task_id.to_owned(),
            generation: generation.to_owned(),
            session_id,
            heartbeat_unix_millis: now_millis,
            observation,
        };
        let _lock = task_generation_lock(state_root, task_id)?;
        write_snapshot(state_root, &snapshot)?;
        Ok(Self {
            state_root: state_root.to_path_buf(),
            last_published: snapshot,
        })
    }

    pub(crate) fn publish(
        &mut self,
        session_id: Option<String>,
        observation: AcpStatusObservation,
        now_millis: u128,
    ) -> Result<bool, CliError> {
        let changed = self.last_published.session_id != session_id
            || self.last_published.observation != observation;
        let heartbeat_due = now_millis.saturating_sub(self.last_published.heartbeat_unix_millis)
            >= HEARTBEAT_INTERVAL_MILLIS;
        if !changed && !heartbeat_due {
            return Ok(false);
        }

        let _lock = task_generation_lock(&self.state_root, &self.last_published.task_id)?;
        self.ensure_current_generation()?;
        let snapshot = AcpRuntimeSnapshot {
            schema_version: SCHEMA_VERSION,
            task_id: self.last_published.task_id.clone(),
            generation: self.last_published.generation.clone(),
            session_id,
            heartbeat_unix_millis: now_millis,
            observation,
        };
        write_snapshot(&self.state_root, &snapshot)?;
        self.last_published = snapshot;
        Ok(true)
    }

    fn ensure_current_generation(&self) -> Result<(), CliError> {
        let bytes = match fs::read(snapshot_path(
            &self.state_root,
            &self.last_published.task_id,
        )) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(CliError::CommandFailed(format!(
                    "failed to verify ACP snapshot generation: {error}"
                )));
            }
        };
        let current: AcpRuntimeSnapshot = serde_json::from_slice(&bytes).map_err(|error| {
            CliError::CommandFailed(format!(
                "cannot publish ACP snapshot because the current file is malformed: {error}"
            ))
        })?;
        if current.generation != self.last_published.generation {
            return Err(CliError::CommandFailed(format!(
                "ACP snapshot generation mismatch for task {}; claim a new publisher before writing",
                self.last_published.task_id
            )));
        }
        Ok(())
    }
}

fn task_generation_lock(
    state_root: &Path,
    task_id: &str,
) -> Result<Flock<std::fs::File>, CliError> {
    fs::create_dir_all(state_root).map_err(|error| {
        CliError::CommandFailed(format!("failed to create ACP snapshot directory: {error}"))
    })?;
    let lock_path = snapshot_path(state_root, task_id).with_extension("lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|error| {
            CliError::CommandFailed(format!(
                "failed to open ACP snapshot generation lock for task {task_id}: {error}"
            ))
        })?;
    Flock::lock(file, FlockArg::LockExclusive).map_err(|(_file, error)| {
        CliError::CommandFailed(format!(
            "failed to acquire ACP snapshot generation lock for task {task_id}: {error}"
        ))
    })
}

fn write_snapshot(state_root: &Path, snapshot: &AcpRuntimeSnapshot) -> Result<(), CliError> {
    let final_path = snapshot_path(state_root, &snapshot.task_id);
    fs::create_dir_all(state_root).map_err(|error| {
        CliError::CommandFailed(format!("failed to create ACP snapshot directory: {error}"))
    })?;
    let encoded = serde_json::to_vec(snapshot)
        .map_err(|error| CliError::JsonSerialization(error.to_string()))?;
    let temporary_path = state_root.join(format!(
        ".{}.tmp-{}-{}",
        snapshot.task_id.replace(['/', '\\'], "__"),
        std::process::id(),
        TEMP_SUFFIX.fetch_add(1, Ordering::Relaxed)
    ));

    if let Err(error) = fs::write(&temporary_path, encoded) {
        let _ = fs::remove_file(&temporary_path);
        return Err(CliError::CommandFailed(format!(
            "failed to write ACP runtime snapshot: {error}"
        )));
    }
    if let Err(error) = fs::rename(&temporary_path, &final_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(CliError::CommandFailed(format!(
            "failed to publish ACP runtime snapshot: {error}"
        )));
    }
    Ok(())
}

pub(crate) fn snapshot_path(state_root: &Path, task_id: &str) -> PathBuf {
    state_root.join(format!("{}.json", task_id.replace(['/', '\\'], "__")))
}

pub(crate) fn read_snapshot(
    state_root: &Path,
    task_id: &str,
    now_millis: u128,
) -> Result<Option<AcpRuntimeSnapshot>, CliError> {
    let bytes = match fs::read(snapshot_path(state_root, task_id)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::CommandFailed(format!(
                "failed to read ACP runtime snapshot: {error}"
            )));
        }
    };
    let snapshot: AcpRuntimeSnapshot = match serde_json::from_slice(&bytes) {
        Ok(snapshot) => snapshot,
        Err(_) => return Ok(None),
    };
    if snapshot.schema_version != SCHEMA_VERSION || snapshot.task_id != task_id {
        return Ok(None);
    }
    let activity_is_stale = matches!(
        &snapshot.observation.state,
        AcpSessionState::Connecting | AcpSessionState::Running | AcpSessionState::RequiresAction(_)
    ) && now_millis.saturating_sub(snapshot.heartbeat_unix_millis)
        > SNAPSHOT_STALE_AFTER_MILLIS;

    Ok((!activity_is_stale).then_some(snapshot))
}

pub(crate) fn collect_statuses(
    state_root: &Path,
    task_ids: &[TaskId],
    now: SystemTime,
) -> Result<BTreeMap<TaskId, ObservedAcpStatus>, CliError> {
    let now_millis = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let mut statuses = BTreeMap::new();
    for task_id in task_ids {
        let Some(snapshot) = read_snapshot(state_root, task_id.as_str(), now_millis)? else {
            continue;
        };
        let Ok(heartbeat_millis) = u64::try_from(snapshot.heartbeat_unix_millis) else {
            continue;
        };
        let Some(observed_at) =
            SystemTime::UNIX_EPOCH.checked_add(Duration::from_millis(heartbeat_millis))
        else {
            continue;
        };
        statuses.insert(
            task_id.clone(),
            ObservedAcpStatus {
                observation: snapshot.observation,
                observed_at,
            },
        );
    }
    Ok(statuses)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use ajax_core::acp_status::{AcpActionKind, AcpStopReason};

    use super::*;

    static TEST_SUFFIX: AtomicU64 = AtomicU64::new(0);

    struct TempCache(PathBuf);

    impl TempCache {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "ajax-acp-snapshot-{}-{}",
                std::process::id(),
                TEST_SUFFIX.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempCache {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn observation(state: AcpSessionState) -> AcpStatusObservation {
        AcpStatusObservation {
            state,
            detail: None,
        }
    }

    fn write_fixture(state_root: &Path, task_id: &str, snapshot: &AcpRuntimeSnapshot) {
        let path = snapshot_path(state_root, task_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec(snapshot).unwrap()).unwrap();
    }

    fn read(state_root: &Path, task_id: &str, now_millis: u128) -> AcpRuntimeSnapshot {
        read_snapshot(state_root, task_id, now_millis)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn state_root_is_snapshot_directory() {
        let state_root = TempCache::new();
        let task_id = "repo/task\\handle";
        let expected_path = state_root.0.join("repo__task__handle.json");
        let running = observation(AcpSessionState::Running);
        AcpSnapshotPublisher::claim(
            &state_root.0,
            task_id,
            "generation-a",
            Some("session-a".to_owned()),
            running,
            100,
        )
        .unwrap();

        assert!(expected_path.is_file());
        assert_eq!(snapshot_path(&state_root.0, task_id), expected_path);
        assert!(
            !state_root.0.join("agent-acp").exists(),
            "state root must not create a nested agent-acp directory"
        );
    }

    #[test]
    fn atomic_round_trip_and_heartbeat() {
        let cache = TempCache::new();
        let state_root = cache.0.join("agent-acp");
        let task_id = "repo/task\\handle";
        let expected_path = state_root.join("repo__task__handle.json");
        let running = observation(AcpSessionState::Running);
        let mut publisher = AcpSnapshotPublisher::claim(
            &state_root,
            task_id,
            "generation-a",
            Some("session-a".to_owned()),
            running,
            100,
        )
        .unwrap();

        assert_eq!(snapshot_path(&state_root, task_id), expected_path);
        let claimed = read(&state_root, task_id, 100);
        assert_eq!(claimed.session_id.as_deref(), Some("session-a"));
        assert_eq!(claimed.heartbeat_unix_millis, 100);

        let waiting = observation(AcpSessionState::RequiresAction(AcpActionKind::Input));
        assert!(publisher
            .publish(Some("session-a".to_owned()), waiting.clone(), 101)
            .unwrap());
        assert_eq!(read(&state_root, task_id, 101).heartbeat_unix_millis, 101);

        assert!(!publisher
            .publish(Some("session-a".to_owned()), waiting.clone(), 5_100)
            .unwrap());
        assert_eq!(read(&state_root, task_id, 5_100).heartbeat_unix_millis, 101);

        assert!(publisher
            .publish(Some("session-a".to_owned()), waiting, 5_101)
            .unwrap());
        assert_eq!(
            read(&state_root, task_id, 5_101).heartbeat_unix_millis,
            5_101
        );

        let mut siblings: Vec<_> = fs::read_dir(expected_path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        siblings.sort();
        let mut expected = vec![
            expected_path.clone(),
            snapshot_path(&state_root, task_id).with_extension("lock"),
        ];
        expected.sort();
        assert_eq!(siblings, expected);
    }

    #[test]
    fn claim_and_publish_wait_for_task_generation_lock() {
        use std::{sync::mpsc, thread, time::Duration};

        use nix::fcntl::{Flock, FlockArg};

        let cache = TempCache::new();
        let state_root = cache.0.join("agent-acp");
        let task_id = "repo/task";
        let lock_path = snapshot_path(&state_root, task_id).with_extension("lock");
        let running = observation(AcpSessionState::Running);

        let mut publisher_a = AcpSnapshotPublisher::claim(
            &state_root,
            task_id,
            "generation-a",
            None,
            running.clone(),
            100,
        )
        .unwrap();

        fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        let held_lock = Flock::lock(lock_file, FlockArg::LockExclusive).unwrap();

        let state_root_for_claim = state_root.clone();
        let running_for_claim = running.clone();
        let (claim_started_tx, claim_started_rx) = mpsc::channel();
        let (claim_result_tx, claim_result_rx) = mpsc::channel();
        thread::spawn(move || {
            claim_started_tx.send(()).unwrap();
            let result = AcpSnapshotPublisher::claim(
                &state_root_for_claim,
                task_id,
                "generation-b",
                None,
                running_for_claim,
                200,
            );
            let _ = claim_result_tx.send(result);
        });

        claim_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("replacement claim thread should start");
        assert!(
            claim_result_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "claim should block while the per-task generation lock is held"
        );

        drop(held_lock);

        claim_result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("replacement claim should complete after lock release")
            .expect("replacement claim should succeed after lock release");

        let lock_file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)
            .unwrap();
        let held_lock = Flock::lock(lock_file, FlockArg::LockExclusive).unwrap();

        let waiting = observation(AcpSessionState::RequiresAction(AcpActionKind::Permission));
        let (publish_started_tx, publish_started_rx) = mpsc::channel();
        let (publish_result_tx, publish_result_rx) = mpsc::channel();
        thread::spawn(move || {
            publish_started_tx.send(()).unwrap();
            let result = publisher_a.publish(None, waiting, 201);
            let _ = publish_result_tx.send(result);
        });

        publish_started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("stale publish thread should start");
        assert!(
            publish_result_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "publish should block while the per-task generation lock is held"
        );

        drop(held_lock);

        let publish_error = publish_result_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("stale publish should complete after lock release")
            .expect_err("stale publish should reject the newer generation");
        assert!(publish_error.to_string().contains("generation mismatch"));
        assert_eq!(read(&state_root, task_id, 201).generation, "generation-b");
    }

    #[test]
    fn rejects_generation_mismatch() {
        let cache = TempCache::new();
        let state_root = cache.0.join("agent-acp");
        let task_id = "repo/task";
        let running = observation(AcpSessionState::Running);
        let claim = |generation, now| {
            AcpSnapshotPublisher::claim(
                &state_root,
                task_id,
                generation,
                None,
                running.clone(),
                now,
            )
            .unwrap()
        };
        let mut publisher_a = claim("generation-a", 100);
        let mut publisher_b = claim("generation-b", 200);
        let waiting = observation(AcpSessionState::RequiresAction(AcpActionKind::Permission));

        let error = publisher_a.publish(None, waiting.clone(), 201).unwrap_err();
        assert!(error.to_string().contains("generation mismatch"));
        assert_eq!(read(&state_root, task_id, 201).generation, "generation-b");

        assert!(publisher_b.publish(None, waiting, 201).unwrap());
    }

    #[test]
    fn ignores_malformed_and_stale_activity() {
        let cache = TempCache::new();
        let state_root = cache.0.join("agent-acp");
        let task_id = "repo/task";
        let path = snapshot_path(&state_root, task_id);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"{").unwrap();
        assert_eq!(read_snapshot(&state_root, task_id, 20_001).unwrap(), None);

        let mut snapshot = AcpRuntimeSnapshot {
            schema_version: SCHEMA_VERSION + 1,
            task_id: task_id.to_owned(),
            generation: "generation-a".to_owned(),
            session_id: None,
            heartbeat_unix_millis: 5_000,
            observation: observation(AcpSessionState::Running),
        };
        write_fixture(&state_root, task_id, &snapshot);
        assert_eq!(read_snapshot(&state_root, task_id, 20_001).unwrap(), None);

        snapshot.schema_version = SCHEMA_VERSION;
        snapshot.task_id = "other/task".to_owned();
        write_fixture(&state_root, task_id, &snapshot);
        assert_eq!(read_snapshot(&state_root, task_id, 20_001).unwrap(), None);

        snapshot.task_id = task_id.to_owned();
        write_fixture(&state_root, task_id, &snapshot);
        assert_eq!(read_snapshot(&state_root, task_id, 20_001).unwrap(), None);

        snapshot.observation = observation(AcpSessionState::Idle(Some(AcpStopReason::EndTurn)));
        write_fixture(&state_root, task_id, &snapshot);
        assert_eq!(
            read_snapshot(&state_root, task_id, 20_001).unwrap(),
            Some(snapshot)
        );
    }

    #[test]
    fn collects_fresh_statuses_and_ignores_legacy_or_stale_files() {
        let cache = TempCache::new();
        let state_root = cache.0.join("agent-acp");
        let task_id = TaskId::new("repo/task");
        let stem = "repo__task";
        fs::create_dir_all(cache.0.join("agent-events")).unwrap();
        fs::create_dir_all(cache.0.join("agent-runtime")).unwrap();
        fs::write(
            cache.0.join("agent-events").join(format!("{stem}.jsonl")),
            b"{\"status\":\"working\"}\n",
        )
        .unwrap();
        fs::write(
            cache.0.join("agent-runtime").join(format!("{stem}.json")),
            b"{\"state\":\"running\"}",
        )
        .unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_millis(20_000);

        assert!(
            collect_statuses(&state_root, std::slice::from_ref(&task_id), now)
                .unwrap()
                .is_empty()
        );

        let running = observation(AcpSessionState::Running);
        let mut snapshot = AcpRuntimeSnapshot {
            schema_version: SCHEMA_VERSION,
            task_id: task_id.as_str().to_owned(),
            generation: "generation-a".to_owned(),
            session_id: Some("session-a".to_owned()),
            heartbeat_unix_millis: 20_000,
            observation: running.clone(),
        };
        write_fixture(&state_root, task_id.as_str(), &snapshot);
        let statuses = collect_statuses(&state_root, std::slice::from_ref(&task_id), now).unwrap();
        assert_eq!(statuses.len(), 1);
        assert_eq!(
            statuses.get(&task_id),
            Some(&ObservedAcpStatus {
                observation: running,
                observed_at: now,
            })
        );

        snapshot.heartbeat_unix_millis = 4_999;
        write_fixture(&state_root, task_id.as_str(), &snapshot);
        assert!(
            collect_statuses(&state_root, std::slice::from_ref(&task_id), now)
                .unwrap()
                .is_empty()
        );

        snapshot.heartbeat_unix_millis = u128::from(u64::MAX) + 1;
        write_fixture(&state_root, task_id.as_str(), &snapshot);
        assert!(collect_statuses(&state_root, &[task_id], now)
            .unwrap()
            .is_empty());
    }
}
