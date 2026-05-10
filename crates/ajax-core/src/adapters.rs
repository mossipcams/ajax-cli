use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};

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
pub struct WorkmuxNewTask {
    pub repo_path: String,
    pub branch: String,
    pub title: String,
    pub agent: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkmuxAdapter {
    program: String,
}

impl WorkmuxAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn add_task(&self, task: &WorkmuxNewTask) -> CommandSpec {
        self.add_task_with_args(task, [])
    }

    pub fn add_task_open_if_exists(&self, task: &WorkmuxNewTask) -> CommandSpec {
        self.add_task_with_args(task, ["--open-if-exists"])
    }

    pub fn open_task(&self, qualified_handle: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["open", qualified_handle])
    }

    pub fn merge_task(&self, qualified_handle: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["merge", qualified_handle])
    }

    pub fn remove_task(&self, qualified_handle: &str) -> CommandSpec {
        CommandSpec::new(&self.program, ["remove", qualified_handle])
    }

    fn add_task_with_args<const N: usize>(
        &self,
        task: &WorkmuxNewTask,
        extra_args: [&str; N],
    ) -> CommandSpec {
        let mut args = vec![
            "add".to_string(),
            task.branch.clone(),
            "--agent".to_string(),
            task.agent.clone(),
            "--background".to_string(),
            "--no-hooks".to_string(),
        ];
        args.extend(extra_args.into_iter().map(str::to_string));

        CommandSpec {
            program: self.program.clone(),
            args,
            cwd: Some(task.repo_path.clone()),
            mode: CommandMode::Capture,
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

    pub fn ensure_worktrunk(&self, session: &str, window: &str, path: &str) -> CommandSpec {
        CommandSpec::new(
            &self.program,
            ["new-window", "-t", session, "-n", window, "-c", path],
        )
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
        RecordingCommandRunner, TmuxAdapter, WorkmuxAdapter, WorkmuxNewTask,
    };
    use crate::models::{TmuxStatus, WorktrunkStatus};

    #[test]
    fn workmux_adapter_builds_lifecycle_commands() {
        let adapter = WorkmuxAdapter::new("workmux");
        let new_task = WorkmuxNewTask {
            repo_path: "/Users/matt/projects/web".to_string(),
            branch: "ajax/fix-login".to_string(),
            title: "fix login".to_string(),
            agent: "codex".to_string(),
        };

        assert_eq!(
            adapter.add_task(&new_task),
            CommandSpec::new(
                "workmux",
                [
                    "add",
                    "ajax/fix-login",
                    "--agent",
                    "codex",
                    "--background",
                    "--no-hooks"
                ]
            )
            .with_cwd("/Users/matt/projects/web")
        );
        assert_eq!(
            adapter.open_task("web/fix-login"),
            CommandSpec::new("workmux", ["open", "web/fix-login"])
        );
        assert_eq!(
            adapter.add_task_open_if_exists(&new_task),
            CommandSpec::new(
                "workmux",
                [
                    "add",
                    "ajax/fix-login",
                    "--agent",
                    "codex",
                    "--background",
                    "--no-hooks",
                    "--open-if-exists"
                ]
            )
            .with_cwd("/Users/matt/projects/web")
        );
        assert_eq!(
            adapter.merge_task("web/fix-login"),
            CommandSpec::new("workmux", ["merge", "web/fix-login"])
        );
        assert_eq!(
            adapter.remove_task("web/fix-login"),
            CommandSpec::new("workmux", ["remove", "web/fix-login"])
        );
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
        let output = runner
            .run(&CommandSpec::new("workmux", ["open", "web/fix-login"]))
            .unwrap();

        assert_eq!(output.status_code, 0);
        assert_eq!(
            runner.commands(),
            &[CommandSpec::new("workmux", ["open", "web/fix-login"])]
        );
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
