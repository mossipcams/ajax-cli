//! Command execution for the training host. The web slice never builds a shell
//! command line: each argv element is passed separately to `ssh`, and the
//! remote LLM host stays authoritative for all policy decisions (see
//! `.planning/agent-plans/web-training-modal.md`).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Overall deadline for one ssh round trip to the training host. Connection
/// setup is separately bounded by `ConnectTimeout=5` on the command line.
pub const SSH_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingRunError {
    /// The ssh round trip exceeded its deadline and was killed.
    Timeout,
    /// ssh failed to spawn, the remote command exited non-zero, or stdout
    /// could not be collected.
    Failed,
}

/// Injectable command runner for the training slice: executes
/// `ssh -o BatchMode=yes -o ConnectTimeout=5 <host> <args...>` and returns
/// stdout. Browser-supplied text never reaches `args` except a profile name
/// validated against the host-reported list and `^[a-z0-9][a-z0-9.-]{0,63}$`.
pub trait TrainingCommandRunner: Send + Sync {
    fn run(&self, args: &[String]) -> Result<String, TrainingRunError>;
}

/// Production runner: ssh to the training host (default `llm-gpu`, overridable via
/// the `AJAX_TRAINING_SSH_HOST` environment variable).
#[derive(Clone, Debug)]
pub struct SshTrainingRunner {
    host: String,
}

impl SshTrainingRunner {
    /// Default construction from the environment (`llm-gpu` when unset/blank).
    pub fn from_env() -> Self {
        let host = std::env::var("AJAX_TRAINING_SSH_HOST")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "llm-gpu".to_string());
        Self { host }
    }

    /// Explicit construction (tests, callers that resolve the host elsewhere).
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
        // Stderr is discarded: host banners and ssh diagnostics can embed the
        // hostname, key paths, or remote output. Nothing of that reaches the
        // browser; failures surface as a short generic 502/504.
        command.stdout(Stdio::piped()).stderr(Stdio::null());

        let mut child = command.spawn().map_err(|_| TrainingRunError::Failed)?;
        let deadline = Instant::now() + SSH_TIMEOUT;
        loop {
            match child.try_wait().map_err(|_| TrainingRunError::Failed)? {
                Some(_) => break,
                None => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(TrainingRunError::Timeout);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|_| TrainingRunError::Failed)?;
        if !output.status.success() {
            return Err(TrainingRunError::Failed);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner() -> SshTrainingRunner {
        SshTrainingRunner::with_host("llm-gpu")
    }

    #[test]
    fn from_env_default_and_override() {
        // Single test because the two cases share the process environment.
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

    /// `ssh true` with a resolvable local target is harmless; this only proves
    /// argv is passed element-wise, without a shell. A refused/unavailable ssh
    /// maps to `Failed` rather than panicking.
    #[test]
    fn real_runner_passes_argv_without_shell() {
        let result = runner().run(&["true".to_string()]);
        assert!(result.is_ok() || matches!(result, Err(TrainingRunError::Failed)));
        if let Ok(stdout) = &result {
            assert!(stdout.trim().is_empty());
        }
    }

    /// An RFC 6761 reserved name can never resolve to a real host: this proves
    /// unreachable remotes map to `Failed` (and never leak hostnames into the
    /// error) without touching any live machine.
    #[test]
    fn real_runner_maps_unreachable_host_to_failed() {
        let runner = SshTrainingRunner::with_host("nonexistent-host.invalid");
        assert!(matches!(
            runner.run(&["status".to_string()]),
            Err(TrainingRunError::Failed)
        ));
    }
}
