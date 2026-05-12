use std::{error::Error, fmt, path::PathBuf};

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
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub repos: Vec<ManagedRepo>,
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

impl fmt::Display for ConfigParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Toml(message) => write!(formatter, "toml parse error: {message}"),
        }
    }
}

impl Error for ConfigParseError {}

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
    use super::{Config, ConfigParseError, ConfigPaths, ManagedRepo, TestCommand};
    use proptest::prelude::*;
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
    fn config_tracks_repos_and_tests() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
            test_commands: vec![TestCommand::new("web", "cargo test")],
        };

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.test_commands[0].command, "cargo test");
    }

    proptest! {
        #[test]
        fn constructors_preserve_input_values(
            repo_name in "\\PC*",
            repo_path in "\\PC*",
            default_branch in "\\PC*",
            test_repo in "\\PC*",
            test_command in "\\PC*",
        ) {
            let repo = ManagedRepo::new(&repo_name, &repo_path, &default_branch);
            prop_assert_eq!(repo.name, repo_name);
            prop_assert_eq!(repo.path, Path::new(&repo_path));
            prop_assert_eq!(repo.default_branch, default_branch);

            let test_command_value = TestCommand::new(&test_repo, &test_command);
            prop_assert_eq!(test_command_value.repo, test_repo);
            prop_assert_eq!(test_command_value.command, test_command);
        }
    }

    #[test]
    fn config_loads_from_documented_toml_shape() {
        let config = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [[test_commands]]
            repo = "web"
            command = "cargo test"
            "#,
        )
        .unwrap();

        assert_eq!(config.repos[0].name, "web");
        assert_eq!(config.test_commands[0].repo, "web");
    }

    #[test]
    fn config_rejects_undocumented_launcher_sections() {
        let error = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [[launchers]]
            name = "codex"
            command = "codex"
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `launchers`"));
    }

    #[test]
    fn config_rejects_undocumented_cleanup_sections() {
        let error = Config::from_toml_str(
            r#"
            [[repos]]
            name = "web"
            path = "/Users/matt/projects/web"
            default_branch = "main"

            [cleanup]
            require_clean_worktree = true
            require_merged_branch = true
            require_no_unpushed_commits = true
            "#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `cleanup`"));
    }

    #[test]
    fn config_parse_errors_have_operator_facing_display() {
        assert_eq!(
            ConfigParseError::Toml("missing field".to_string()).to_string(),
            "toml parse error: missing field"
        );
    }
}
