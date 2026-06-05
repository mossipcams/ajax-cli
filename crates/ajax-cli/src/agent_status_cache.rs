use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use ajax_core::runtime_refresh::AgentStatusCache;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct TmuxAgentStatusSnapshot {
    by_session: BTreeMap<String, Vec<String>>,
}

impl TmuxAgentStatusSnapshot {
    pub(crate) fn from_root(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        let mut by_session: BTreeMap<String, Vec<String>> = BTreeMap::new();

        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                    continue;
                };
                let Some(session) = file_name.strip_suffix(".status") else {
                    continue;
                };
                if let Some(value) = read_status_value(&path) {
                    by_session
                        .entry(session.to_string())
                        .or_default()
                        .push(value);
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
                if let Some(value) = read_status_value(&path) {
                    by_session
                        .entry(session.to_string())
                        .or_default()
                        .push(value);
                }
            }
        }

        Self { by_session }
    }

    pub(crate) fn from_default_location() -> Option<Self> {
        let home = std::env::var_os("HOME")?;
        Some(Self::from_root(
            PathBuf::from(home).join(".cache/tmux-agent-status"),
        ))
    }
}

impl AgentStatusCache for TmuxAgentStatusSnapshot {
    fn status_values_for_session(&self, session: &str) -> Vec<String> {
        self.by_session.get(session).cloned().unwrap_or_default()
    }
}

fn read_status_value(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use ajax_core::runtime_refresh::AgentStatusCache;

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

        let mut values = cache.status_values_for_session("ajax-web-fix-login");
        values.sort();

        assert_eq!(values, vec!["done", "wait", "working"]);
        assert_eq!(cache.by_session.len(), 2);

        fs::remove_dir_all(root).unwrap();
    }
}
