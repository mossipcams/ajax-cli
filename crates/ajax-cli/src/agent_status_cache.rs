use std::{
    fs,
    path::{Path, PathBuf},
};

use ajax_core::runtime_refresh::AgentStatusCache;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TmuxAgentStatusCache {
    root: PathBuf,
}

impl TmuxAgentStatusCache {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn from_default_location() -> Option<Self> {
        let home = std::env::var_os("HOME")?;
        Some(Self::new(
            PathBuf::from(home).join(".cache/tmux-agent-status"),
        ))
    }
}

impl AgentStatusCache for TmuxAgentStatusCache {
    fn status_values_for_session(&self, session: &str) -> Vec<String> {
        let mut values = Vec::new();

        if let Some(value) = read_status_value(&self.root.join(format!("{session}.status"))) {
            values.push(value);
        }

        let pane_dir = self.root.join("panes");
        let Ok(entries) = fs::read_dir(pane_dir) else {
            return values;
        };
        let pane_prefix = format!("{session}_");

        for entry in entries.flatten() {
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !file_name.starts_with(&pane_prefix) || !file_name.ends_with(".status") {
                continue;
            }
            if let Some(value) = read_status_value(&path) {
                values.push(value);
            }
        }

        values
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

    use super::TmuxAgentStatusCache;

    fn temp_cache_root() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("ajax-agent-status-cache-{suffix}"))
    }

    #[test]
    fn tmux_agent_status_cache_reads_session_and_pane_values() {
        let root = temp_cache_root();
        let pane_dir = root.join("panes");
        fs::create_dir_all(&pane_dir).unwrap();
        fs::write(root.join("ajax-web-fix-login.status"), "done\n").unwrap();
        fs::write(pane_dir.join("ajax-web-fix-login_%1.status"), "working\n").unwrap();
        fs::write(pane_dir.join("ajax-web-fix-login_%2.status"), "wait\n").unwrap();
        fs::write(pane_dir.join("ajax-other_%3.status"), "working\n").unwrap();
        let cache = TmuxAgentStatusCache::new(root.clone());

        let mut values = cache.status_values_for_session("ajax-web-fix-login");
        values.sort();

        assert_eq!(values, vec!["done", "wait", "working"]);

        fs::remove_dir_all(root).unwrap();
    }
}
