#![deny(unsafe_op_in_unsafe_fn)]

use std::{error::Error, fmt};

pub mod codex;
pub mod process;
pub mod renderer;
pub mod repo;

pub use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent, RepoEvent};

#[derive(Debug)]
pub enum SupervisorError {
    Io(String),
    Json(String),
    Notify(String),
    Process(String),
}

impl fmt::Display for SupervisorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "I/O error: {message}"),
            Self::Json(message) => write!(formatter, "json error: {message}"),
            Self::Notify(message) => write!(formatter, "notify error: {message}"),
            Self::Process(message) => write!(formatter, "process error: {message}"),
        }
    }
}

impl Error for SupervisorError {}

impl From<std::io::Error> for SupervisorError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

impl From<serde_json::Error> for SupervisorError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

impl From<notify::Error> for SupervisorError {
    fn from(error: notify::Error) -> Self {
        Self::Notify(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::SupervisorError;

    #[test]
    fn supervisor_errors_have_operator_facing_display() {
        assert_eq!(
            SupervisorError::Process("codex exited".to_string()).to_string(),
            "process error: codex exited"
        );
        assert_eq!(
            SupervisorError::Json("expected value".to_string()).to_string(),
            "json error: expected value"
        );
    }
}
