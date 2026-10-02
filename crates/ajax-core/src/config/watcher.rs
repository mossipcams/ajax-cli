//! Optional local progress judge. Unset command keeps deterministic supervision.
use serde::{Deserialize, Deserializer, Serialize};

/// A whitespace-separated command (like STT), or argv for paths with spaces.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum LayaCommand {
    String(String),
    Argv(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct WatcherConfig {
    pub enabled: bool,
    pub laya_command: Option<LayaCommand>,
    #[serde(deserialize_with = "clamped_timeout")]
    pub judge_timeout_ms: u64,
}

fn clamped_timeout<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    Ok(u64::deserialize(deserializer)?.clamp(200, 10_000))
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            laya_command: None,
            judge_timeout_ms: 2_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn watcher_defaults_and_unknown_fields() {
        for source in ["", "[watcher]", "[watcher]\nfuture_setting = true"] {
            assert_eq!(
                Config::from_toml_str(source).unwrap().watcher,
                WatcherConfig::default()
            );
        }
    }

    #[test]
    fn watcher_disabled_and_commands() {
        let config = Config::from_toml_str(
            "[watcher]\nenabled = false\nlaya_command = 'python3 scripts/ajax-laya-sidecar'",
        )
        .unwrap();
        assert!(!config.watcher.enabled);
        assert_eq!(
            config.watcher.laya_command,
            Some(LayaCommand::String(
                "python3 scripts/ajax-laya-sidecar".into()
            ))
        );
        let config = Config::from_toml_str(
            "[watcher]\nlaya_command = ['python3', '/path with spaces/sidecar']",
        )
        .unwrap();
        assert_eq!(
            config.watcher.laya_command,
            Some(LayaCommand::Argv(vec![
                "python3".into(),
                "/path with spaces/sidecar".into()
            ]))
        );
    }

    #[test]
    fn watcher_timeout_is_clamped() {
        for (input, expected) in [(0, 200), (199, 200), (850, 850), (10_001, 10_000)] {
            let config =
                Config::from_toml_str(&format!("[watcher]\njudge_timeout_ms = {input}")).unwrap();
            assert_eq!(config.watcher.judge_timeout_ms, expected);
        }
    }
}
