#![deny(unsafe_op_in_unsafe_fn)]

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
