use crate::{
    adapters::{CommandRunError, CommandSpec},
    registry::RegistryError,
};
use serde::{Deserialize, Serialize};

pub struct CommandContext<R> {
    pub config: crate::config::Config,
    pub registry: R,
    pub runtime_paths: crate::config::RuntimePaths,
}

impl<R> CommandContext<R> {
    pub fn new(config: crate::config::Config, registry: R) -> Self {
        Self {
            config,
            registry,
            runtime_paths: crate::config::RuntimePathRequest::new("").resolve(),
        }
    }

    pub fn with_runtime_paths(
        config: crate::config::Config,
        registry: R,
        runtime_paths: crate::config::RuntimePaths,
    ) -> Self {
        Self {
            config,
            registry,
            runtime_paths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CommandContext;
    use crate::{
        config::{Config, RuntimePathRequest, WorktreePlacement},
        registry::InMemoryRegistry,
    };
    use std::path::Path;

    #[test]
    fn command_context_carries_runtime_paths_for_task_planning() {
        let runtime_paths = RuntimePathRequest::new("/Users/matt")
            .with_cli_profile("dev")
            .resolve();
        let context = CommandContext::with_runtime_paths(
            Config::default(),
            InMemoryRegistry::default(),
            runtime_paths,
        );

        assert_eq!(context.runtime_paths.profile, "dev");
        assert_eq!(
            context.runtime_paths.worktree_placement,
            WorktreePlacement::Root(Path::new("/Users/matt/.ajax-dev/worktrees").to_path_buf())
        );
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandError {
    TaskNotFound(String),
    RepoNotFound(String),
    ConfirmationRequired,
    PlanBlocked(Vec<String>),
    CommandRun(CommandRunError),
    Registry(RegistryError),
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CommandPlan {
    pub title: String,
    pub commands: Vec<CommandSpec>,
    pub requires_confirmation: bool,
    pub blocked_reasons: Vec<String>,
}

impl CommandPlan {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            commands: Vec::new(),
            requires_confirmation: false,
            blocked_reasons: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpenMode {
    Attach,
    SwitchClient,
}
