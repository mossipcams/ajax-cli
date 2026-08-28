//! Host-owned ACP chat-context continuity state projected into protocol snapshots.

use serde::{Deserialize, Serialize};

/// Whether the harness can prove the same model context is active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextState {
    #[default]
    Live,
    Restored,
    Unavailable,
}

/// Continuity inputs carried on every attach/snapshot envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextContinuity {
    pub state: ContextState,
    pub epoch: u64,
    pub error: Option<String>,
}

impl Default for ContextContinuity {
    fn default() -> Self {
        Self {
            state: ContextState::Live,
            epoch: 0,
            error: None,
        }
    }
}

impl ContextContinuity {
    pub fn live(epoch: u64) -> Self {
        Self {
            state: ContextState::Live,
            epoch,
            error: None,
        }
    }

    pub fn restored(epoch: u64) -> Self {
        Self {
            state: ContextState::Restored,
            epoch,
            error: None,
        }
    }

    pub fn unavailable(epoch: u64, error: String) -> Self {
        Self {
            state: ContextState::Unavailable,
            epoch,
            error: Some(error),
        }
    }

    pub fn prompts_blocked(&self) -> bool {
        matches!(self.state, ContextState::Unavailable)
    }
}
