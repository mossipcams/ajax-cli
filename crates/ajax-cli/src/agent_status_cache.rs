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
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use ajax_core::{
        models::TaskId,
        runtime_refresh::{AgentStatusCache, AgentStatusCacheEntry, AgentStatusCacheSource},
    };

    use super::TmuxAgentStatusSnapshot;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_cache_root() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "ajax-agent-status-cache-{}-{suffix}-{counter}",
            std::process::id()
        ))
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
        // The adapter must not choose a winner between hook files: it preserves
        // both entries with their exact values and timestamps, and core decides
        // which observation applies.
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
        let entries = cache.status_entries_for_session("ajax-web-fix-login");

        assert!(entries.iter().any(|entry| entry.value == "working"
            && entry.observed_at == now - Duration::from_secs(10)
            && entry.source == AgentStatusCacheSource::Hook));
        assert!(entries.iter().any(|entry| entry.value == "done"
            && entry.observed_at == now - Duration::from_secs(2)
            && entry.source == AgentStatusCacheSource::Hook));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_snapshot_retains_entry_past_legacy_thirty_second_window() {
        let root = temp_cache_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("ajax-web-fix-login.status");
        fs::write(&path, "working\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let observed_at = now - Duration::from_secs(119);
        set_modified(&path, observed_at);

        let cache = TmuxAgentStatusSnapshot::from_root_at(&root, now, Duration::from_secs(30));
        let entries = cache.status_entries_for_session("ajax-web-fix-login");

        let entry = entries
            .iter()
            .find(|entry| entry.value == "working")
            .expect("working entry retained past 30s window");
        assert_eq!(entry.observed_at, observed_at);
        assert_eq!(entry.source, AgentStatusCacheSource::Hook);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_snapshot_retains_stale_entry_for_core_fallback_decision() {
        let root = temp_cache_root();
        fs::create_dir_all(&root).unwrap();
        let path = root.join("ajax-web-fix-login.status");
        fs::write(&path, "wait\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let observed_at = now - Duration::from_secs(121);
        set_modified(&path, observed_at);

        let cache = TmuxAgentStatusSnapshot::from_root_at(&root, now, Duration::from_secs(30));
        let entries = cache.status_entries_for_session("ajax-web-fix-login");

        assert!(entries
            .iter()
            .any(|entry| entry.value == "wait" && entry.observed_at == observed_at));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn runtime_snapshot_keeps_old_terminal_exit_but_expires_old_running_heartbeat() {
        let root = temp_cache_root();
        let tmux_root = root.join("tmux-agent-status");
        let runtime_root = root.join("agent-runtime");
        fs::create_dir_all(&tmux_root).unwrap();
        fs::create_dir_all(&runtime_root).unwrap();
        // Both observed ten minutes ago.
        let observed_millis = (1_000 - 600) * 1_000;
        fs::write(
            runtime_root.join("web__done.json"),
            format!(
                r#"{{"task_id":"web/done","state":"exited_success","observed_at_unix_millis":{observed_millis},"pid":42,"exit_code":0,"message":null}}"#
            ),
        )
        .unwrap();
        fs::write(
            runtime_root.join("web__running.json"),
            format!(
                r#"{{"task_id":"web/running","state":"running","observed_at_unix_millis":{observed_millis},"pid":43,"exit_code":null,"message":null}}"#
            ),
        )
        .unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);

        let cache = TmuxAgentStatusSnapshot::from_roots_at(
            &tmux_root,
            &runtime_root,
            now,
            Duration::from_secs(30),
        );

        let terminal = cache.status_entries_for_task(&TaskId::new("web/done"), "ajax-web-done");
        assert!(terminal.iter().any(|entry| entry.value == "done"
            && entry.fresh
            && entry.source == AgentStatusCacheSource::RuntimeWrapper));

        let running =
            cache.status_entries_for_task(&TaskId::new("web/running"), "ajax-web-running");
        assert!(running.iter().any(|entry| entry.value == "working"
            && !entry.fresh
            && entry.source == AgentStatusCacheSource::RuntimeWrapper));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merged_snapshot_preserves_source_and_timestamp_for_each_entry() {
        let root = temp_cache_root();
        let tmux_root = root.join("tmux-agent-status");
        let runtime_root = root.join("agent-runtime");
        fs::create_dir_all(&tmux_root).unwrap();
        fs::create_dir_all(&runtime_root).unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        let hook = tmux_root.join("ajax-web-fix-login.status");
        fs::write(&hook, "wait\n").unwrap();
        let hook_observed = now - Duration::from_secs(50);
        set_modified(&hook, hook_observed);
        let runtime_observed_millis = (1_000 - 100) * 1_000;
        let runtime_observed = now - Duration::from_secs(100);
        fs::write(
            runtime_root.join("web__fix-login.json"),
            format!(
                r#"{{"task_id":"web/fix-login","state":"exited_success","observed_at_unix_millis":{runtime_observed_millis},"pid":42,"exit_code":0,"message":null}}"#
            ),
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

        assert!(entries.iter().any(|entry| entry.value == "wait"
            && entry.observed_at == hook_observed
            && entry.source == AgentStatusCacheSource::Hook));
        assert!(entries.iter().any(|entry| entry.value == "done"
            && entry.observed_at == runtime_observed
            && entry.source == AgentStatusCacheSource::RuntimeWrapper));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn snapshot_does_not_choose_a_winner_between_hook_files() {
        let root = temp_cache_root();
        let pane_dir = root.join("panes");
        fs::create_dir_all(&pane_dir).unwrap();
        let session = root.join("ajax-web-fix-login.status");
        let pane = pane_dir.join("ajax-web-fix-login_%1.status");
        fs::write(&session, "working\n").unwrap();
        fs::write(&pane, "wait\n").unwrap();
        let now = UNIX_EPOCH + Duration::from_secs(1_000);
        set_modified(&session, now - Duration::from_secs(5));
        set_modified(&pane, now - Duration::from_secs(3));

        let cache = TmuxAgentStatusSnapshot::from_root_at(&root, now, Duration::from_secs(30));
        let mut values = cache
            .status_entries_for_session("ajax-web-fix-login")
            .into_iter()
            .map(|entry| entry.value)
            .collect::<Vec<_>>();
        values.sort();

        assert_eq!(values, vec!["wait", "working"]);

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
