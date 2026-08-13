use super::command::CommandSpec;
use crate::models::AgentClient;

/// Default Cursor Agent model for Ajax-started tasks (not Fast).
pub const CURSOR_DEFAULT_MODEL: &str = "cursor-grok-4.6-high";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentLaunch {
    pub worktree_path: String,
    pub prompt: String,
}

pub fn agent_launch_spec(
    program: impl Into<String>,
    client: AgentClient,
    launch: &AgentLaunch,
) -> CommandSpec {
    let program = program.into();
    let mut args = match client {
        AgentClient::Codex => {
            vec!["--cd".to_string(), launch.worktree_path.clone()]
        }
        AgentClient::Claude => vec!["--dangerously-skip-permissions".to_string()],
        AgentClient::Cursor if program == "cursor" => vec![
            "agent".to_string(),
            "--model".to_string(),
            CURSOR_DEFAULT_MODEL.to_string(),
        ],
        AgentClient::Other if program == "cursor" => vec![
            "agent".to_string(),
            "--model".to_string(),
            CURSOR_DEFAULT_MODEL.to_string(),
        ],
        AgentClient::Cursor | AgentClient::Pi | AgentClient::Other => Vec::new(),
    };
    if !launch.prompt.is_empty() {
        args.push(launch.prompt.clone());
    }
    CommandSpec {
        program,
        args,
        cwd: None,
        mode: super::command::CommandMode::Capture,
        timeout: None,
    }
}
