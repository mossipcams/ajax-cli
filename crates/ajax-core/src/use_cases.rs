use crate::{
    adapters::{CommandRunError, CommandSpec},
    registry::RegistryError,
};
use serde::{Deserialize, Serialize};

pub struct CommandContext<R> {
    pub config: crate::config::Config,
    pub registry: R,
}

impl<R> CommandContext<R> {
    pub fn new(config: crate::config::Config, registry: R) -> Self {
        Self { config, registry }
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
