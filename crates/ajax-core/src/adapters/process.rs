use std::{
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};

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
        if command.program == "git" {
            clear_repo_local_git_env(&mut process);
        }
        match command.mode {
            CommandMode::Capture => run_capture(process, command),
            CommandMode::InheritStdio => run_inherit_stdio(process),
        }
    }
}

fn run_capture(
    mut process: Command,
    command: &CommandSpec,
) -> Result<CommandOutput, CommandRunError> {
    if let Some(timeout) = command.timeout {
        let child = process
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;
        let shared_child = Arc::new(Mutex::new(Some(child)));
        let waiter_child = Arc::clone(&shared_child);
        let (sender, receiver) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let output = waiter_child
                .lock()
                .ok()
                .and_then(|mut guard| guard.take())
                .and_then(|child| child.wait_with_output().ok());
            let _ = sender.send(output);
        });
        match receiver.recv_timeout(timeout) {
            Ok(Some(output)) => {
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
            Ok(None) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if let Ok(mut guard) = shared_child.lock() {
                    if let Some(mut child) = guard.take() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
                Err(CommandRunError::TimedOut {
                    program: command.program.clone(),
                    timeout,
                })
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(CommandRunError::MissingStatusCode)
            }
        }
    } else {
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
}

fn run_inherit_stdio(mut process: Command) -> Result<CommandOutput, CommandRunError> {
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

fn clear_repo_local_git_env(process: &mut Command) {
    for variable in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        process.env_remove(variable);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{CommandRunError, CommandRunner, CommandSpec, ProcessCommandRunner};

    #[test]
    fn capture_command_times_out_when_configured() {
        let mut runner = ProcessCommandRunner;
        let command = CommandSpec::new("sleep", ["2"]).with_timeout(Duration::from_millis(50));

        let error = runner.run(&command).unwrap_err();

        assert!(matches!(
            error,
            CommandRunError::TimedOut {
                program,
                timeout,
            } if program == "sleep" && timeout == Duration::from_millis(50)
        ));
    }

    #[test]
    fn capture_command_completes_before_timeout() {
        let mut runner = ProcessCommandRunner;
        let command = CommandSpec::new("true", []).with_timeout(Duration::from_secs(5));

        let output = runner.run(&command).unwrap();

        assert_eq!(output.status_code, 0);
    }

    #[test]
    fn timed_out_command_does_not_block_follow_up_commands() {
        let mut runner = ProcessCommandRunner;
        let slow = CommandSpec::new("sleep", ["1"]).with_timeout(Duration::from_millis(25));
        let _ = runner.run(&slow).unwrap_err();

        let output = runner.run(&CommandSpec::new("true", [])).unwrap();

        assert_eq!(output.status_code, 0);
    }
}
