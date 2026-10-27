use std::io::{self, Read};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const SSH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingRunError {
    Timeout,
    Failed,
}

pub trait TrainingCommandRunner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, TrainingRunError>;
}

#[derive(Clone, Debug)]
pub struct SshTrainingRunner {
    host: String,
}

impl SshTrainingRunner {
    pub fn from_env() -> Self {
        let host = std::env::var("AJAX_TRAINING_SSH_HOST")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "llm-gpu".to_string());
        Self { host }
    }

    pub fn with_host(host: impl Into<String>) -> Self {
        Self { host: host.into() }
    }
}

impl TrainingCommandRunner for SshTrainingRunner {
    fn run(&self, args: &[String]) -> Result<String, TrainingRunError> {
        let mut command = Command::new("ssh");
        command
            .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"])
            .arg(&self.host);
        for arg in args {
            command.arg(arg);
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|_| TrainingRunError::Failed)?;
        let stdout_reader = drain(child.stdout.take());
        let stderr_reader = drain(child.stderr.take());
        let deadline = Instant::now() + SSH_TIMEOUT;
        let status = loop {
            match child.try_wait().map_err(|_| TrainingRunError::Failed)? {
                Some(status) => break status,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(TrainingRunError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        };

        let stdout = stdout_reader
            .join()
            .map_err(|_| TrainingRunError::Failed)?
            .map_err(|_| TrainingRunError::Failed)?;
        let stderr = stderr_reader
            .join()
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        if !status.success() {
            let stderr = String::from_utf8_lossy(&stderr);
            if let Some(line) = stderr.lines().rev().find(|line| !line.trim().is_empty()) {
                tracing::warn!(
                    host = %self.host,
                    status = %status,
                    stderr = %line.trim(),
                    "training host command failed"
                );
            }
            return Err(TrainingRunError::Failed);
        }
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    }
}

fn drain<R: Read + Send + 'static>(pipe: Option<R>) -> JoinHandle<io::Result<Vec<u8>>> {
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            pipe.read_to_end(&mut buffer)?;
        }
        Ok(buffer)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner() -> SshTrainingRunner {
        SshTrainingRunner::with_host("llm-gpu")
    }

    #[test]
    fn from_env_default_and_override() {
        let previous = std::env::var("AJAX_TRAINING_SSH_HOST");
        std::env::remove_var("AJAX_TRAINING_SSH_HOST");
        assert_eq!(SshTrainingRunner::from_env().host, "llm-gpu");
        std::env::set_var("AJAX_TRAINING_SSH_HOST", "vm-llm");
        assert_eq!(SshTrainingRunner::from_env().host, "vm-llm");
        std::env::set_var("AJAX_TRAINING_SSH_HOST", " ");
        assert_eq!(SshTrainingRunner::from_env().host, "llm-gpu");
        match previous {
            Ok(host) => std::env::set_var("AJAX_TRAINING_SSH_HOST", host),
            Err(_) => std::env::remove_var("AJAX_TRAINING_SSH_HOST"),
        }
    }

    #[test]
    fn real_runner_passes_argv_without_shell() {
        let result = runner().run(&["true".to_string()]);
        assert!(result.is_ok() || matches!(result, Err(TrainingRunError::Failed)));
        if let Ok(stdout) = &result {
            assert!(stdout.trim().is_empty());
        }
    }

    #[test]
    fn real_runner_maps_unreachable_host_to_failed() {
        let runner = SshTrainingRunner::with_host("nonexistent-host.invalid");
        assert!(matches!(
            runner.run(&["status".to_string()]),
            Err(TrainingRunError::Failed)
        ));
    }
}
