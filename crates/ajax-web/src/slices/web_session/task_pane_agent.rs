//! Whether the task tmux pane is running the harness agent (vs a shell).

use ajax_core::{
    adapters::{acp_launch_for_agent, CommandRunner, CommandSpec},
    models::{AgentClient, Task},
};
use std::path::Path;

const SHELL_COMMANDS: &[&str] = &["zsh", "bash", "fish", "sh"];

pub(crate) fn tmux_task_pane_runs_live_agent(runner: &mut impl CommandRunner, task: &Task) -> bool {
    let Some(expected) = live_pane_basenames(task.selected_agent) else {
        return false;
    };
    let Ok(foreground) = tmux_pane_current_command(runner, &task.tmux_session, &task.task_window)
    else {
        return false;
    };
    let observed = process_basename(&foreground);
    if observed.is_empty() || is_shell_command(observed) {
        return false;
    }
    expected.iter().any(|name| *name == observed)
}

fn live_pane_basenames(agent: AgentClient) -> Option<&'static [&'static str]> {
    if acp_launch_for_agent(agent).is_none() {
        return None;
    }
    Some(match agent {
        AgentClient::Cursor => &["agent", "cursor"],
        AgentClient::Codex => &["codex"],
        AgentClient::Claude => &["claude"],
        AgentClient::Pi => &["pi"],
        AgentClient::Other => return None,
    })
}

fn tmux_pane_current_command(
    runner: &mut impl CommandRunner,
    session: &str,
    window: &str,
) -> Result<String, ()> {
    let target = format!("{session}:{window}");
    let output = runner
        .run(&CommandSpec::new(
            "tmux",
            [
                "display-message",
                "-p",
                "-t",
                &target,
                "#{pane_current_command}",
            ],
        ))
        .map_err(|_| ())?;
    if output.status_code != 0 {
        return Err(());
    }
    Ok(output.stdout)
}

fn process_basename(value: &str) -> &str {
    Path::new(value.trim())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
}

fn is_shell_command(name: &str) -> bool {
    SHELL_COMMANDS.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::adapters::{CommandOutput, CommandRunError, CommandSpec};

    struct PaneRunner {
        stdout: String,
        status: i32,
    }

    impl CommandRunner for PaneRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            assert_eq!(command.program, "tmux");
            Ok(CommandOutput {
                status_code: self.status,
                stdout: self.stdout.clone(),
                stderr: String::new(),
            })
        }
    }

    fn codex_task() -> Task {
        crate::test_support::fix_login_task()
    }

    #[test]
    fn shell_pane_is_not_a_live_agent_issue_1092() {
        let mut runner = PaneRunner {
            stdout: "zsh\n".to_string(),
            status: 0,
        };
        assert!(!tmux_task_pane_runs_live_agent(&mut runner, &codex_task()));
    }

    #[test]
    fn matching_harness_pane_is_live_issue_1092() {
        let mut runner = PaneRunner {
            stdout: "codex\n".to_string(),
            status: 0,
        };
        assert!(tmux_task_pane_runs_live_agent(&mut runner, &codex_task()));
    }

    #[test]
    fn cursor_agent_pane_is_live_issue_1092() {
        let mut task = codex_task();
        task.selected_agent = AgentClient::Cursor;
        let mut runner = PaneRunner {
            stdout: "agent\n".to_string(),
            status: 0,
        };
        assert!(tmux_task_pane_runs_live_agent(&mut runner, &task));
    }
}
