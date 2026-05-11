use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt,
    process::{Command, Stdio},
};

use crate::models::{GitStatus, TmuxStatus, WorktrunkStatus};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub mode: CommandMode,
}

impl CommandSpec {
    pub fn new<const N: usize>(program: impl Into<String>, args: [&str; N]) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(str::to_string).collect(),
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    pub fn with_mode(mut self, mode: CommandMode) -> Self {
        self.mode = mode;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum CommandMode {
    Capture,
    InheritStdio,
    Spawn,
}

pub trait CommandRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandOutput {
    pub status_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRunError {
    SpawnFailed(String),
    MissingStatusCode,
    NonZeroExit {
        program: String,
        status_code: i32,
        stderr: String,
        cwd: Option<String>,
    },
}

impl fmt::Display for CommandRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(message) => write!(formatter, "failed to start command: {message}"),
            Self::MissingStatusCode => write!(formatter, "command exited without a status code"),
            Self::NonZeroExit {
                program,
                status_code,
                stderr,
                cwd,
            } => {
                write!(formatter, "{program} exited with status {status_code}")?;
                if let Some(cwd) = cwd {
                    write!(formatter, " in {cwd}")?;
                }
                let stderr = stderr.trim();
                if !stderr.is_empty() {
                    write!(formatter, ": {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

impl Error for CommandRunError {}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecordingCommandRunner {
    commands: Vec<CommandSpec>,
}

impl RecordingCommandRunner {
    pub fn commands(&self) -> &[CommandSpec] {
        &self.commands
    }
}

impl CommandRunner for RecordingCommandRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());

        Ok(CommandOutput {
            status_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        if let Some(cwd) = &command.cwd {
            process.current_dir(cwd);
        }
        match command.mode {
            CommandMode::Capture => {
                let output = process
                    .output()
                    .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;
                let status_code = output
                    .status
                    .code()
                    .ok_or(CommandRunError::MissingStatusCode)?;

                Ok(CommandOutput {
                    status_code,
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                })
            }
            CommandMode::InheritStdio => {
                let status = process
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;
                let status_code = status.code().ok_or(CommandRunError::MissingStatusCode)?;

                Ok(CommandOutput {
                    status_code,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
            CommandMode::Spawn => {
                process
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;

                Ok(CommandOutput {
                    status_code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
    }
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitAdapter {
    program: String,
}

impl GitAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn status(&self, worktree_path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", worktree_path, "status", "--porcelain=v1", "--branch"],
        )
    }

    pub fn add_worktree(
        &self,
        repo_path: &str,
        worktree_path: &str,
        branch: &str,
        start_point: &str,
    ) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "add".to_string(),
                "-b".to_string(),
                branch.to_string(),
                worktree_path.to_string(),
                start_point.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "remove".to_string(),
                worktree_path.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn force_remove_worktree(&self, repo_path: &str, worktree_path: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "worktree".to_string(),
                "remove".to_string(),
                "--force".to_string(),
                worktree_path.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "branch".to_string(),
                "-d".to_string(),
                branch.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn force_delete_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "-C".to_string(),
                repo_path.to_string(),
                "branch".to_string(),
                "-D".to_string(),
                branch.to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }

    pub fn switch_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["-C", repo_path, "switch", branch])
    }

    pub fn merge_branch(&self, repo_path: &str, branch: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["-C", repo_path, "merge", "--ff-only", branch],
        )
    }

    pub fn merge_base_is_ancestor(
        &self,
        worktree_path: &str,
        ancestor: &str,
        descendant: &str,
    ) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            [
                "-C",
                worktree_path,
                "merge-base",
                "--is-ancestor",
                ancestor,
                descendant,
            ],
        )
    }

    pub fn parse_status(porcelain_branch_output: &str, merged: bool) -> GitStatus {
        let mut status = GitStatus {
            worktree_exists: true,
            branch_exists: false,
            current_branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        };

        for line in porcelain_branch_output.lines() {
            if let Some(branch_line) = line.strip_prefix("## ") {
                status.current_branch = parse_current_branch(branch_line);
                status.branch_exists =
                    !branch_line.starts_with("No commits yet") && status.current_branch.is_some();
                apply_branch_divergence(&mut status, branch_line);
                continue;
            }

            if line.starts_with("??") {
                status.dirty = true;
                status.untracked_files += 1;
                continue;
            }

            if line.len() >= 2 {
                status.dirty = true;
                let code = &line[..2];
                if matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU") {
                    status.conflicted = true;
                }
            }
        }

        status.unpushed_commits = status.ahead;
        status
    }
}

fn parse_current_branch(branch_line: &str) -> Option<String> {
    if branch_line.starts_with("No commits yet") || branch_line.starts_with("HEAD ") {
        return None;
    }

    let branch = branch_line
        .split_once("...")
        .map_or(branch_line, |(branch, _)| branch);
    let branch = branch.split_once(' ').map_or(branch, |(branch, _)| branch);

    (!branch.is_empty()).then(|| branch.to_string())
}

fn apply_branch_divergence(status: &mut GitStatus, branch_line: &str) {
    let Some(open_bracket) = branch_line.find('[') else {
        return;
    };
    let Some(close_bracket) = branch_line[open_bracket..].find(']') else {
        return;
    };
    let divergence = &branch_line[open_bracket + 1..open_bracket + close_bracket];

    for part in divergence.split(',').map(str::trim) {
        if let Some(ahead) = part.strip_prefix("ahead ") {
            if let Ok(value) = ahead.parse::<u32>() {
                status.ahead = value;
            }
        }
        if let Some(behind) = part.strip_prefix("behind ") {
            if let Ok(value) = behind.parse::<u32>() {
                status.behind = value;
            }
        }
    }
}

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

    pub fn launch(&self, launch: &AgentLaunch) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec![
                "--cd".to_string(),
                launch.worktree_path.clone(),
                launch.prompt.clone(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentAdapter, AgentLaunch, CommandMode, CommandRunner, CommandSpec, GitAdapter,
        RecordingCommandRunner, TmuxAdapter,
    };
    use crate::models::{TmuxStatus, WorktrunkStatus};
    use proptest::prelude::*;

    fn safe_token() -> impl Strategy<Value = String> {
        "[A-Za-z0-9_.-]{1,32}"
    }

    fn safe_path() -> impl Strategy<Value = String> {
        prop::collection::vec("[A-Za-z0-9_.-]{1,16}", 1..6)
            .prop_map(|segments| format!("/{}", segments.join("/")))
    }

    #[test]
    fn tmux_adapter_builds_attach_switch_and_worktrunk_commands() {
        let adapter = TmuxAdapter::new("tmux");

        assert_eq!(
            adapter.attach_session("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.switch_client("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.new_detached_worktrunk_session(
                "ajax-web-fix-login",
                "worktrunk",
                "/tmp/worktree"
            ),
            CommandSpec::new(
                "tmux",
                [
                    "new-session",
                    "-d",
                    "-s",
                    "ajax-web-fix-login",
                    "-n",
                    "worktrunk",
                    "-c",
                    "/tmp/worktree"
                ]
            )
        );
        assert_eq!(
            adapter.ensure_worktrunk("ajax-web-fix-login", "worktrunk", "/tmp/worktree"),
            CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "worktrunk",
                    "-c",
                    "/tmp/worktree"
                ]
            )
        );
        assert_eq!(
            adapter.kill_window("ajax-web-fix-login", "worktrunk"),
            CommandSpec::new(
                "tmux",
                ["kill-window", "-t", "ajax-web-fix-login:worktrunk"]
            )
        );
        assert_eq!(
            adapter.select_window("ajax-web-fix-login", "worktrunk"),
            CommandSpec::new(
                "tmux",
                ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
            )
        );
        assert_eq!(
            adapter.switch_client_to_window("ajax-web-fix-login", "worktrunk"),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.send_agent_command(
                "ajax-web-fix-login",
                "worktrunk",
                "codex --cd /tmp/worktree"
            ),
            CommandSpec::new(
                "tmux",
                [
                    "send-keys",
                    "-t",
                    "ajax-web-fix-login:worktrunk",
                    "codex --cd /tmp/worktree",
                    "Enter"
                ]
            )
        );
        assert_eq!(
            adapter.kill_session("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
        );
        assert_eq!(
            adapter.list_sessions(),
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
        );
        assert_eq!(
            adapter.list_windows("ajax-web-fix-login"),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-t",
                    "ajax-web-fix-login",
                    "-F",
                    "#{window_name}\t#{pane_current_path}"
                ]
            )
        );
        assert_eq!(
            adapter.capture_pane("ajax-web-fix-login", "worktrunk"),
            CommandSpec::new(
                "tmux",
                [
                    "capture-pane",
                    "-p",
                    "-t",
                    "ajax-web-fix-login:worktrunk",
                    "-S",
                    "-200"
                ]
            )
        );
    }

    proptest! {
        #[test]
        fn tmux_adapter_targets_generated_worktrunk_inputs(
            session in safe_token(),
            window in safe_token(),
            path in safe_path(),
            command in "[^\\x00]{0,80}"
        ) {
            let adapter = TmuxAdapter::new("tmux");
            let target = format!("{session}:{window}");

            prop_assert_eq!(
                adapter.new_detached_worktrunk_session(&session, &window, &path),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        session.as_str(),
                        "-n",
                        window.as_str(),
                        "-c",
                        path.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.ensure_worktrunk(&session, &window, &path),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-window",
                        "-t",
                        session.as_str(),
                        "-n",
                        window.as_str(),
                        "-c",
                        path.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.select_window(&session, &window).args,
                vec!["select-window", "-t", target.as_str()]
            );
            prop_assert_eq!(
                adapter.kill_window(&session, &window).args,
                vec!["kill-window", "-t", target.as_str()]
            );
            prop_assert_eq!(
                adapter.capture_pane(&session, &window).args,
                vec!["capture-pane", "-p", "-t", target.as_str(), "-S", "-200"]
            );
            prop_assert_eq!(
                adapter.send_agent_command(&session, &window, &command).args,
                vec!["send-keys", "-t", target.as_str(), command.as_str(), "Enter"]
            );
        }

        #[test]
        fn git_adapter_native_lifecycle_commands_preserve_generated_inputs(
            repo_path in safe_path(),
            worktree_path in safe_path(),
            branch_suffix in safe_token(),
            start_point in safe_token()
        ) {
            let adapter = GitAdapter::new("git");
            let branch = format!("ajax/{branch_suffix}");

            prop_assert_eq!(
                adapter.add_worktree(&repo_path, &worktree_path, &branch, &start_point),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        repo_path.as_str(),
                        "worktree",
                        "add",
                        "-b",
                        branch.as_str(),
                        worktree_path.as_str(),
                        start_point.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.remove_worktree(&repo_path, &worktree_path).args,
                vec!["-C", repo_path.as_str(), "worktree", "remove", worktree_path.as_str()]
            );
            prop_assert_eq!(
                adapter.force_remove_worktree(&repo_path, &worktree_path).args,
                vec![
                    "-C",
                    repo_path.as_str(),
                    "worktree",
                    "remove",
                    "--force",
                    worktree_path.as_str(),
                ]
            );
            prop_assert_eq!(
                adapter.delete_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "branch", "-d", branch.as_str()]
            );
            prop_assert_eq!(
                adapter.force_delete_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "branch", "-D", branch.as_str()]
            );
            prop_assert_eq!(
                adapter.switch_branch(&repo_path, &start_point).args,
                vec!["-C", repo_path.as_str(), "switch", start_point.as_str()]
            );
            prop_assert_eq!(
                adapter.merge_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "merge", "--ff-only", branch.as_str()]
            );
        }
    }

    #[test]
    fn git_adapter_builds_native_lifecycle_commands() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.add_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "ajax/fix-login",
                "main"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "-b",
                    "ajax/fix-login",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login",
                    "main"
                ]
            )
        );
        assert_eq!(
            adapter.remove_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.force_remove_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "--force",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.delete_branch("/Users/matt/projects/web", "ajax/fix-login"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.force_delete_branch("/Users/matt/projects/web", "ajax/fix-login"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-D",
                    "ajax/fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.switch_branch("/Users/matt/projects/web", "main"),
            CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"])
        );
        assert_eq!(
            adapter.merge_branch("/Users/matt/projects/web", "ajax/fix-login"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "merge",
                    "--ff-only",
                    "ajax/fix-login"
                ]
            )
        );
    }

    #[test]
    fn tmux_interactive_commands_inherit_stdio() {
        let adapter = TmuxAdapter::new("tmux");

        assert_eq!(
            adapter.attach_session("ajax-web-fix-login").mode,
            CommandMode::InheritStdio
        );
        assert_eq!(
            adapter.switch_client("ajax-web-fix-login").mode,
            CommandMode::InheritStdio
        );
        assert_eq!(adapter.list_sessions().mode, CommandMode::Capture);
    }

    #[test]
    fn tmux_parsers_detect_session_and_worktrunk_health() {
        let tmux = TmuxAdapter::parse_session_status(
            "ajax-web-fix-login",
            "ajax-api-add-cache\najax-web-fix-login\n",
        );
        let worktrunk = TmuxAdapter::parse_worktrunk_status(
            "worktrunk",
            "/tmp/worktree",
            "agent\t/tmp/worktree\nworktrunk\t/tmp/worktree\n",
        );

        assert_eq!(tmux, TmuxStatus::present("ajax-web-fix-login"));
        assert_eq!(
            worktrunk,
            WorktrunkStatus::present("worktrunk", "/tmp/worktree")
        );
    }

    #[test]
    fn tmux_worktrunk_parser_detects_wrong_path() {
        let worktrunk = TmuxAdapter::parse_worktrunk_status(
            "worktrunk",
            "/tmp/worktree",
            "worktrunk\t/tmp/wrong\n",
        );

        assert!(worktrunk.exists);
        assert_eq!(
            worktrunk.current_path,
            std::path::PathBuf::from("/tmp/wrong")
        );
        assert!(!worktrunk.points_at_expected_path);
    }

    #[test]
    fn git_adapter_builds_status_commands_for_worktrees() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.status("/tmp/worktree"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktree",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )
        );
        assert_eq!(
            adapter.merge_base_is_ancestor("/tmp/worktree", "ajax/fix-login", "main"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktree",
                    "merge-base",
                    "--is-ancestor",
                    "ajax/fix-login",
                    "main"
                ]
            )
        );
    }

    #[test]
    fn agent_adapter_builds_launch_command() {
        let adapter = AgentAdapter::new("codex");
        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: "fix login".to_string(),
        };

        assert_eq!(
            adapter.launch(&launch),
            CommandSpec::new("codex", ["--cd", "/tmp/worktree", "fix login"])
        );
    }

    #[test]
    fn recording_runner_captures_planned_commands_without_executing() {
        let mut runner = RecordingCommandRunner::default();
        let output = runner.run(&CommandSpec::new("git", ["status"])).unwrap();

        assert_eq!(output.status_code, 0);
        assert_eq!(runner.commands(), &[CommandSpec::new("git", ["status"])]);
    }

    #[test]
    fn git_status_parser_detects_dirty_untracked_conflicts_and_divergence() {
        let status = GitAdapter::parse_status(
            "## ajax/fix-login...origin/ajax/fix-login [ahead 2, behind 1]\n M src/main.rs\n?? scratch.txt\nUU src/auth.rs\n",
            true,
        );

        assert!(status.worktree_exists);
        assert!(status.branch_exists);
        assert_eq!(status.current_branch.as_deref(), Some("ajax/fix-login"));
        assert!(status.dirty);
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.untracked_files, 1);
        assert_eq!(status.unpushed_commits, 2);
        assert!(status.conflicted);
        assert!(status.merged);
    }

    #[test]
    fn git_status_parser_handles_clean_local_branch() {
        let status = GitAdapter::parse_status("## main\n", false);

        assert!(status.worktree_exists);
        assert!(status.branch_exists);
        assert_eq!(status.current_branch.as_deref(), Some("main"));
        assert!(!status.dirty);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
        assert_eq!(status.untracked_files, 0);
        assert_eq!(status.unpushed_commits, 0);
        assert!(!status.conflicted);
        assert!(!status.merged);
    }
}
