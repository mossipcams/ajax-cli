use std::process::{Command, Stdio};

use super::command::{CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec};

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
        }
    }
}
