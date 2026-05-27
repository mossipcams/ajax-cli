//! Shared browser action capability vocabulary for Web Cockpit slices.

use ajax_core::{models::OperatorAction, output::TaskCard};

pub const SYNC_ACTION: &str = "sync";

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
            Some("live agent typing requires native cockpit"),
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

pub fn sync_action_state() -> WebActionState {
    WebActionState {
        action: SYNC_ACTION.to_string(),
        status: "supported".to_string(),
        reason: Some("refresh task runtime without terminal attach"),
        destructive: false,
        confirmation_required: false,
    }
}

pub fn browser_action_states(card: &TaskCard) -> Vec<WebActionState> {
    let mut states = card
        .available_actions
        .iter()
        .copied()
        .filter(|action| *action != OperatorAction::Start)
        .map(web_action_state)
        .collect::<Vec<_>>();

    let resume_relevant = card.available_actions.contains(&OperatorAction::Resume)
        || card.primary_action == OperatorAction::Resume;
    if resume_relevant && !states.iter().any(|state| state.action == SYNC_ACTION) {
        states.push(sync_action_state());
    }

    states
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    web_action_state(action).status == "supported"
}

pub fn supported_browser_action(action: &str) -> bool {
    if action == SYNC_ACTION {
        return true;
    }
    OperatorAction::from_label(action).is_some_and(supported_web_action)
}

pub fn primary_browser_action(card: &TaskCard) -> String {
    if supported_web_action(card.primary_action) {
        return card.primary_action.as_str().to_string();
    }
    if card.available_actions.contains(&OperatorAction::Resume)
        || card.primary_action == OperatorAction::Resume
    {
        return SYNC_ACTION.to_string();
    }
    card.available_actions
        .iter()
        .copied()
        .find(|action| supported_web_action(*action))
        .map(|action| action.as_str().to_string())
        .unwrap_or_else(|| card.primary_action.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        browser_action_states, primary_browser_action, supported_browser_action,
        supported_web_action, web_action_state, SYNC_ACTION,
    };
    use ajax_core::{
        models::{LifecycleStatus, OperatorAction, TaskId},
        output::TaskCard,
        ui_state::UiState,
    };

    #[test]
    fn resume_action_needs_terminal_in_web_cockpit() {
        let state = web_action_state(OperatorAction::Resume);

        assert_eq!(state.action, "resume");
        assert_eq!(state.status, "needs_terminal");
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

    #[test]
    fn browser_action_states_adds_sync_when_resume_is_relevant() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            ui_state: UiState::Running,
            status_label: "running".to_string(),
            lifecycle: LifecycleStatus::Active,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume, OperatorAction::Review],
            live_summary: None,
        };

        let states = browser_action_states(&card);

        assert!(states.iter().any(|state| state.action == SYNC_ACTION));
        assert!(states.iter().any(|state| state.action == "review"));
        assert_eq!(primary_browser_action(&card), SYNC_ACTION);
        assert!(supported_browser_action(SYNC_ACTION));
    }
}
