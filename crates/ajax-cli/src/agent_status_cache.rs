//! Native hook-derived agent-status evidence for runtime refresh.
//!
//! Reads only the two files Ajax itself writes per task: the canonical event
//! log (`agent-events/{stem}.jsonl`) and the launch-wrapper runtime snapshot
//! (`agent-runtime/{stem}.json`). It folds the canonical log into reducer
//! observations and translates confirmed wrapper exit / liveness. There are no
//! legacy `~/.cache/tmux-agent-status` reads, no pane status files, no
//! pane-text inference, and no scalar status snapshots.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use ajax_core::{
    agent_status::{
        ActivityKind, Confidence, ObservationSource, ProcessLiveness, StatusObservation,
    },
    canonical_agent_event::{fold_envelopes, observations_from_run_snapshot},
    models::TaskId,
    runtime_refresh::{AgentStatusSource, PRIMARY_RUN_ID},
};

use crate::agent_event::parse_envelopes_from_jsonl;
use crate::agent_runtime::{task_file_stem, AgentRuntimeSnapshot, AgentRuntimeState};

/// Freshness window for a confirmed wrapper exit, matching the prior terminal
/// window: the wrapper only vouches for the process it supervised.
const WRAPPER_TERMINAL_FRESH_FOR: Duration = Duration::from_secs(120);

/// Filesystem source of native hook-derived agent status for a task.
pub(crate) struct AgentStatusFiles {
    events_dir: PathBuf,
    runtime_dir: PathBuf,
}

impl AgentStatusFiles {
    pub(crate) fn from_runtime_cache(cache_dir: &Path) -> Self {
        Self {
            events_dir: cache_dir.join("agent-events"),
            runtime_dir: cache_dir.join("agent-runtime"),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_dirs(events_dir: PathBuf, runtime_dir: PathBuf) -> Self {
        Self {
            events_dir,
            runtime_dir,
        }
    }

    fn read_runtime_snapshot(&self, stem: &str) -> Option<AgentRuntimeSnapshot> {
        let path = self.runtime_dir.join(format!("{stem}.json"));
        serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
    }
}

impl AgentStatusSource for AgentStatusFiles {
    fn observations_for_task(&self, task_id: &TaskId) -> Vec<StatusObservation> {
        let now = SystemTime::now();
        let stem = task_file_stem(task_id.as_str());
        let mut observations = Vec::new();

        // Native lifecycle: fold the canonical JSONL log for the primary run.
        let jsonl = self.events_dir.join(format!("{stem}.jsonl"));
        let envelopes = parse_envelopes_from_jsonl(&jsonl);
        if !envelopes.is_empty() {
            let observed_at = envelopes
                .iter()
                .map(|event| event.received_at_unix_millis)
                .max()
                .and_then(millis_to_systemtime)
                .unwrap_or(now);
            let snapshot = fold_envelopes(&envelopes);
            observations.extend(observations_from_run_snapshot(
                &snapshot,
                observed_at,
                PRIMARY_RUN_ID,
            ));
        }

        // Confirmed wrapper exit is a terminal fallback (requirement 12).
        if let Some(snapshot) = self.read_runtime_snapshot(&stem) {
            if let Some(observation) = wrapper_exit_observation(&snapshot) {
                observations.push(observation);
            }
        }

        observations
    }

    fn process_liveness_for_task(&self, task_id: &TaskId) -> Option<ProcessLiveness> {
        let stem = task_file_stem(task_id.as_str());
        let snapshot = self.read_runtime_snapshot(&stem)?;
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

fn millis_to_systemtime(millis: u128) -> Option<SystemTime> {
    let millis = u64::try_from(millis).ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_millis(millis))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ajax_core::{
        agent_status::{ActivityKind, ObservationSource},
        models::TaskId,
        runtime_refresh::AgentStatusSource,
    };

    use crate::agent_event::{run_agent_event, AgentEventIdentity};
    use crate::agent_runtime::{AgentRuntimeSnapshot, AgentRuntimeState};

    use super::AgentStatusFiles;

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
}
