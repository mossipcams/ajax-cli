use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};

pub fn render_event_log_line(event: &MonitorEvent) -> String {
    match event {
        MonitorEvent::Agent(AgentEvent::Started { agent }) => format!("agent started: {agent}"),
        MonitorEvent::Agent(AgentEvent::Thinking) => "agent thinking".to_string(),
        MonitorEvent::Agent(AgentEvent::ToolCall { name }) => format!("tool call: {name}"),
        MonitorEvent::Agent(AgentEvent::WaitingForApproval { command }) => command
            .as_ref()
            .map(|command| format!("waiting for approval: {command}"))
            .unwrap_or_else(|| "waiting for approval".to_string()),
        MonitorEvent::Agent(AgentEvent::WaitingForInput { prompt }) => {
            format!("waiting for input: {prompt}")
        }
        MonitorEvent::Agent(AgentEvent::Message { text }) => format!("agent: {text}"),
        MonitorEvent::Agent(AgentEvent::Completed) => "agent completed".to_string(),
        MonitorEvent::Agent(AgentEvent::Failed { message }) => format!("agent failed: {message}"),
        MonitorEvent::Repo(RepoEvent::FileChanged { path }) => {
            format!("repo changed: {}", path.display())
        }
        MonitorEvent::Repo(RepoEvent::GitSnapshot {
            worktree_path,
            status,
            diff_stat,
        }) => format!(
            "git snapshot: {} dirty:{} conflicted:{} ahead:{} behind:{} diff:{}",
            worktree_path.display(),
            status.dirty,
            status.conflicted,
            status.ahead,
            status.behind,
            diff_stat.lines().next().unwrap_or_default()
        ),
        MonitorEvent::Process(ProcessEvent::Started { pid }) => {
            format!(
                "process started: {}",
                pid.map_or("?".to_string(), |pid| pid.to_string())
            )
        }
        MonitorEvent::Process(ProcessEvent::Stdout { line }) => format!("stdout: {line}"),
        MonitorEvent::Process(ProcessEvent::Stderr { line }) => format!("stderr: {line}"),
        MonitorEvent::Process(ProcessEvent::Exited { code }) => {
            format!(
                "process exited: {}",
                code.map_or("?".to_string(), |code| code.to_string())
            )
        }
        MonitorEvent::Process(ProcessEvent::Hung { quiet_for }) => {
            format!("process hung: {}s quiet", quiet_for.as_secs())
        }
    }
}

#[cfg(test)]
mod tests {
    use ajax_core::events::{AgentEvent, MonitorEvent};

    use super::render_event_log_line;

    #[test]
    fn renderer_formats_monitor_events_for_logs() {
        assert_eq!(
            render_event_log_line(&MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string())
            })),
            "waiting for approval: cargo test"
        );
    }
}
