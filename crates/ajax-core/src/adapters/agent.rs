use super::command::CommandSpec;
use crate::models::AgentClient;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLaunch {
    pub worktree_path: String,
    pub prompt: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentAdapter {
    program: String,
}

impl AgentAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn launch(&self, client: AgentClient, launch: &AgentLaunch) -> CommandSpec {
        let mut args = match client {
            AgentClient::Codex => {
                vec!["--cd".to_string(), launch.worktree_path.clone()]
            }
            AgentClient::Claude => Vec::new(),
            AgentClient::Other if self.program == "cursor" => vec!["agent".to_string()],
            AgentClient::Other => Vec::new(),
        };
        if !launch.prompt.is_empty() {
            args.push(launch.prompt.clone());
        }
        CommandSpec {
            program: self.program.clone(),
            args,
            cwd: None,
            mode: super::command::CommandMode::Capture,
            timeout: None,
        }
    }
}
