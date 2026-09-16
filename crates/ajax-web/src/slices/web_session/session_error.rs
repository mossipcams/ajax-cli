//! Typed errors at the web-session slice boundary.

use crate::adapters::web_session_acp::{AcpSpawnError, RestoreFailure};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    Spawn(String),
    Persist(String),
    Protocol(String),
    Operator(String),
    RestoreUnavailable(RestoreFailure),
}

impl SessionError {
    pub fn spawn(message: impl Into<String>) -> Self {
        Self::Spawn(message.into())
    }

    pub fn persist(message: impl Into<String>) -> Self {
        Self::Persist(message.into())
    }

    pub fn protocol(message: impl Into<String>) -> Self {
        Self::Protocol(message.into())
    }

    pub fn operator(message: impl Into<String>) -> Self {
        Self::Operator(message.into())
    }

    pub fn restore_unavailable(error: RestoreFailure) -> Self {
        Self::RestoreUnavailable(error)
    }

    pub fn classify_spawn(error: AcpSpawnError) -> Self {
        match error {
            AcpSpawnError::Restore(error) => Self::RestoreUnavailable(error),
            AcpSpawnError::Message(message) => Self::Spawn(message),
        }
    }

    pub fn is_restore_unavailable(&self) -> bool {
        matches!(self, Self::RestoreUnavailable(_))
    }

    /// Stable id for transcript dedupe on reconnect ([#1040](https://github.com/mossipcams/ajax-cli/issues/1040)).
    pub fn spawn_error_id(generation: u64, message: &str) -> Option<String> {
        let kind = if message.contains("Authentication required") {
            "auth"
        } else if message.contains("session/new failed") {
            "session_new"
        } else if message.contains("ACP startup timed out") {
            "startup_timeout"
        } else if message.starts_with("ACP restore unavailable") {
            "restore_unavailable"
        } else {
            return None;
        };
        Some(format!("g{generation}:spawn:{kind}"))
    }
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(message) => write!(f, "{message}"),
            Self::Persist(message) => write!(f, "{message}"),
            Self::Protocol(message) => write!(f, "{message}"),
            Self::Operator(message) => write!(f, "{message}"),
            Self::RestoreUnavailable(error) => write!(f, "{error}"),
        }
    }
}

impl From<SessionError> for String {
    fn from(error: SessionError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_auth_error_id_is_stable_per_generation() {
        let msg = "ACP session/new failed: Authentication required";
        assert_eq!(
            SessionError::spawn_error_id(3, msg).as_deref(),
            Some("g3:spawn:auth")
        );
        assert_eq!(
            SessionError::spawn_error_id(4, msg).as_deref(),
            Some("g4:spawn:auth")
        );
    }

    #[test]
    fn restore_unavailable_classifies_as_typed_variant() {
        assert!(
            SessionError::classify_spawn(AcpSpawnError::Restore(RestoreFailure::TimedOut {
                session_id: "s1".to_string(),
                method: crate::adapters::web_session_acp::RestoreMethod::Load,
            },))
            .is_restore_unavailable()
        );
    }
}
