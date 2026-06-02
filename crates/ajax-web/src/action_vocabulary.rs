//! Shared browser action capability vocabulary for Web Cockpit slices.

use ajax_core::{models::OperatorAction, output::TaskCard};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebActionState {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
        OperatorAction::Resume => ("unsupported", Some("resume requires native cockpit")),
        OperatorAction::Start => (
            "unsupported",
            Some("start uses the dedicated Web Cockpit new-task operation"),
        ),
    };

    WebActionState {
        action: action.as_str().to_string(),
        label: None,
        status: status.to_string(),
        reason,
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
    }
}

pub fn remediation_action_state(
    option: &ajax_core::remediation::RemediationOption,
) -> WebActionState {
    WebActionState {
        action: option.id.clone(),
        label: Some(option.label.clone()),
        status: "supported".to_string(),
        reason: Some("runs the skill brief in the task agent session"),
        destructive: false,
        confirmation_required: false,
    }
}

pub fn browser_action_states(card: &TaskCard) -> Vec<WebActionState> {
    let mut states: Vec<WebActionState> = card
        .remediations
        .iter()
        .map(remediation_action_state)
        .collect();

    states.extend(
        card.available_actions
            .iter()
            .copied()
            .filter(|action| supported_web_action(*action))
            .map(web_action_state),
    );

    states
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    web_action_state(action).status == "supported"
}

pub fn supported_browser_action(action: &str) -> bool {
    if ajax_core::remediation::is_remediation_action(action) {
        return true;
    }
    OperatorAction::from_label(action).is_some_and(supported_web_action)
}

pub fn primary_browser_action(card: &TaskCard) -> String {
    if let Some(remediation) = card.remediations.first() {
        return remediation.id.clone();
    }
    if supported_web_action(card.primary_action) {
        return card.primary_action.as_str().to_string();
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
        supported_web_action, web_action_state,
    };
    use ajax_core::{
        models::{
            LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction, SideFlag, TaskId,
        },
        output::TaskCard,
        remediation::FIX_CI,
        ui_state::UiState,
    };

    #[test]
    fn resume_action_needs_terminal_in_web_cockpit() {
        let state = web_action_state(OperatorAction::Resume);

        assert_eq!(state.action, "resume");
        assert_eq!(state.status, "unsupported");
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
    fn browser_action_states_do_not_surface_resume_or_sync_in_web_cockpit() {
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
            remediations: Vec::new(),
            live_summary: None,
        };

        let states = browser_action_states(&card);

        assert!(!states.iter().any(|state| state.action == "resume"));
        assert!(!states.iter().any(|state| state.action == "sync"));
        assert!(states.iter().any(|state| state.action == "review"));
        assert_eq!(primary_browser_action(&card), "review");
        assert!(!supported_browser_action("sync"));
    }

    #[test]
    fn browser_action_states_surface_remediation_buttons_as_supported() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            ui_state: UiState::Blocked,
            status_label: "ci failed".to_string(),
            lifecycle: LifecycleStatus::Error,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume],
            remediations: ajax_core::remediation::remediations_for_task(&blocked_ci_task()),
            live_summary: Some("ci failed".to_string()),
        };

        let states = browser_action_states(&card);
        let fix_ci = states
            .iter()
            .find(|state| state.action == FIX_CI)
            .expect("fix-ci action");

        assert_eq!(fix_ci.status, "supported");
        assert_eq!(primary_browser_action(&card), FIX_CI);
        assert!(supported_browser_action(FIX_CI));
    }

    fn blocked_ci_task() -> ajax_core::models::Task {
        use ajax_core::models::{AgentClient, Task};
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);
        task
    }
}
