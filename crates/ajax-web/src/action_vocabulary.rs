//! Shared browser action capability vocabulary for Web Cockpit slices.

use ajax_core::{
    models::OperatorAction,
    output::TaskCard,
    slices::remediate::{self, RemediationOption},
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebAction {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub destructive: bool,
    pub confirmation_required: bool,
}

pub fn web_action(action: OperatorAction) -> Option<WebAction> {
    if !supported_web_action(action) {
        return None;
    }
    Some(WebAction {
        action: action.as_str().to_string(),
        label: None,
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
    })
}

pub fn remediation_action_state(option: &RemediationOption) -> WebAction {
    WebAction {
        action: option.id.clone(),
        label: Some(option.label.clone()),
        destructive: false,
        confirmation_required: false,
    }
}

pub fn browser_actions(card: &TaskCard) -> Vec<WebAction> {
    let mut actions: Vec<WebAction> = card
        .remediations
        .iter()
        .map(remediation_action_state)
        .collect();

    actions.extend(
        card.available_actions
            .iter()
            .copied()
            .filter_map(web_action),
    );
    let mut seen = std::collections::HashSet::new();
    actions.retain(|action| seen.insert(action.action.clone()));
    actions
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    matches!(
        action,
        OperatorAction::Review
            | OperatorAction::Ship
            | OperatorAction::Repair
            | OperatorAction::Drop
    )
}

pub fn supported_browser_action(action: &str) -> bool {
    if remediate::is_remediation_action(action) {
        return true;
    }
    OperatorAction::from_label(action).is_some_and(supported_web_action)
}

#[cfg(test)]
mod tests {
    use super::{browser_actions, supported_browser_action, supported_web_action, web_action};
    use ajax_core::{
        models::{
            LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction, SideFlag, TaskId,
        },
        output::TaskCard,
        remediation::FIX_CI,
        slices::remediate,
    };

    #[test]
    fn resume_action_needs_terminal_in_web_cockpit() {
        let state = web_action(OperatorAction::Resume);

        assert_eq!(state, None);
        assert!(!supported_web_action(OperatorAction::Resume));
    }

    #[test]
    fn review_action_is_supported_in_web_cockpit() {
        let state = web_action(OperatorAction::Review).unwrap();

        assert_eq!(state.action, "review");
        assert!(supported_web_action(OperatorAction::Review));
    }

    #[test]
    fn browser_action_states_do_not_surface_resume_or_sync_in_web_cockpit() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            status: ajax_core::ui_state::TaskStatus::Running,
            status_explanation: Some("Agent working".to_string()),
            lifecycle: LifecycleStatus::Active,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume, OperatorAction::Review],
            remediations: Vec::new(),
        };

        let states = browser_actions(&card);

        assert!(!states.iter().any(|state| state.action == "resume"));
        assert!(!states.iter().any(|state| state.action == "sync"));
        assert!(states.iter().any(|state| state.action == "review"));
        assert!(!supported_browser_action("sync"));
    }

    #[test]
    fn browser_action_states_surface_remediation_buttons_as_supported() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            status: ajax_core::ui_state::TaskStatus::Error,
            status_explanation: Some("CI failed".to_string()),
            lifecycle: LifecycleStatus::Error,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume],
            remediations: remediate::remediations_for_task(&blocked_ci_task()),
        };

        let states = browser_actions(&card);
        let fix_ci = states
            .iter()
            .find(|state| state.action == FIX_CI)
            .expect("fix-ci action");

        assert_eq!(fix_ci.label.as_deref(), Some("Fix CI"));
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
