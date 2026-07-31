//! Per-client capability profiles for canonical agent events.
//!
//! Declares which facts each client can supply natively, via wrapper, pane
//! fallback, or not at all. Used by status projection and pane fallback to
//! avoid inventing high-confidence wait states from silence.

use crate::models::AgentClient;

/// How a client supplies a canonical fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilitySupport {
    Native,
    Wrapper,
    PaneFallback,
    Unavailable,
    Unverified,
}

/// A canonical fact whose coverage can vary per client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityFact {
    TurnStarted,
    TurnSettled,
    PermissionWait,
    QuestionWait,
    Subagents,
    SessionClosed,
}

/// Declared coverage for each canonical fact on a given client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentCapabilityProfile {
    pub turn_started: CapabilitySupport,
    pub turn_settled: CapabilitySupport,
    pub permission_wait: CapabilitySupport,
    pub question_wait: CapabilitySupport,
    pub subagents: CapabilitySupport,
    pub session_closed: CapabilitySupport,
}

impl AgentCapabilityProfile {
    pub fn support_for(self, fact: CapabilityFact) -> CapabilitySupport {
        match fact {
            CapabilityFact::TurnStarted => self.turn_started,
            CapabilityFact::TurnSettled => self.turn_settled,
            CapabilityFact::PermissionWait => self.permission_wait,
            CapabilityFact::QuestionWait => self.question_wait,
            CapabilityFact::Subagents => self.subagents,
            CapabilityFact::SessionClosed => self.session_closed,
        }
    }

    pub fn supports_native(self, fact: CapabilityFact) -> bool {
        matches!(self.support_for(fact), CapabilitySupport::Native)
    }

    pub fn allows_pane_fallback(self, fact: CapabilityFact) -> bool {
        matches!(
            self.support_for(fact),
            CapabilitySupport::Unavailable | CapabilitySupport::Unverified
        )
    }
}

pub fn profile_for_agent_client(client: AgentClient) -> AgentCapabilityProfile {
    match client {
        AgentClient::Claude => claude_profile(),
        AgentClient::Codex => codex_profile(),
        AgentClient::Cursor => cursor_profile(),
        AgentClient::Pi => pi_profile(),
        AgentClient::Other => unknown_other_profile(),
    }
}

pub fn profile_for_hook_client(client: &str) -> AgentCapabilityProfile {
    match client.trim().to_ascii_lowercase().as_str() {
        "claude" => claude_profile(),
        "codex" => codex_profile(),
        "cursor" => cursor_profile(),
        "pi" => pi_profile(),
        _ => unknown_other_profile(),
    }
}

const fn claude_profile() -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        turn_started: CapabilitySupport::Native,
        turn_settled: CapabilitySupport::Native,
        permission_wait: CapabilitySupport::Native,
        question_wait: CapabilitySupport::Native,
        subagents: CapabilitySupport::Unverified,
        session_closed: CapabilitySupport::Native,
    }
}

const fn codex_profile() -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        turn_started: CapabilitySupport::Native,
        turn_settled: CapabilitySupport::Native,
        permission_wait: CapabilitySupport::Native,
        question_wait: CapabilitySupport::Unavailable,
        subagents: CapabilitySupport::Unverified,
        session_closed: CapabilitySupport::Native,
    }
}

const fn cursor_profile() -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        turn_started: CapabilitySupport::Native,
        turn_settled: CapabilitySupport::Native,
        permission_wait: CapabilitySupport::Unavailable,
        question_wait: CapabilitySupport::Unavailable,
        subagents: CapabilitySupport::Unverified,
        session_closed: CapabilitySupport::Native,
    }
}

const fn pi_profile() -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        turn_started: CapabilitySupport::Native,
        turn_settled: CapabilitySupport::Native,
        permission_wait: CapabilitySupport::Unavailable,
        question_wait: CapabilitySupport::Unavailable,
        subagents: CapabilitySupport::Unverified,
        session_closed: CapabilitySupport::Wrapper,
    }
}

const fn unknown_other_profile() -> AgentCapabilityProfile {
    AgentCapabilityProfile {
        turn_started: CapabilitySupport::Unverified,
        turn_settled: CapabilitySupport::Wrapper,
        permission_wait: CapabilitySupport::Unverified,
        question_wait: CapabilitySupport::Unverified,
        subagents: CapabilitySupport::Unverified,
        session_closed: CapabilitySupport::Unverified,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_profile_marks_wait_capabilities_unavailable() {
        let profile = profile_for_hook_client("cursor");
        assert_eq!(profile.permission_wait, CapabilitySupport::Unavailable);
        assert_eq!(profile.question_wait, CapabilitySupport::Unavailable);
    }

    #[test]
    fn claude_profile_has_native_permission_and_question() {
        let profile = profile_for_agent_client(AgentClient::Claude);
        assert!(profile.supports_native(CapabilityFact::PermissionWait));
        assert!(profile.supports_native(CapabilityFact::QuestionWait));
    }

    #[test]
    fn pane_fallback_allowed_only_when_unavailable_or_unverified() {
        let cursor = profile_for_hook_client("cursor");
        assert!(cursor.allows_pane_fallback(CapabilityFact::QuestionWait));

        let claude = profile_for_agent_client(AgentClient::Claude);
        assert!(!claude.allows_pane_fallback(CapabilityFact::QuestionWait));
    }

    #[test]
    fn cursor_agent_client_profile_matches_hook_client() {
        let from_client = profile_for_agent_client(AgentClient::Cursor);
        let from_hook = profile_for_hook_client("cursor");
        assert_eq!(
            from_client.permission_wait, from_hook.permission_wait,
            "Cursor wait capabilities must match hook profile"
        );
        assert_eq!(from_client.question_wait, from_hook.question_wait);
        assert_eq!(from_client.permission_wait, CapabilitySupport::Unavailable);
    }

    #[test]
    fn pi_agent_client_profile_matches_hook_client() {
        let from_client = profile_for_agent_client(AgentClient::Pi);
        let from_hook = profile_for_hook_client("pi");
        assert_eq!(from_client.permission_wait, from_hook.permission_wait);
        assert_eq!(from_client.question_wait, from_hook.question_wait);
        assert_eq!(from_client.permission_wait, CapabilitySupport::Unavailable);
    }
}
