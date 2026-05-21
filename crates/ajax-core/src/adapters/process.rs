use std::process::{Command, Stdio};

use super::command::{CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let mut process = Command::new(&command.program);
        clear_repo_local_git_environment(&mut process);
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

fn clear_repo_local_git_environment(process: &mut Command) {
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        process.env_remove(name);
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn process_runner_clears_git_hook_repository_environment_from_children() {
        let mut command = Command::new("git");
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ] {
            command.env(name, format!("repo-local-{name}"));
        }

        super::clear_repo_local_git_environment(&mut command);

        let cleared = command
            .get_envs()
            .filter_map(|(name, value)| value.is_none().then_some(name))
            .collect::<Vec<_>>();
        for name in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        ] {
            assert!(
                cleared.iter().any(|cleared_name| *cleared_name == name),
                "{name} should be explicitly removed from child command environment"
            );
        }
    }
}
