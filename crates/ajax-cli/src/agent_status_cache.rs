use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use ajax_core::runtime_refresh::{AgentStatusCache, AgentStatusCacheEntry, AgentStatusCacheSource};

use crate::agent_runtime::{AgentRuntimeSnapshot, AgentRuntimeState};

const AGENT_STATUS_FRESH_FOR: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TmuxAgentStatusSnapshot {
    by_session: BTreeMap<String, Vec<AgentStatusCacheEntry>>,
    by_task: BTreeMap<String, Vec<AgentStatusCacheEntry>>,
}

impl TmuxAgentStatusSnapshot {
    #[cfg(test)]
    pub(crate) fn from_root(root: impl AsRef<Path>) -> Self {
        Self::from_root_at(root, SystemTime::now(), AGENT_STATUS_FRESH_FOR)
    }

    fn from_root_at(root: impl AsRef<Path>, now: SystemTime, fresh_for: Duration) -> Self {
        let root = root.as_ref();
        let mut by_session: BTreeMap<String, Vec<AgentStatusCacheEntry>> = BTreeMap::new();

        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(session) = file_name.strip_suffix(".status") else {
                    continue;
                };
                if let Some(entry) = read_status_entry(&path, now, fresh_for) {
                    by_session
                        .entry(session.to_string())
                        .or_default()
                        .push(entry);
                }
            }
        }

        let pane_dir = root.join("panes");
        if let Ok(entries) = fs::read_dir(pane_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some((session, _pane_id)) = file_name.split_once('_') else {
                    continue;
                };
                if !file_name.ends_with(".status") {
                    continue;
                }
                if let Some(entry) = read_status_entry(&path, now, fresh_for) {
                    by_session
                        .entry(session.to_string())
                        .or_default()
                        .push(entry);
                }
            }
        }

        Self {
            by_session,
            by_task: BTreeMap::new(),
        }
    }

    fn from_roots_at(
        tmux_root: impl AsRef<Path>,
        runtime_root: impl AsRef<Path>,
        now: SystemTime,
        fresh_for: Duration,
    ) -> Self {
        let mut snapshot = Self::from_root_at(tmux_root, now, fresh_for);
        if let Ok(entries) = fs::read_dir(runtime_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    continue;
                }
                let Some((task_id, status)) = read_agent_runtime_entry(&path, now, fresh_for)
                else {
                    continue;
                };
                snapshot.by_task.entry(task_id).or_default().push(status);
            }
        }
        snapshot
    }

    pub(crate) fn from_runtime_cache(cache_dir: &Path) -> Self {
        let tmux_root = std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| home.join(".cache/tmux-agent-status"))
            .unwrap_or_default();
        Self::from_roots_at(
            tmux_root,
            cache_dir.join("agent-runtime"),
            SystemTime::now(),
            AGENT_STATUS_FRESH_FOR,
        )
    }
}

impl AgentStatusCache for TmuxAgentStatusSnapshot {
    fn status_entries_for_session(&self, session: &str) -> Vec<AgentStatusCacheEntry> {
        self.by_session.get(session).cloned().unwrap_or_default()
    }

    fn status_entries_for_task(
        &self,
        task_id: &ajax_core::models::TaskId,
        session: &str,
    ) -> Vec<AgentStatusCacheEntry> {
        let mut entries = self.status_entries_for_session(session);
        if let Some(runtime_entries) = self.by_task.get(task_id.as_str()) {
            entries.extend(runtime_entries.iter().cloned());
        }
        entries
    }
}

fn read_status_entry(
    path: &Path,
    now: SystemTime,
    fresh_for: Duration,
) -> Option<AgentStatusCacheEntry> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let observed_at = fs::metadata(path).ok()?.modified().ok()?;
    let fresh = now
        .duration_since(observed_at)
        .map_or(true, |age| age <= fresh_for);
    Some(AgentStatusCacheEntry {
        value: value.to_string(),
        observed_at,
        fresh,
        source: AgentStatusCacheSource::Hook,
    })
}

fn read_agent_runtime_entry(
    path: &Path,
    now: SystemTime,
    fresh_for: Duration,
) -> Option<(String, AgentStatusCacheEntry)> {
    let snapshot =
        serde_json::from_str::<AgentRuntimeSnapshot>(&fs::read_to_string(path).ok()?).ok()?;
    let millis = u64::try_from(snapshot.observed_at_unix_millis).ok()?;
    let observed_at = SystemTime::UNIX_EPOCH + Duration::from_millis(millis);
    let terminal = matches!(
        snapshot.state,
        AgentRuntimeState::ExitedSuccess | AgentRuntimeState::ExitedFailure
    );
    let fresh = terminal
        || now
            .duration_since(observed_at)
            .map_or(true, |age| age <= fresh_for);
    let value = match snapshot.state {
        AgentRuntimeState::Starting => "starting",
        AgentRuntimeState::Running => "working",
        AgentRuntimeState::ExitedSuccess => "done",
        AgentRuntimeState::ExitedFailure => "failed",
    };

    Some((
        snapshot.task_id,
        AgentStatusCacheEntry {
            value: value.to_string(),
            observed_at,
            fresh,
            source: AgentStatusCacheSource::RuntimeWrapper,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File, FileTimes},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use ajax_core::{
        models::TaskId,
        runtime_refresh::{AgentStatusCache, AgentStatusCacheEntry, AgentStatusCacheSource},
    };

    use super::TmuxAgentStatusSnapshot;

    fn temp_cache_root() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-agent-status-cache-{suffix}"))
    }

    #[test]
    fn tmux_agent_status_snapshot_reads_session_and_pane_values_once() {
        let root = temp_cache_root();
        let pane_dir = root.join("panes");
        fs::create_dir_all(&pane_dir).unwrap();
        fs::write(root.join("ajax-web-fix-login.status"), "done\n").unwrap();
        fs::write(pane_dir.join("ajax-web-fix-login_%1.status"), "working\n").unwrap();
        fs::write(pane_dir.join("ajax-web-fix-login_%2.status"), "wait\n").unwrap();
        fs::write(pane_dir.join("ajax-other_%3.status"), "working\n").unwrap();
        let cache = TmuxAgentStatusSnapshot::from_root(root.clone());

        let mut values = cache
            .status_entries_for_session("ajax-web-fix-login")
            .into_iter()
            .map(|entry| entry.value)
            .collect::<Vec<_>>();
        values.sort();

        assert_eq!(values, vec!["done", "wait", "working"]);
        assert_eq!(cache.by_session.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }

    fn set_modified(path: &std::path::Path, modified: SystemTime) {
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(modified))
            .unwrap();
    }

    #[test]
    fn tmux_agent_status_snapshot_marks_old_files_stale() {
        let root = temp_cache_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("ajax-web-fix-login.status");
        fs::write(&path, "working\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        set_modified(&path, now - Duration::from_secs(60));

        let cache = TmuxAgentStatusSnapshot::from_root_at(&root, now, Duration::from_secs(30));
        let entries = cache.status_entries_for_session("ajax-web-fix-login");

        assert_eq!(
            entries,
            vec![AgentStatusCacheEntry {
                value: "working".to_string(),
                observed_at: now - Duration::from_secs(60),
                fresh: false,
                source: AgentStatusCacheSource::Hook,
            }]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn newest_fresh_agent_status_wins_over_older_working_value() {
        let root = temp_cache_root();
        let pane_dir = root.join("panes");
        fs::create_dir_all(&pane_dir).unwrap();
        let working = root.join("ajax-web-fix-login.status");
        let done = pane_dir.join("ajax-web-fix-login_%1.status");
        fs::write(&working, "working\n").unwrap();
        fs::write(&done, "done\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        set_modified(&working, now - Duration::from_secs(10));
        set_modified(&done, now - Duration::from_secs(2));

        let cache = TmuxAgentStatusSnapshot::from_root_at(&root, now, Duration::from_secs(30));

        let latest = cache
            .status_entries_for_session("ajax-web-fix-login")
            .into_iter()
            .filter(|entry| entry.fresh)
            .max_by_key(|entry| entry.observed_at)
            .map(|entry| entry.value);

        assert_eq!(latest.as_deref(), Some("done"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_status_snapshot_merges_wrapper_status_by_task_id() {
        let root = temp_cache_root();
        let tmux_root = root.join("tmux-agent-status");
        let runtime_root = root.join("agent-runtime");
        fs::create_dir_all(&tmux_root).unwrap();
        fs::create_dir_all(&runtime_root).unwrap();
        let hook = tmux_root.join("ajax-web-fix-login.status");
        fs::write(&hook, "working\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        set_modified(&hook, now - Duration::from_secs(60));
        fs::write(
            runtime_root.join("web__fix-login.json"),
            r#"{"task_id":"web/fix-login","state":"exited_success","observed_at_unix_millis":1000000,"pid":42,"exit_code":0,"message":null}"#,
        )
        .unwrap();

        let cache = TmuxAgentStatusSnapshot::from_roots_at(
            &tmux_root,
            &runtime_root,
            now,
            Duration::from_secs(30),
        );
        let entries =
            cache.status_entries_for_task(&TaskId::new("web/fix-login"), "ajax-web-fix-login");

        assert!(entries.iter().any(|entry| {
            entry.value == "working" && !entry.fresh && entry.source == AgentStatusCacheSource::Hook
        }));
        assert!(entries.iter().any(|entry| {
            entry.value == "done"
                && entry.fresh
                && entry.source == AgentStatusCacheSource::RuntimeWrapper
        }));

        fs::remove_dir_all(root).unwrap();
    }
}
