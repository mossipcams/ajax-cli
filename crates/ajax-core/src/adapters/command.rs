use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
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

    pub fn with_cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
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
        cwd: Option<PathBuf>,
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
                    write!(formatter, " in {}", cwd.display())?;
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::CommandRunError;

    #[test]
    fn command_run_errors_include_context_without_empty_stderr_suffix() {
        assert_eq!(
            CommandRunError::SpawnFailed("permission denied".to_string()).to_string(),
            "failed to start command: permission denied"
        );
        assert_eq!(
            CommandRunError::MissingStatusCode.to_string(),
            "command exited without a status code"
        );
        assert_eq!(
            CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 128,
                stderr: " fatal: nope \n".to_string(),
                cwd: Some(PathBuf::from("/repo")),
            }
            .to_string(),
            "git exited with status 128 in /repo: fatal: nope"
        );
        assert_eq!(
            CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 1,
                stderr: " \n".to_string(),
                cwd: None,
            }
            .to_string(),
            "git exited with status 1"
        );
    }
}
