//! Shared browser action capability vocabulary for Web Cockpit slices.

use ajax_core::{
    commands::{BranchAdoptionPlan, CommandContext, OpenMode},
    models::OperatorAction,
    output::TaskCard,
    registry::Registry,
    task_operations::task_command::{plan_task_command_operation, TaskCommandKind},
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebAction {
    pub action: String,
    pub label: String,
    pub destructive: bool,
    pub confirmation_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_adoption: Option<BranchAdoptionPlan>,
}

pub fn web_action(action: OperatorAction) -> Option<WebAction> {
    if !supported_web_action(action) {
        return None;
    }
    Some(WebAction {
        action: action.as_str().to_string(),
        label: operator_action_label(action).to_string(),
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
        branch_adoption: None,
    })
}

fn operator_action_label(action: OperatorAction) -> &'static str {
    match action {
        OperatorAction::Start => "Start",
        OperatorAction::Resume => "Resume",
        OperatorAction::Review => "Review",
        OperatorAction::Ship => "Ship",
        OperatorAction::Drop => "Drop",
        OperatorAction::Repair => "Repair",
    }
}

pub fn remediation_action_state(option: &ajax_core::remediation::RemediationOption) -> WebAction {
    WebAction {
        action: option.id.clone(),
        label: option.label.clone(),
        destructive: false,
        confirmation_required: false,
        branch_adoption: None,
    }
}

pub fn browser_actions<R: Registry>(
    context: &CommandContext<R>,
    card: &TaskCard,
) -> Vec<WebAction> {
    let mut actions: Vec<WebAction> = card
        .remediations
        .iter()
        .map(remediation_action_state)
        .collect();

    actions.extend(
        card.available_actions
            .iter()
            .copied()
            .filter_map(|action| web_action_for_card(context, card, action)),
    );
    let mut seen = std::collections::HashSet::new();
    actions.retain(|action| seen.insert(action.action.clone()));
    actions
}

fn web_action_for_card<R: Registry>(
    context: &CommandContext<R>,
    card: &TaskCard,
    action: OperatorAction,
) -> Option<WebAction> {
    if !supported_web_action(action) {
        return None;
    }
    let mut state = WebAction {
        action: action.as_str().to_string(),
        label: operator_action_label(action).to_string(),
        destructive: action == OperatorAction::Drop,
        confirmation_required: action == OperatorAction::Drop,
        branch_adoption: None,
    };
    if action == OperatorAction::Repair {
        if let Ok(plan) = plan_task_command_operation(
            context,
            TaskCommandKind::Repair,
            &card.qualified_handle,
            OpenMode::NoAttach,
        ) {
            state.confirmation_required = plan.requires_confirmation;
            state.branch_adoption = plan.branch_adoption;
        }
    }
    Some(state)
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    matches!(
        action,
        OperatorAction::Review
            | OperatorAction::Ship
            | OperatorAction::Repair
            | OperatorAction::Drop
            | OperatorAction::Resume
    )
}

pub fn supported_browser_action(action: &str) -> bool {
    if ajax_core::remediation::is_remediation_action(action) {
        return true;
    }
    OperatorAction::from_label(action).is_some_and(supported_web_action)
}

#[cfg(test)]
mod tests {
    use super::{browser_actions, supported_browser_action, supported_web_action, web_action};
    use ajax_core::{
        commands::CommandContext,
        config::Config,
        models::{
            LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction, SideFlag, TaskId,
        },
        output::TaskCard,
        registry::InMemoryRegistry,
        remediation::FIX_CI,
    };

    #[test]
    fn resume_action_is_supported_in_web_cockpit() {
        let state = web_action(OperatorAction::Resume).unwrap();

        assert_eq!(state.action, "resume");
        assert_eq!(state.label, "Resume");
        assert!(supported_web_action(OperatorAction::Resume));
    }

    #[test]
    fn review_action_is_supported_in_web_cockpit() {
        let state = web_action(OperatorAction::Review).unwrap();

        assert_eq!(state.action, "review");
        assert_eq!(state.label, "Review");
        assert!(supported_web_action(OperatorAction::Review));
    }

    #[test]
    fn browser_action_states_surface_resume_but_not_sync_in_web_cockpit() {
        let card = TaskCard {
            id: TaskId::new("web/fix-login"),
            qualified_handle: "web/fix-login".to_string(),
            title: "Fix login".to_string(),
            status: ajax_core::ui_state::TaskStatus::Running,
            status_explanation: Some("Agent working".to_string()),
            lifecycle: LifecycleStatus::Active,
            last_activity_at: std::time::UNIX_EPOCH,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume, OperatorAction::Review],
            remediations: Vec::new(),
        };
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let states = browser_actions(&context, &card);

        assert!(states.iter().any(|state| state.action == "resume"));
        assert!(states.iter().any(|state| state.action == "review"));
        assert!(!states.iter().any(|state| state.action == "sync"));
        assert!(supported_browser_action("resume"));
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
            last_activity_at: std::time::UNIX_EPOCH,
            annotations: Vec::new(),
            primary_action: OperatorAction::Resume,
            available_actions: vec![OperatorAction::Resume],
            remediations: ajax_core::remediation::remediations_for_task(&blocked_ci_task()),
        };
        let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

        let states = browser_actions(&context, &card);
        let fix_ci = states
            .iter()
            .find(|state| state.action == FIX_CI)
            .expect("fix-ci action");

        assert_eq!(fix_ci.label, "Fix CI");
        assert!(supported_browser_action(FIX_CI));
    }

    fn blocked_ci_task() -> ajax_core::models::Task {
        let mut task = crate::test_support::fix_login_task();
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);
        task
    }
}
