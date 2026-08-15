use super::{browser_cockpit_json, browser_task_card};
use ajax_core::{
    commands::CommandContext,
    config::Config,
    models::{
        GitStatus, LifecycleStatus, LiveObservation, LiveStatusKind, OperatorAction,
        RuntimeObservationSource, SideFlag, TaskId,
    },
    output::TaskCard,
    registry::{InMemoryRegistry, Registry as _},
};

// Routing depends on this: the browser opens chat only for tasks the host will
// actually attach, so the card must mark capability the same way the session
// slice admits one.
#[test]
fn cards_mark_only_provisioned_acp_tasks_as_session_capable() {
    use ajax_core::models::AgentClient;

    let mut interactive = crate::test_support::fix_login_task();
    interactive.selected_agent = AgentClient::Cursor;

    let mut provisioned = crate::test_support::task_in("web", "chat-task", "Chat task");
    provisioned.selected_agent = AgentClient::Cursor;
    provisioned.set_skip_interactive_agent(true);

    let mut no_acp = crate::test_support::task_in("web", "other-task", "Other task");
    no_acp.selected_agent = AgentClient::Other;
    no_acp.set_skip_interactive_agent(true);

    let context =
        crate::test_support::context_with_tasks(&["web"], vec![interactive, provisioned, no_acp]);
    let view = super::browser_cockpit_view(&context);
    let capable = |handle: &str| {
        view.cards
            .iter()
            .find(|card| card.qualified_handle == handle)
            .unwrap_or_else(|| panic!("expected a card for {handle}"))
            .session_capable
    };

    assert!(
        capable("web/chat-task"),
        "provisioned Cursor holds a session"
    );
    assert!(
        !capable("web/fix-login"),
        "interactive task keeps its terminal"
    );
    assert!(
        !capable("web/other-task"),
        "an agent without ACP cannot attach"
    );
}

#[test]
fn cockpit_slice_serializes_empty_projection() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["repos"]["repos"], serde_json::json!([]));
    assert_eq!(value["cards"], serde_json::json!([]));
    assert_eq!(value["inbox"]["items"], serde_json::json!([]));
    assert_eq!(value["backend"]["authority"], "host-native");
    assert_eq!(value["backend"]["control_enabled"], true);
}

#[test]
fn browser_cockpit_surfaces_missing_substrate_tasks() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TmuxMissing);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::TmuxMissing,
        "tmux session missing",
    ));
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["cards"].as_array().unwrap().len(), 1);
    assert_eq!(value["cards"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(value["cards"][0]["status"], "error");
    assert_eq!(
        value["cards"][0]["status_explanation"],
        "Tmux session missing"
    );
    assert_eq!(value["cards"][0]["actions"][0]["action"], "drop");
    for removed in [
        "ui_state",
        "status_label",
        "live_summary",
        "primary_action",
        "available_actions",
        "action_states",
    ] {
        assert!(value["cards"][0].get(removed).is_none(), "{removed}");
    }
    assert_eq!(value["inbox"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(value["inbox"]["items"][0]["task_handle"], "web/fix-login");
}

#[test]
fn browser_cockpit_keeps_removed_tasks_out_of_browser_only_cards() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::task_in("web", "old-task", "Old task");
    task.lifecycle_status = LifecycleStatus::Removed;
    task.add_side_flag(SideFlag::TmuxMissing);
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["cards"], serde_json::json!([]));
}

#[test]
fn task_detail_returns_none_for_unknown_handle() {
    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());
    let detail = super::browser_task_detail_view(&context, "web/missing");
    assert!(detail.is_none());
}

#[test]
fn task_detail_exposes_runtime_probe_failure_reason() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Active;
    registry.create_task(task).unwrap();
    registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap()
        .record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );
    let context = CommandContext::new(Config::default(), registry);

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

    assert_eq!(detail.status, ajax_core::ui_state::TaskStatus::Error);
    assert_eq!(
        detail.status_explanation.as_deref(),
        Some("Status unavailable")
    );
    assert_eq!(
        detail.runtime_observation_error.as_deref(),
        Some("tmux server unavailable")
    );
}

#[test]
fn browser_cockpit_mismatch_repair_projects_exact_adoption_confirmation() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: Some("abc123 Fix login".to_string()),
    });
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let card_repair = &value["cards"][0]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["action"] == "repair")
        .expect("repair action on card");
    assert_eq!(card_repair["confirmation_required"], true);
    assert_eq!(
        card_repair["branch_adoption"],
        serde_json::json!({
            "expected_branch": "ajax/fix-login",
            "observed_branch": "fix/pane-stuck",
        })
    );
    let card_resume = value["cards"][0]["actions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|action| action["action"] == "resume")
        .expect("resume action on card");
    assert_eq!(card_resume["confirmation_required"], false);
    assert!(card_resume.get("branch_adoption").is_none());

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();
    let detail_repair = detail
        .actions
        .iter()
        .find(|action| action.action == "repair")
        .expect("repair action in detail");
    assert!(detail_repair.confirmation_required);
    assert_eq!(
        detail_repair.branch_adoption,
        Some(ajax_core::commands::BranchAdoptionPlan {
            expected_branch: "ajax/fix-login".to_string(),
            observed_branch: "fix/pane-stuck".to_string(),
        })
    );
    let detail_resume = detail
        .actions
        .iter()
        .find(|action| action.action == "resume")
        .expect("resume action in detail");
    assert!(!detail_resume.confirmation_required);
    assert!(detail_resume.branch_adoption.is_none());
}

#[test]
fn browser_cockpit_and_detail_pass_through_checkout_mismatch() {
    const EXPLANATION: &str = "Worktree on fix/pane-stuck; expected ajax/fix-login";
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: Some("abc123 Fix login".to_string()),
    });
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let card = &value["cards"][0];

    assert_eq!(card["qualified_handle"], "web/fix-login");
    assert_eq!(card["status"], "error");
    assert_eq!(card["status_explanation"], EXPLANATION);
    assert_eq!(card["actions"][0]["action"], "repair");
    assert_eq!(card["actions"][1]["action"], "resume");
    assert_eq!(card["actions"][2]["action"], "drop");

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

    assert_eq!(detail.status, ajax_core::ui_state::TaskStatus::Error);
    assert_eq!(detail.status_explanation.as_deref(), Some(EXPLANATION));
    assert_eq!(detail.actions[0].action, "repair");
    assert_eq!(detail.actions[1].action, "resume");
    assert_eq!(detail.actions[2].action, "drop");
}

#[test]
fn task_detail_returns_missing_substrate_task_when_visible_in_cockpit() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::WorktreeMissing);
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

    assert_eq!(detail.qualified_handle, "web/fix-login");
    // A missing worktree with an intact branch is recoverable — Repair is
    // surfaced (primary), and Drop stays available as an escape hatch.
    assert_eq!(detail.actions[0].action, "repair");
    assert!(detail.actions.iter().any(|action| action.action == "drop"));
    assert_eq!(detail.status, ajax_core::ui_state::TaskStatus::Error);
    assert_eq!(
        detail.status_explanation.as_deref(),
        Some("Worktree missing")
    );
}

#[test]
fn task_detail_surfaces_structured_live_state_for_a_task() {
    use ajax_core::models::GitStatus;

    let config = crate::test_support::config_with(&["web"]);
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for review",
    ));
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 3,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    registry.create_task(task).unwrap();
    let context = CommandContext::new(config, registry);

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

    assert_eq!(detail.qualified_handle, "web/fix-login");
    assert_eq!(detail.title, "Fix login");
    assert_eq!(detail.branch, "ajax/fix-login");
    assert_eq!(detail.base_branch, "main");
    assert_eq!(detail.lifecycle, "Reviewable");
    assert_eq!(
        detail.live_status_summary.as_deref(),
        Some("waiting for review")
    );
    assert_eq!(
        detail.live_status_kind.as_deref(),
        Some("WaitingForApproval")
    );
    assert_eq!(detail.git.as_ref().map(|g| g.ahead), Some(3));
    assert!(detail.worktree_path.contains("ajax-fix-login"));
}

#[test]
fn cockpit_slice_shapes_cards_for_the_mobile_pwa() {
    let card = TaskCard {
        id: TaskId::new("web/fix-login"),
        qualified_handle: "web/fix-login".to_string(),
        title: "Fix login".to_string(),
        status: ajax_core::ui_state::TaskStatus::Waiting,
        status_explanation: Some("Ready for review".to_string()),
        lifecycle: LifecycleStatus::Reviewable,
        last_activity_at: std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
        annotations: Vec::new(),
        primary_action: OperatorAction::Resume,
        available_actions: vec![
            OperatorAction::Start,
            OperatorAction::Resume,
            OperatorAction::Review,
            OperatorAction::Ship,
        ],
        remediations: Vec::new(),
        attention: ajax_core::ui_state::AttentionBand::Review,
    };

    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let browser = browser_task_card(&context, &card);

    assert_eq!(browser.last_activity_unix_secs, 1_700_000_000);
    assert_eq!(browser.qualified_handle, "web/fix-login");
    assert_eq!(browser.status, ajax_core::ui_state::TaskStatus::Waiting);
    assert_eq!(
        browser.status_explanation.as_deref(),
        Some("Ready for review")
    );
    assert_eq!(
        browser
            .actions
            .iter()
            .map(|action| action.action.as_str())
            .collect::<Vec<_>>(),
        ["resume", "review", "ship"]
    );
}

#[test]
fn browser_task_card_surfaces_supported_fix_ci_remediation_button() {
    use ajax_core::models::{LiveObservation, LiveStatusKind, SideFlag};
    use ajax_core::remediation::FIX_CI;

    let mut source = crate::test_support::fix_login_task();
    source.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
    source.add_side_flag(SideFlag::TestsFailed);
    let card = TaskCard {
        id: source.id.clone(),
        qualified_handle: source.qualified_handle(),
        title: source.title.clone(),
        status: ajax_core::ui_state::TaskStatus::Error,
        status_explanation: Some("CI failed".to_string()),
        lifecycle: LifecycleStatus::Error,
        last_activity_at: std::time::UNIX_EPOCH,
        annotations: Vec::new(),
        primary_action: OperatorAction::Resume,
        available_actions: vec![OperatorAction::Resume],
        remediations: ajax_core::remediation::remediations_for_task(&source),
        attention: ajax_core::ui_state::AttentionBand::NeedsYou,
    };

    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let browser = browser_task_card(&context, &card);
    let fix_ci = browser
        .actions
        .iter()
        .find(|state| state.action == FIX_CI)
        .expect("fix-ci button");

    assert_eq!(fix_ci.label, "Fix CI");
    assert!(browser.actions.iter().any(|action| action.action == FIX_CI));
}

#[test]
fn cockpit_cards_expose_only_executable_web_actions() {
    let card = TaskCard {
        id: TaskId::new("web/fix-login"),
        qualified_handle: "web/fix-login".to_string(),
        title: "Fix login".to_string(),
        status: ajax_core::ui_state::TaskStatus::Waiting,
        status_explanation: Some("Ready for review".to_string()),
        lifecycle: LifecycleStatus::Reviewable,
        last_activity_at: std::time::UNIX_EPOCH,
        annotations: Vec::new(),
        primary_action: OperatorAction::Resume,
        available_actions: vec![
            OperatorAction::Resume,
            OperatorAction::Review,
            OperatorAction::Drop,
        ],
        remediations: Vec::new(),
        attention: ajax_core::ui_state::AttentionBand::Review,
    };

    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let browser = browser_task_card(&context, &card);
    let states: Vec<(&str, &str, bool, bool)> = browser
        .actions
        .iter()
        .map(|state| {
            (
                state.action.as_str(),
                state.label.as_str(),
                state.destructive,
                state.confirmation_required,
            )
        })
        .collect();

    assert_eq!(
        states,
        vec![
            ("resume", "Resume", false, false),
            ("review", "Review", false, false),
            ("drop", "Drop", true, true),
        ]
    );
}

#[test]
fn browser_card_exposes_explicit_repo_identity() {
    let card = TaskCard {
        id: TaskId::new("web/fix-login"),
        qualified_handle: "web/fix-login".to_string(),
        title: "Fix login".to_string(),
        status: ajax_core::ui_state::TaskStatus::Waiting,
        status_explanation: None,
        lifecycle: LifecycleStatus::Reviewable,
        last_activity_at: std::time::UNIX_EPOCH,
        annotations: Vec::new(),
        primary_action: OperatorAction::Review,
        available_actions: vec![OperatorAction::Review],
        remediations: Vec::new(),
        attention: ajax_core::ui_state::AttentionBand::Review,
    };

    let context = CommandContext::new(Config::default(), InMemoryRegistry::default());

    let browser = browser_task_card(&context, &card);

    // The browser must not split `qualified_handle` to learn the repo.
    assert_eq!(browser.repo, "web");
}

#[test]
fn browser_detail_exposes_repo_and_server_actions() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for review",
    ));
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let detail = super::browser_task_detail_view(&context, "web/fix-login").unwrap();

    assert_eq!(detail.repo, "web");
    assert!(
        !detail.actions.is_empty(),
        "detail should expose server-provided actions"
    );
}

#[test]
fn browser_contract_fixture_has_stable_card_shape() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let card = &value["cards"][0];

    // Explicit repo identity is part of the browser contract.
    assert_eq!(card["repo"], "web");
    assert_eq!(card["qualified_handle"], "web/fix-login");
    // Actions remain the sole capability list and carry no `status` field.
    assert!(card["actions"].is_array());
    for action in card["actions"].as_array().unwrap() {
        assert!(
            action.get("status").is_none(),
            "WebAction must not expose a `status` field"
        );
    }
}

#[test]
fn browser_cockpit_json_carries_attention_band() {
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    registry.create_task(task).unwrap();
    let context = CommandContext::new(Config::default(), registry);

    let json = browser_cockpit_json(&context).unwrap();

    assert!(
        json.contains("\"attention\":\"review\""),
        "expected review band in cockpit json: {json}"
    );
}

#[test]
fn committed_cockpit_fixture_matches_production_serialization() {
    let context = browser_contract_context();
    let actual = serde_json::to_value(super::browser_cockpit_view(&context)).unwrap();
    let committed: serde_json::Value =
        serde_json::from_str(include_str!("../../../web/src/fixtures/cockpit.json")).unwrap();

    assert_eq!(committed, actual);
}

#[test]
fn committed_task_detail_fixture_matches_production_serialization() {
    let context = browser_contract_context();
    let actual =
        serde_json::to_value(super::browser_task_detail_view(&context, "web/fix-login").unwrap())
            .unwrap();
    let committed: serde_json::Value =
        serde_json::from_str(include_str!("../../../web/src/fixtures/task-detail.json")).unwrap();

    assert_eq!(committed, actual);
}

pub(crate) fn browser_contract_context() -> CommandContext<InMemoryRegistry> {
    use ajax_core::models::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind,
    };
    use std::time::{Duration, SystemTime};

    let config = crate::test_support::config_with(&["web"]);
    let mut registry = InMemoryRegistry::default();
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for review",
    ));
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.created_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    task.last_activity_at = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_001_000);
    task.agent_attempts.push(AgentAttempt {
        agent: AgentClient::Codex,
        launch_target: "task".to_string(),
        started_at: SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
        finished_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_001_000)),
        status: AgentRuntimeStatus::Done,
    });
    registry.create_task(task).unwrap();
    CommandContext::new(config, registry)
}
