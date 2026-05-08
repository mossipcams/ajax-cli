use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigPaths {
    pub config_file: PathBuf,
    pub state_db: PathBuf,
    pub logs_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl ConfigPaths {
    pub fn for_home(home: impl Into<PathBuf>) -> Self {
        let home = home.into();

        Self {
            config_file: home.join(".config/ajax/config.toml"),
            state_db: home.join(".local/state/ajax/ajax.db"),
            logs_dir: home.join(".local/state/ajax/logs"),
            cache_dir: home.join(".cache/ajax"),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<ManagedRepo>,
    #[serde(default)]
    pub launchers: Vec<LauncherDefinition>,
    #[serde(default)]
    pub cleanup: CleanupRules,
    #[serde(default)]
    pub test_commands: Vec<TestCommand>,
}

impl Config {
    pub fn from_toml_str(input: &str) -> Result<Self, ConfigParseError> {
        toml::from_str(input).map_err(|error| ConfigParseError::Toml(error.to_string()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigParseError {
    Toml(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct ManagedRepo {
    pub name: String,
    pub path: PathBuf,
    pub default_branch: String,
}

impl ManagedRepo {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        default_branch: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            default_branch: default_branch.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LauncherDefinition {
    pub name: String,
    pub command: String,
}

impl LauncherDefinition {
    pub fn new(name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            command: command.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CleanupRules {
    pub require_clean_worktree: bool,
    pub require_merged_branch: bool,
    pub require_no_unpushed_commits: bool,
}

impl Default for CleanupRules {
    fn default() -> Self {
        Self {
            require_clean_worktree: true,
            require_merged_branch: true,
            require_no_unpushed_commits: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TestCommand {
    pub repo: String,
    pub command: String,
}

impl TestCommand {
    pub fn new(repo: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            command: command.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CleanupRules, Config, ConfigPaths, LauncherDefinition, ManagedRepo, TestCommand};
    use std::path::Path;

    #[test]
    fn default_paths_live_outside_source_repo() {
        let source_repo = Path::new("/Users/matt/projects/ajax-cli");
        let paths = ConfigPaths::for_home("/Users/matt");

        assert_eq!(
            paths.config_file,
            Path::new("/Users/matt/.config/ajax/config.toml")
        );
        assert_eq!(
            paths.state_db,
            Path::new("/Users/matt/.local/state/ajax/ajax.db")
        );
        assert_eq!(
            paths.logs_dir,
            Path::new("/Users/matt/.local/state/ajax/logs")
        );
        assert_eq!(paths.cache_dir, Path::new("/Users/matt/.cache/ajax"));
        assert!(!paths.config_file.starts_with(source_repo));
        assert!(!paths.state_db.starts_with(source_repo));
        assert!(!paths.logs_dir.starts_with(source_repo));
        assert!(!paths.cache_dir.starts_with(source_repo));
    }

    #[test]
    fn config_tracks_repos_launchers_cleanup_and_tests() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            launchers: vec![LauncherDefinition::new("codex", "codex")],
            cleanup: CleanupRules {
                require_clean_worktree: true,
                require_merged_branch: true,
                require_no_unpushed_commits: true,
            },
            test_commands: vec![TestCommand::new("web", "cargo test")],
        };

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.launchers[0].name, "codex");
        assert!(config.cleanup.require_clean_worktree);
        assert_eq!(config.test_commands[0].command, "cargo test");
    }

    #[test]
    fn config_loads_from_documented_toml_shape() {
        let config = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [[launchers]]
            name = "codex"
            command = "codex"

            [cleanup]
            require_clean_worktree = true
            require_merged_branch = true
            require_no_unpushed_commits = true

            [[test_commands]]
            repo = "web"
            command = "cargo test"
            "#,
        )
        .unwrap();

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.launchers[0].command, "codex");
        assert_eq!(config.test_commands[0].repo, "web");
    }
}
