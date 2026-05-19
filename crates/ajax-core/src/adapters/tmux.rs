use super::command::{CommandMode, CommandSpec};
use crate::models::{TmuxStatus, WorktrunkStatus};

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
        CommandSpec::new(&self.program, ["attach-session", "-t", session])
            .with_mode(CommandMode::InheritStdio)
    }

    pub fn switch_client(&self, session: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["switch-client", "-t", session])
            .with_mode(CommandMode::InheritStdio)
    }

    pub fn new_detached_worktrunk_session(
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

    pub fn ensure_worktrunk(&self, session: &str, window: &str, path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["new-window", "-t", session, "-n", window, "-c", path],
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
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "send-keys".to_string(),
                "-t".to_string(),
                target,
                command.to_string(),
                "Enter".to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn kill_session(&self, session: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["kill-session", "-t", session])
    }

    pub fn list_sessions(&self) -> CommandSpec {
        CommandSpec::new(&self.program, ["list-sessions", "-F", "#{session_name}"])
    }

    pub fn list_windows(&self, session: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "list-windows",
                "-t",
                session,
                "-F",
                "#{window_name}\t#{pane_current_path}",
            ],
        )
    }

    pub fn capture_pane(&self, session: &str, window: &str) -> CommandSpec {
        let target = format!("{session}:{window}");
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "capture-pane".to_string(),
                "-p".to_string(),
                "-t".to_string(),
                target,
                "-S".to_string(),
                "-200".to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
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

    pub fn parse_worktrunk_status(
        window: &str,
        expected_path: &str,
        list_windows_output: &str,
    ) -> WorktrunkStatus {
        let mut status = WorktrunkStatus {
            exists: false,
            window_name: window.to_string(),
            current_path: String::new().into(),
            points_at_expected_path: false,
        };

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
}

fn tmux_window_target(session: &str, window: &str) -> String {
    format!("{session}:{window}")
}

#[cfg(test)]
mod tests {
    use super::TmuxAdapter;

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
