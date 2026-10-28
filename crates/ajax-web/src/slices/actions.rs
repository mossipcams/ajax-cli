use ajax_core::{
    commands::{merge_task_plan, CommandContext},
    models::OperatorAction,
    output::TaskCard,
    registry::Registry,
};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct WebAction {
    pub action: String,
    pub label: String,
    pub destructive: bool,
    pub confirmation_required: bool,
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
    let mut state = web_action(action)?;
    if action == OperatorAction::Ship {
        state.confirmation_required = merge_task_plan(context, &card.qualified_handle)
            .is_ok_and(|plan| plan.requires_confirmation);
    }
    Some(state)
}

pub fn supported_web_action(action: OperatorAction) -> bool {
    matches!(
        action,
        OperatorAction::Review
            | OperatorAction::Ship
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
        remediation::{CREATE_PR, FIX_CI},
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
            attention: ajax_core::ui_state::AttentionBand::Active,
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
            attention: ajax_core::ui_state::AttentionBand::NeedsYou,
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

    #[test]
    fn create_pr_remediation_is_a_supported_browser_action() {
        let mut task = crate::test_support::fix_login_task();
        task.set_skip_interactive_agent(true);
        let remediations = ajax_core::remediation::remediations_for_task(&task);
        assert!(remediations.iter().any(|option| option.id == CREATE_PR));

        assert!(supported_browser_action(CREATE_PR));
        assert!(!supported_browser_action("sync"));
    }

    #[test]
    fn ship_action_requires_confirmation_only_when_task_has_side_flags() {
        let mut unpushed_task = crate::test_support::fix_login_task();
        unpushed_task.lifecycle_status = LifecycleStatus::Reviewable;
        unpushed_task.add_side_flag(SideFlag::Unpushed);
        let unpushed_context =
            crate::test_support::context_with_tasks(&["web"], vec![unpushed_task.clone()]);
        let unpushed_card = reviewable_ship_card(&unpushed_task);

        let unpushed_states = browser_actions(&unpushed_context, &unpushed_card);
        let ship = unpushed_states
            .iter()
            .find(|state| state.action == "ship")
            .expect("ship action");

        assert!(!ship.destructive);
        assert!(ship.confirmation_required);

        let mut clean_task = crate::test_support::fix_login_task();
        clean_task.lifecycle_status = LifecycleStatus::Reviewable;
        let clean_context =
            crate::test_support::context_with_tasks(&["web"], vec![clean_task.clone()]);
        let clean_card = reviewable_ship_card(&clean_task);

        let clean_states = browser_actions(&clean_context, &clean_card);
        let ship = clean_states
            .iter()
            .find(|state| state.action == "ship")
            .expect("ship action");

        assert!(!ship.confirmation_required);
    }

    fn reviewable_ship_card(task: &ajax_core::models::Task) -> TaskCard {
        TaskCard {
            id: task.id.clone(),
            qualified_handle: task.qualified_handle(),
            title: task.title.clone(),
            status: ajax_core::ui_state::TaskStatus::Idle,
            status_explanation: None,
            lifecycle: LifecycleStatus::Reviewable,
            last_activity_at: std::time::UNIX_EPOCH,
            annotations: Vec::new(),
            primary_action: OperatorAction::Ship,
            available_actions: vec![OperatorAction::Ship],
            remediations: Vec::new(),
            attention: ajax_core::ui_state::AttentionBand::NeedsYou,
        }
    }

    fn blocked_ci_task() -> ajax_core::models::Task {
        let mut task = crate::test_support::fix_login_task();
        task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
        task.add_side_flag(SideFlag::TestsFailed);
        task
    }
}
