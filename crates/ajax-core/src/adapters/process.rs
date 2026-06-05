use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
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
        let mut child = process
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;

        let started = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?
                .is_some()
            {
                let output = child
                    .wait_with_output()
                    .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;
                return command_output_from_process_output(output);
            }

            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CommandRunError::TimedOut {
                    program: command.program.clone(),
                    timeout,
                });
            }

            let remaining = timeout.saturating_sub(started.elapsed());
            thread::sleep(remaining.min(Duration::from_millis(5)));
        }
    } else {
        let output = process
            .output()
            .map_err(|error| CommandRunError::SpawnFailed(error.to_string()))?;
        command_output_from_process_output(output)
    }
}

fn command_output_from_process_output(
    output: std::process::Output,
) -> Result<CommandOutput, CommandRunError> {
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

    #[test]
    fn timed_out_command_is_terminated() {
        let root = std::env::temp_dir().join(format!(
            "ajax-timeout-kill-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let marker = root.join("marker");
        let script = format!("sleep 0.2; touch '{}'", marker.display());
        let mut runner = ProcessCommandRunner;
        let command =
            CommandSpec::new("sh", ["-c", script.as_str()]).with_timeout(Duration::from_millis(25));

        let error = runner.run(&command).unwrap_err();
        std::thread::sleep(Duration::from_millis(350));

        assert!(matches!(error, CommandRunError::TimedOut { .. }));
        assert!(
            !marker.exists(),
            "timed-out child continued running and created {}",
            marker.display()
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
