use super::command::{CommandMode, CommandSpec};
use std::time::Duration;

const TMUX_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
use crate::models::{TaskWindowStatus, TmuxStatus};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmuxAdapter {
    program: String,
}

impl TmuxAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn attach_session(&self, session: &str) -> CommandSpec {
        let target = tmux_session_target(session);
        CommandSpec::new(&self.program, ["attach-session", "-t", &target])
            .with_mode(CommandMode::InheritStdio)
    }

    pub fn switch_client(&self, session: &str) -> CommandSpec {
        let target = tmux_session_target(session);
        CommandSpec::new(&self.program, ["switch-client", "-t", &target])
            .with_mode(CommandMode::InheritStdio)
    }

    pub fn new_detached_task_session(
        &self,
        session: &str,
        window: &str,
        path: &str,
    ) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["new-session", "-d", "-s", session, "-n", window, "-c", path],
        )
    }

    pub fn ensure_task_window(&self, session: &str, window: &str, path: &str) -> CommandSpec {
        let target = tmux_session_slot_target(session);
        CommandSpec::new(
            &self.program,
            ["new-window", "-t", &target, "-n", window, "-c", path],
        )
    }

    pub fn kill_window(&self, session: &str, window: &str) -> CommandSpec {
        let target = tmux_window_target(session, window);
        CommandSpec::new(&self.program, ["kill-window", "-t", &target])
    }

    pub fn select_window(&self, session: &str, window: &str) -> CommandSpec {
        let target = tmux_window_target(session, window);
        CommandSpec::new(&self.program, ["select-window", "-t", &target])
    }

    pub fn attach_window(&self, session: &str, _window: &str) -> CommandSpec {
        self.attach_session(session)
    }

    pub fn switch_client_to_window(&self, session: &str, _window: &str) -> CommandSpec {
        self.switch_client(session)
    }

    pub fn send_agent_command(&self, session: &str, window: &str, command: &str) -> CommandSpec {
        let target = tmux_window_target(session, window);
        CommandSpec::new(
            &self.program,
            ["send-keys", "-t", &target, command, "Enter"],
        )
    }

    pub fn kill_session(&self, session: &str) -> CommandSpec {
        let target = tmux_session_target(session);
        CommandSpec::new(&self.program, ["kill-session", "-t", &target])
    }

    pub fn list_sessions(&self) -> CommandSpec {
        CommandSpec::new(&self.program, ["list-sessions", "-F", "#{session_name}"])
            .with_timeout(TMUX_PROBE_TIMEOUT)
    }

    pub fn list_windows(&self, session: &str) -> CommandSpec {
        let target = tmux_session_target(session);
        CommandSpec::new(
            &self.program,
            [
                "list-windows",
                "-t",
                &target,
                "-F",
                "#{window_name}\t#{pane_current_path}",
            ],
        )
    }

    pub fn list_all_windows(&self) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "list-windows",
                "-a",
                "-F",
                "#{session_name}\t#{window_name}\t#{pane_current_path}",
            ],
        )
        .with_timeout(TMUX_PROBE_TIMEOUT)
    }

    pub fn list_all_panes(&self) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "list-panes",
                "-a",
                "-F",
                "#{session_name}\t#{window_name}\t#{pane_current_path}",
            ],
        )
        .with_timeout(TMUX_PROBE_TIMEOUT)
    }

    pub fn capture_pane(&self, session: &str, window: &str) -> CommandSpec {
        let target = tmux_window_target(session, window);
        CommandSpec::new(&self.program, ["capture-pane", "-p", "-t", &target])
            .with_timeout(TMUX_PROBE_TIMEOUT)
    }

    pub fn parse_session_status(session: &str, list_sessions_output: &str) -> TmuxStatus {
        TmuxStatus {
            exists: list_sessions_output
                .lines()
                .map(str::trim)
                .any(|line| line == session),
            session_name: session.to_string(),
        }
    }

    pub fn parse_task_window_status(
        window: &str,
        expected_path: &str,
        list_windows_output: &str,
    ) -> TaskWindowStatus {
        let mut status = TaskWindowStatus::missing(window, "");

        for line in list_windows_output.lines() {
            let Some((window_name, current_path)) = line.split_once('\t') else {
                continue;
            };

            if window_name == window {
                status.exists = true;
                status.current_path = current_path.into();
                status.points_at_expected_path = current_path == expected_path;
                break;
            }
        }

        status
    }

    pub fn parse_task_window_status_for_session(
        session: &str,
        window: &str,
        expected_path: &str,
        list_all_windows_output: &str,
    ) -> TaskWindowStatus {
        let mut status = TaskWindowStatus::missing(window, "");

        for line in list_all_windows_output.lines() {
            let mut fields = line.splitn(3, '\t');
            let Some(session_name) = fields.next() else {
                continue;
            };
            let Some(window_name) = fields.next() else {
                continue;
            };
            let Some(current_path) = fields.next() else {
                continue;
            };

            if session_name == session && window_name == window {
                status.exists = true;
                status.current_path = current_path.into();
                status.points_at_expected_path = current_path == expected_path;
                break;
            }
        }

        if !status.exists {
            status = Self::parse_task_window_status(window, expected_path, list_all_windows_output);
        }

        status
    }
}

pub fn tmux_session_target(session: &str) -> String {
    format!("={session}")
}

pub fn tmux_session_slot_target(session: &str) -> String {
    format!("={session}:")
}

pub fn tmux_window_target(session: &str, window: &str) -> String {
    format!("={session}:={window}")
}

#[cfg(test)]
mod tests {
    use super::TmuxAdapter;

    #[test]
    fn session_and_window_commands_use_exact_match_targets() {
        let adapter = TmuxAdapter::new("tmux");

        assert_eq!(
            adapter.attach_session("ajax-web-fix").args,
            ["attach-session", "-t", "=ajax-web-fix"]
        );
        assert_eq!(
            adapter.switch_client("ajax-web-fix").args,
            ["switch-client", "-t", "=ajax-web-fix"]
        );
        assert_eq!(
            adapter.switch_client_to_window("ajax-web-fix", "task").args,
            ["switch-client", "-t", "=ajax-web-fix"]
        );
        assert_eq!(
            adapter
                .ensure_task_window("ajax-web-fix", "task", "/tmp/wt")
                .args,
            [
                "new-window",
                "-t",
                "=ajax-web-fix:",
                "-n",
                "task",
                "-c",
                "/tmp/wt"
            ]
        );
        assert_eq!(
            adapter.kill_window("ajax-web-fix", "task").args,
            ["kill-window", "-t", "=ajax-web-fix:=task"]
        );
        assert_eq!(
            adapter.select_window("ajax-web-fix", "task").args,
            ["select-window", "-t", "=ajax-web-fix:=task"]
        );
        assert_eq!(
            adapter
                .send_agent_command("ajax-web-fix", "task", "codex")
                .args,
            ["send-keys", "-t", "=ajax-web-fix:=task", "codex", "Enter"]
        );
        assert_eq!(
            adapter.capture_pane("ajax-web-fix", "task").args,
            ["capture-pane", "-p", "-t", "=ajax-web-fix:=task"]
        );
        assert_eq!(
            adapter.kill_session("ajax-web-fix").args,
            ["kill-session", "-t", "=ajax-web-fix"]
        );
        assert_eq!(
            adapter.list_windows("ajax-web-fix").args,
            [
                "list-windows",
                "-t",
                "=ajax-web-fix",
                "-F",
                "#{window_name}\t#{pane_current_path}"
            ]
        );
    }

    #[test]
    fn session_status_matches_trimmed_session_name_exactly() {
        let status = TmuxAdapter::parse_session_status(
            "ajax-web-fix",
            " ajax-web-fix \najax-web-fix-extra\nother\n",
        );
        let missing = TmuxAdapter::parse_session_status("ajax-web", "ajax-web-fix\nother\n");

        assert!(status.exists);
        assert_eq!(status.session_name, "ajax-web-fix");
        assert!(!missing.exists);
        assert_eq!(missing.session_name, "ajax-web");
    }
}
