//! Shared browser action capability vocabulary for Web Cockpit slices.

use ajax_core::models::OperatorAction;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebActionState {
    pub action: String,
    pub status: String,
    pub reason: Option<&'static str>,
    pub destructive: bool,
    pub confirmation_required: bool,
}

pub fn web_action_state(action: OperatorAction) -> WebActionState {
    let (status, reason) = match action {
        OperatorAction::Review
        | OperatorAction::Ship
        | OperatorAction::Repair
        | OperatorAction::Drop => ("supported", None),
        OperatorAction::Resume => (
            "needs_terminal",
            Some("terminal attach requires native cockpit"),
        ),
        OperatorAction::Start => (
            "unsupported",
            Some("start uses the dedicated Web Cockpit new-task operation"),
        ),
    };

    WebActionState {
        action: action.as_str().to_string(),
        status: status.to_string(),
        reason,
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
    }
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    web_action_state(action).status == "supported"
}

#[cfg(test)]
mod tests {
    use super::{supported_web_action, web_action_state};
    use ajax_core::models::OperatorAction;

    #[test]
    fn resume_action_needs_terminal_in_web_cockpit() {
        let state = web_action_state(OperatorAction::Resume);

        assert_eq!(state.action, "resume");
        assert_eq!(state.status, "needs_terminal");
        assert_eq!(
            state.reason,
            Some("terminal attach requires native cockpit")
        );
        assert!(!supported_web_action(OperatorAction::Resume));
    }

    #[test]
    fn review_action_is_supported_in_web_cockpit() {
        let state = web_action_state(OperatorAction::Review);

        assert_eq!(state.action, "review");
        assert_eq!(state.status, "supported");
        assert!(state.reason.is_none());
        assert!(supported_web_action(OperatorAction::Review));
    }
}
