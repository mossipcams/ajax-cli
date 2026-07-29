use super::command::{CommandMode, CommandSpec};
use crate::models::AgentClient;
use std::path::Path;

pub fn agent_acp_launch_spec(
    task_id: &str,
    state_root: &Path,
    program: &str,
    client: AgentClient,
) -> CommandSpec {
    let (adapter, adapter_args): (&str, &[&str]) = match client {
        AgentClient::Codex => ("codex-acp", &[]),
        AgentClient::Claude => ("claude-agent-acp", &[]),
        AgentClient::Cursor => ("cursor-agent", &["acp"]),
        AgentClient::Pi => ("pi-acp", &[]),
        AgentClient::Other => (program, &[]),
    };
    let mut args = vec![
        "__agent-acp".to_string(),
        "--task-id".to_string(),
        task_id.to_string(),
        "--state-root".to_string(),
        state_root.display().to_string(),
        adapter.to_string(),
    ];
    args.extend(adapter_args.iter().copied().map(str::to_string));
    CommandSpec {
        program: "ajax-cli".to_string(),
        args,
        cwd: None,
        mode: CommandMode::Capture,
        timeout: None,
    }
}
