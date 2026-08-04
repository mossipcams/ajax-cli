//! Native hook-derived agent-status evidence for runtime refresh.
//!
//! Reads only the two files Ajax itself writes per task: the canonical event
//! log (`agent-events/{stem}.jsonl`) and the launch-wrapper runtime snapshot
//! (`agent-runtime/{stem}.json`). It folds the canonical log into reducer
//! observations and translates confirmed wrapper exit / liveness. There are no
//! legacy `~/.cache/tmux-agent-status` reads, no pane status files, no
//! pane-text inference, and no scalar status snapshots.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, SystemTime},
};

static SHARED_AGENT_STATUS: Mutex<Option<(PathBuf, Arc<AgentStatusFiles>)>> = Mutex::new(None);

use ajax_core::{
    agent_status::{
        ActivityKind, Confidence, ObservationSource, ProcessLiveness, StatusObservation,
    },
    canonical_agent_event::{fold_envelopes, observations_from_run_snapshot, ParsedEnvelope},
    models::TaskId,
    runtime_refresh::{AgentStatusSource, PRIMARY_RUN_ID},
};

use crate::agent_event::parse_envelopes_from_jsonl;
use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};

/// Freshness window for a confirmed wrapper exit, matching the prior terminal
/// window: the wrapper only vouches for the process it supervised.
const WRAPPER_TERMINAL_FRESH_FOR: Duration = Duration::from_secs(120);

/// On-disk metadata used to skip unchanged file reads within a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileStamp {
    mtime: Option<SystemTime>,
    len: u64,
}

struct JsonlCacheEntry {
    stamp: FileStamp,
    observations: Vec<StatusObservation>,
}

struct RuntimeCacheEntry {
    stamp: FileStamp,
    snapshot: Option<AgentRuntimeSnapshot>,
}

#[derive(Default)]
struct AgentStatusCaches {
    jsonl: HashMap<String, JsonlCacheEntry>,
    runtime: HashMap<String, RuntimeCacheEntry>,
}

/// Filesystem source of native hook-derived agent status for a task.
pub(crate) struct AgentStatusFiles {
    events_dir: PathBuf,
    runtime_dir: PathBuf,
    cache: Mutex<AgentStatusCaches>,
}

impl AgentStatusFiles {
    pub(crate) fn from_runtime_cache(cache_dir: &Path) -> Self {
        Self {
            events_dir: cache_dir.join("agent-events"),
            runtime_dir: cache_dir.join("agent-runtime"),
            cache: Mutex::new(AgentStatusCaches::default()),
        }
    }

    pub(crate) fn shared_from_runtime_cache(cache_dir: &Path) -> Arc<Self> {
        let mut slot = SHARED_AGENT_STATUS
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((cached_dir, shared)) = slot.as_ref() {
            if cached_dir == cache_dir {
                return Arc::clone(shared);
            }
        }
        let shared = Arc::new(Self::from_runtime_cache(cache_dir));
        *slot = Some((cache_dir.to_path_buf(), Arc::clone(&shared)));
        shared
    }

    #[cfg(test)]
    pub(crate) fn from_dirs(events_dir: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            events_dir,
            runtime_dir,
            cache: Mutex::new(AgentStatusCaches::default()),
        }
    }

    fn file_stamp(path: &Path) -> FileStamp {
        match fs::metadata(path) {
            Ok(meta) => FileStamp {
                mtime: meta.modified().ok(),
                len: meta.len(),
            },
            Err(_) => FileStamp {
                mtime: None,
                len: 0,
            },
        }
    }

    fn jsonl_observations_for_stem(
        &self,
        stem: &str,
        now: SystemTime,
        caches: &mut AgentStatusCaches,
    ) -> Vec<StatusObservation> {
        let path = self.events_dir.join(format!("{stem}.jsonl"));
        let stamp = Self::file_stamp(&path);
        if let Some(entry) = caches.jsonl.get(stem) {
            if entry.stamp == stamp {
                return entry.observations.clone();
            }
        }

        #[cfg(test)]
        test_counters::JSONL_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let observations = observations_from_jsonl(&path, now);
        caches.jsonl.insert(
            stem.to_string(),
            JsonlCacheEntry {
                stamp,
                observations: observations.clone(),
            },
        );
        observations
    }

    fn runtime_snapshot_for_stem(
        &self,
        stem: &str,
        caches: &mut AgentStatusCaches,
    ) -> Option<AgentRuntimeSnapshot> {
        let path = self.runtime_dir.join(format!("{stem}.json"));
        let stamp = Self::file_stamp(&path);
        if let Some(entry) = caches.runtime.get(stem) {
            if entry.stamp == stamp {
                return entry.snapshot.clone();
            }
        }

        #[cfg(test)]
        test_counters::RUNTIME_READS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let snapshot = if stamp.len == 0 {
            None
        } else {
            serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()
        };
        caches.runtime.insert(
            stem.to_string(),
            RuntimeCacheEntry {
                stamp,
                snapshot: snapshot.clone(),
            },
        );
        snapshot
    }
}

impl AgentStatusSource for AgentStatusFiles {
    fn observations_for_task(&self, task_id: &TaskId) -> Vec<StatusObservation> {
        let now = SystemTime::now();
        let stem = task_file_stem(task_id.as_str());
        let mut caches = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let mut observations = self.jsonl_observations_for_stem(&stem, now, &mut caches);

        // Confirmed wrapper exit is a terminal fallback (requirement 12).
        if let Some(snapshot) = self.runtime_snapshot_for_stem(&stem, &mut caches) {
            if let Some(observation) = wrapper_exit_observation(&snapshot) {
                observations.push(observation);
            }
        }

        observations
    }

    fn process_liveness_for_task(&self, task_id: &TaskId) -> Option<ProcessLiveness> {
        let stem = task_file_stem(task_id.as_str());
        let mut caches = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let snapshot = self.runtime_snapshot_for_stem(&stem, &mut caches)?;
        match snapshot.state {
            AgentRuntimeState::Starting | AgentRuntimeState::Running => Some(ProcessLiveness {
                alive: true,
                observed_at: millis_to_systemtime(snapshot.observed_at_unix_millis)?,
            }),
            AgentRuntimeState::ExitedSuccess | AgentRuntimeState::ExitedFailure => None,
        }
    }
}

/// Translate a confirmed wrapper exit into a terminal `ProcessExit`
/// observation. `Starting`/`Running` yield no activity — only liveness.
fn wrapper_exit_observation(snapshot: &AgentRuntimeSnapshot) -> Option<StatusObservation> {
    let kind = match snapshot.state {
        AgentRuntimeState::ExitedSuccess => ActivityKind::Done,
        AgentRuntimeState::ExitedFailure => ActivityKind::Failed,
        AgentRuntimeState::Starting | AgentRuntimeState::Running => return None,
    };
    let observed_at = millis_to_systemtime(snapshot.observed_at_unix_millis)?;
    Some(StatusObservation {
        source: ObservationSource::ProcessExit,
        observed_at,
        expires_at: observed_at + WRAPPER_TERMINAL_FRESH_FOR,
        confidence: Confidence::High,
        run_id: PRIMARY_RUN_ID.to_string(),
        parent_run_id: None,
        kind,
    })
}

/// Fold a task's canonical log into reducer-ready observations.
fn observations_from_jsonl(jsonl: &Path, now: SystemTime) -> Vec<StatusObservation> {
    let mut observations = Vec::new();
    // Native lifecycle: every run appends to the one per-task log, so group
    // by run before folding — a child's events must not move the parent's
    // phase. Each run yields its own observation so the reducer can
    // aggregate the run graph.
    for (run_id, parent_run_id, envelopes) in group_envelopes_by_run(jsonl) {
        let observed_at = envelopes
            .iter()
            .map(|event| event.received_at_unix_millis)
            .max()
            .and_then(millis_to_systemtime)
            .unwrap_or(now);
        let snapshot = fold_envelopes(&envelopes);
        observations.extend(
            observations_from_run_snapshot(&snapshot, observed_at, &run_id)
                .into_iter()
                .map(|observation| StatusObservation {
                    parent_run_id: parent_run_id.clone(),
                    ..observation
                }),
        );
    }
    observations
}

/// Group a task's canonical log into `(run_id, parent_run_id, envelopes)`,
/// preserving append order within each run. Envelopes with no `run_id` (written
/// before the field existed) fold into the primary run, and the primary run is
/// always keyed [`PRIMARY_RUN_ID`] with no parent.
fn group_envelopes_by_run(jsonl: &Path) -> Vec<(String, Option<String>, Vec<ParsedEnvelope>)> {
    let mut runs: Vec<(String, Option<String>, Vec<ParsedEnvelope>)> = Vec::new();
    for envelope in parse_envelopes_from_jsonl(jsonl) {
        let run_id = envelope
            .run_id
            .clone()
            .filter(|run| !run.trim().is_empty())
            .unwrap_or_else(|| PRIMARY_RUN_ID.to_string());
        let parent_run_id = if run_id == PRIMARY_RUN_ID {
            None
        } else {
            envelope
                .parent_run_id
                .clone()
                .filter(|parent| !parent.trim().is_empty())
                .or_else(|| Some(PRIMARY_RUN_ID.to_string()))
        };
        match runs.iter_mut().find(|(id, _, _)| *id == run_id) {
            Some((_, _, envelopes)) => envelopes.push(envelope),
            None => runs.push((run_id, parent_run_id, vec![envelope])),
        }
    }
    runs
}

fn millis_to_systemtime(millis: u128) -> Option<SystemTime> {
    let millis = u64::try_from(millis).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_millis(millis))
}

#[cfg(test)]
mod test_counters {
    use std::sync::atomic::AtomicUsize;

    pub(super) static JSONL_READS: AtomicUsize = AtomicUsize::new(0);
    pub(super) static RUNTIME_READS: AtomicUsize = AtomicUsize::new(0);
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::atomic::Ordering};

    use ajax_core::{
        agent_status::{ActivityKind, ObservationSource},
        models::TaskId,
        runtime_refresh::AgentStatusSource,
    };

    use crate::agent_event::{run_agent_event, AgentEventIdentity};
    use crate::agent_runtime::{AgentRuntimeSnapshot, AgentRuntimeState};

    use super::{test_counters, AgentStatusFiles};

    fn reset_read_counters() {
        test_counters::JSONL_READS.store(0, Ordering::SeqCst);
        test_counters::RUNTIME_READS.store(0, Ordering::SeqCst);
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ajax-agent-source-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("agent-events")).unwrap();
        fs::create_dir_all(root.join("agent-runtime")).unwrap();
        root
    }

    fn write_runtime(root: &std::path::Path, task_id: &str, state: AgentRuntimeState, at: u128) {
        let snapshot = AgentRuntimeSnapshot {
            task_id: task_id.to_string(),
            state,
            observed_at_unix_millis: at,
            pid: Some(1),
            exit_code: None,
            message: None,
        };
        let stem = crate::agent_runtime::task_file_stem(task_id);
        fs::write(
            root.join("agent-runtime").join(format!("{stem}.json")),
            serde_json::to_vec(&snapshot).unwrap(),
        )
        .unwrap();
    }

    fn source(root: &std::path::Path) -> AgentStatusFiles {
        AgentStatusFiles::from_dirs(root.join("agent-events"), root.join("agent-runtime"))
    }

    #[test]
    fn native_turn_started_yields_running_lifecycle_observation() {
        let root = temp_root("running");
        let events_dir = root.join("agent-events");
        write_runtime(&root, "web/fix-login", AgentRuntimeState::Running, 1);
        let identity = AgentEventIdentity {
            task_id: "web/fix-login".to_string(),
            run_id: "primary".to_string(),
            events_dir: events_dir.clone(),
        };
        run_agent_event(
            Some(&identity),
            "claude",
            "UserPromptSubmit",
            &serde_json::json!({}),
        )
        .unwrap();

        let observations = source(&root).observations_for_task(&TaskId::new("web/fix-login"));
        assert!(observations
            .iter()
            .any(|o| o.source == ObservationSource::ProviderLifecycle
                && o.kind == ActivityKind::Working));
        // Resume-race guard: while the wrapper snapshot says Running, no
        // terminal ProcessExit observation is produced, so a fresh native turn
        // can never be dragged back to Done by a prior exit.
        assert!(!observations
            .iter()
            .any(|o| o.source == ObservationSource::ProcessExit));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wrapper_exit_yields_process_exit_terminal_observation() {
        let root = temp_root("exit");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::ExitedSuccess,
            crate::agent_runtime::now_millis().unwrap(),
        );

        let src = source(&root);
        let observations = src.observations_for_task(&TaskId::new("web/fix-login"));
        assert!(observations
            .iter()
            .any(|o| o.source == ObservationSource::ProcessExit && o.kind == ActivityKind::Done));
        // A confirmed exit is not liveness.
        assert!(src
            .process_liveness_for_task(&TaskId::new("web/fix-login"))
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn running_wrapper_is_liveness_only_not_activity() {
        let root = temp_root("liveness");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );

        let src = source(&root);
        // No native events and only a running wrapper: no activity observation.
        assert!(src
            .observations_for_task(&TaskId::new("web/fix-login"))
            .is_empty());
        assert!(src
            .process_liveness_for_task(&TaskId::new("web/fix-login"))
            .is_some_and(|liveness| liveness.alive));
        fs::remove_dir_all(root).unwrap();
    }

    /// Append a raw canonical envelope for an explicit run, which
    /// `run_agent_event` cannot do (it only ever writes `primary`).
    fn append_envelope(root: &std::path::Path, task_id: &str, run: &str, kind: &str, at: u128) {
        let stem = crate::agent_runtime::task_file_stem(task_id);
        let mut envelope = serde_json::json!({
            "schema_version": 1,
            "task_id": task_id,
            "run_id": run,
            "kind": kind,
            "received_at_unix_millis": at,
            "occurred_at_unix_millis": at,
        });
        if run != "primary" {
            envelope["parent_run_id"] = serde_json::json!("primary");
        }
        if kind == "turn_settled" {
            envelope["detail"] = serde_json::json!({"outcome": {"outcome": "completed"}});
        }
        let path = root.join("agent-events").join(format!("{stem}.jsonl"));
        let mut line = serde_json::to_string(&envelope).unwrap();
        line.push('\n');
        use std::io::Write;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap()
            .write_all(line.as_bytes())
            .unwrap();
    }

    #[test]
    fn delegated_run_events_do_not_move_the_primary_phase() {
        // architecture.md: "Parent and delegated runs are aggregated as a run
        // graph: a parent is not fully complete while non-detached descendants
        // remain active." Every run appends to the one {stem}.jsonl, so the
        // fold must be per-run — otherwise a child's TurnStarted drags the
        // settled parent back to Working.
        let root = temp_root("per-run");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );
        let base = crate::agent_runtime::now_millis().unwrap();
        append_envelope(&root, "web/fix-login", "primary", "turn_settled", base);
        append_envelope(&root, "web/fix-login", "deleg-1", "turn_started", base + 10);

        let observations = source(&root).observations_for_task(&TaskId::new("web/fix-login"));

        let primary = observations
            .iter()
            .find(|o| o.run_id == "primary")
            .expect("primary run observation");
        assert_eq!(
            primary.kind,
            ActivityKind::Done,
            "the child's turn must not reopen the settled parent"
        );

        let child = observations
            .iter()
            .find(|o| o.run_id == "deleg-1")
            .expect("delegated run observation");
        assert_eq!(child.kind, ActivityKind::Working);
        assert_eq!(child.parent_run_id.as_deref(), Some("primary"));

        // The reducer can now actually see the run graph.
        let projection =
            ajax_core::agent_status::reduce_agent_status(ajax_core::agent_status::ReduceInput {
                now: std::time::SystemTime::now(),
                primary_run_id: "primary".to_string(),
                process_liveness: None,
                observations: &observations,
            });
        assert_eq!(
            projection.phase,
            ajax_core::agent_status::ParentPhase::CompletedLocallyChildrenActive,
            "parent is not fully complete while a descendant is active"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn no_files_yields_no_observations() {
        let root = temp_root("empty");
        let src = source(&root);
        assert!(src
            .observations_for_task(&TaskId::new("web/none"))
            .is_empty());
        assert!(src
            .process_liveness_for_task(&TaskId::new("web/none"))
            .is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn jsonl_cache_hit_skips_reread_on_unchanged_metadata() {
        reset_read_counters();
        let root = temp_root("jsonl-cache-hit");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );
        let base = crate::agent_runtime::now_millis().unwrap();
        append_envelope(&root, "web/fix-login", "primary", "turn_started", base);

        let src = source(&root);
        let task_id = TaskId::new("web/fix-login");
        let first = src.observations_for_task(&task_id);
        let second = src.observations_for_task(&task_id);

        assert_eq!(first, second);
        assert_eq!(test_counters::JSONL_READS.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn jsonl_append_invalidates_cache() {
        reset_read_counters();
        let root = temp_root("jsonl-cache-append");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );
        let base = crate::agent_runtime::now_millis().unwrap();
        append_envelope(&root, "web/fix-login", "primary", "turn_started", base);

        let src = source(&root);
        let task_id = TaskId::new("web/fix-login");
        let before = src.observations_for_task(&task_id);
        append_envelope(&root, "web/fix-login", "primary", "turn_settled", base + 10);
        let after = src.observations_for_task(&task_id);

        assert_ne!(before, after);
        assert!(after.iter().any(|o| o.kind == ActivityKind::Done));
        assert_eq!(test_counters::JSONL_READS.load(Ordering::SeqCst), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn jsonl_truncate_invalidates_cache() {
        reset_read_counters();
        let root = temp_root("jsonl-cache-truncate");
        let base = crate::agent_runtime::now_millis().unwrap();
        append_envelope(&root, "web/fix-login", "primary", "turn_started", base);

        let src = source(&root);
        let task_id = TaskId::new("web/fix-login");
        assert!(!src.observations_for_task(&task_id).is_empty());

        let stem = crate::agent_runtime::task_file_stem("web/fix-login");
        fs::write(root.join("agent-events").join(format!("{stem}.jsonl")), b"").unwrap();

        assert!(src.observations_for_task(&task_id).is_empty());
        assert_eq!(test_counters::JSONL_READS.load(Ordering::SeqCst), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shared_runtime_cache_reuses_jsonl_reads_across_instances() {
        reset_read_counters();
        let root = temp_root("shared-runtime-cache");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );
        let base = crate::agent_runtime::now_millis().unwrap();
        append_envelope(&root, "web/fix-login", "primary", "turn_started", base);

        let first = AgentStatusFiles::shared_from_runtime_cache(&root);
        let task_id = TaskId::new("web/fix-login");
        let obs1 = first.observations_for_task(&task_id);

        let second = AgentStatusFiles::shared_from_runtime_cache(&root);
        let obs2 = second.observations_for_task(&task_id);

        assert_eq!(obs1, obs2);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        assert_eq!(test_counters::JSONL_READS.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_snapshot_shared_between_observations_and_liveness() {
        reset_read_counters();
        let root = temp_root("runtime-cache-shared");
        write_runtime(
            &root,
            "web/fix-login",
            AgentRuntimeState::Running,
            crate::agent_runtime::now_millis().unwrap(),
        );

        let src = source(&root);
        let task_id = TaskId::new("web/fix-login");
        assert!(src.observations_for_task(&task_id).is_empty());
        assert!(src
            .process_liveness_for_task(&task_id)
            .is_some_and(|liveness| liveness.alive));
        assert_eq!(test_counters::RUNTIME_READS.load(Ordering::SeqCst), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
