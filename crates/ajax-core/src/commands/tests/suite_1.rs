use super::super::*;
use super::*;

#[test]
fn repos_include_task_counts_by_repo() {
    let context = context_with_tasks();

    let response = list_repos(&context);

    assert_eq!(response.repos.len(), 2);
    assert_eq!(response.repos[0].name, "web");
    assert_eq!(response.repos[0].reviewable_tasks, 1);
    assert_eq!(response.repos[1].name, "api");
    assert_eq!(response.repos[1].active_tasks, 0);
}

#[test]
fn missing_substrate_tasks_are_not_counted_as_active() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::WorktreeMissing);

    let response = list_repos(&context);

    assert_eq!(response.repos[0].active_tasks, 0);
}

#[test]
fn repo_attention_count_includes_broken_visible_tasks() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::Conflicted);

    let response = list_repos(&context);

    assert_eq!(response.repos[0].attention_items, 1);
}

#[test]
fn repo_attention_count_includes_visible_missing_substrate_tasks() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TmuxMissing);

    let response = list_repos(&context);

    assert_eq!(response.repos[0].attention_items, 1);
}

#[test]
fn cockpit_summary_attention_includes_visible_missing_substrate_tasks() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.lifecycle_status = LifecycleStatus::Active;
    task.add_side_flag(SideFlag::TmuxMissing);

    let response = cockpit(&context);

    assert_eq!(response.summary.attention_items, 1);
}

#[test]
fn repo_counts_include_active_and_attention_work() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::Stale);

    let response = list_repos(&context);

    assert_eq!(response.repos[0].active_tasks, 1);
    assert_eq!(response.repos[0].attention_items, 0);
}

#[test]
fn repo_attention_count_counts_tasks_once() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.add_side_flag(SideFlag::Conflicted);

    let response = list_repos(&context);

    assert_eq!(response.repos[0].attention_items, 1);
}

#[test]
fn tasks_can_be_filtered_by_repo() {
    let context = context_with_tasks();

    let all_tasks = list_tasks(&context, None);
    let web_tasks = list_tasks(&context, Some("web"));
    let api_tasks = list_tasks(&context, Some("api"));

    assert_eq!(all_tasks.tasks.len(), 1);
    assert_eq!(web_tasks.tasks.len(), 1);
    assert_eq!(api_tasks.tasks.len(), 0);
}

#[test]
fn missing_substrate_tasks_remain_visible_in_task_lists() {
    let mut context = context_with_tasks();
    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .add_side_flag(SideFlag::WorktreeMissing);

    let response = list_tasks(&context, None);

    assert_eq!(response.tasks.len(), 1);
    assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
}

#[test]
fn list_repos_scans_registry_once() {
    let context = counting_context_with_tasks();

    let response = list_repos(&context);

    assert_eq!(response.repos.len(), 2);
    assert_eq!(context.registry.list_tasks_calls(), 1);
}

#[test]
fn task_summary_marks_live_attention_without_side_flags() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(crate::models::LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for approval",
    ));

    let response = list_tasks(&context, None);

    assert!(response.tasks[0].needs_attention);
}

#[test]
fn task_summary_and_inbox_ignore_stale_cached_annotations() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.annotations = vec![Annotation::new(
        AnnotationKind::NeedsMe,
        Evidence::SideFlag(SideFlag::NeedsInput),
    )];

    let tasks = list_tasks(&context, None);
    let inbox = inbox(&context);

    assert!(!tasks.tasks[0].needs_attention);
    assert!(inbox.items.is_empty());
}

#[test]
fn cockpit_inbox_lists_unacknowledged_reviewable_tasks() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.remove_side_flag(SideFlag::NeedsInput);

    assert_eq!(review_queue(&context).tasks.len(), 1);
    let items = cockpit_inbox(&context).items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].reason, "Ready for review");
}

fn claude_waiting_context() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.selected_agent = AgentClient::Claude;
    task.lifecycle_status = LifecycleStatus::Active;
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.add_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForInput,
        "waiting for input",
    ));
    task.live_status_observed_at =
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(800));
    task.annotations = crate::attention::annotate(task);
    context
}

#[test]
fn cockpit_inbox_excludes_acknowledged_claude_waiting_task() {
    let mut context = claude_waiting_context();
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(900);

    mark_task_opened_at(&mut context, "web/fix-login", at).unwrap();

    assert!(cockpit_inbox(&context).items.is_empty());
    let tasks = list_tasks(&context, None);
    let summary = tasks
        .tasks
        .iter()
        .find(|task| task.qualified_handle == "web/fix-login")
        .expect("task remains visible in its repo");
    assert!(!summary.needs_attention);
}

#[test]
fn cockpit_inbox_reincludes_task_after_new_waiting_evidence() {
    let mut context = claude_waiting_context();
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(900);
    mark_task_opened_at(&mut context, "web/fix-login", at).unwrap();
    assert!(cockpit_inbox(&context).items.is_empty());

    // New waiting evidence after the acknowledgment.
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        crate::live::apply_observation(
            task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        );
        task.annotations = crate::attention::annotate(task);
    }

    let items = cockpit_inbox(&context).items;
    assert_eq!(
        items
            .iter()
            .filter(|item| item.task_handle == "web/fix-login")
            .count(),
        1
    );
}

#[rstest]
#[case(LiveStatusKind::WaitingForInput, "waiting_for_input")]
#[case(LiveStatusKind::WaitingForApproval, "waiting_for_approval")]
#[case(LiveStatusKind::CommandFailed, "command_failed")]
#[case(LiveStatusKind::Blocked, "blocked")]
#[case(LiveStatusKind::MergeConflict, "merge_conflict")]
#[case(LiveStatusKind::CiFailed, "ci_failed")]
fn cockpit_inbox_lists_waiting_and_blocker_live_statuses(
    #[case] live_status: LiveStatusKind,
    #[case] expected_reason: &str,
) {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.remove_side_flag(SideFlag::NeedsInput);
    task.live_status = Some(LiveObservation::new(live_status, expected_reason));

    let response = cockpit_inbox(&context);

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].task_handle, "web/fix-login");
    assert_eq!(response.items[0].reason, expected_reason);
}

#[test]
fn task_summaries_expose_lifecycle_aware_actions() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.lifecycle_status = LifecycleStatus::Active;

    let active = list_tasks(&context, None);
    assert_eq!(
        active.tasks[0].actions,
        vec![
            OperatorAction::Resume.as_str().to_string(),
            OperatorAction::Drop.as_str().to_string(),
        ]
    );

    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Reviewable;
    let reviewable = list_tasks(&context, None);
    assert_eq!(
        reviewable.tasks[0].actions,
        vec![
            OperatorAction::Resume.as_str().to_string(),
            OperatorAction::Ship.as_str().to_string(),
            OperatorAction::Drop.as_str().to_string(),
        ]
    );

    context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap()
        .lifecycle_status = LifecycleStatus::Cleanable;
    let cleanable = list_tasks(&context, None);
    assert_eq!(
        cleanable.tasks[0].actions,
        vec![
            OperatorAction::Resume.as_str().to_string(),
            OperatorAction::Drop.as_str().to_string(),
        ]
    );
}

#[test]
fn task_summaries_expose_drop_for_invalid_task_evidence() {
    for flag in [SideFlag::TmuxMissing, SideFlag::TaskWindowMissing] {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(flag);

        let response = list_tasks(&context, None);

        assert_eq!(
            response.tasks[0].actions,
            vec![OperatorAction::Drop.as_str().to_string()],
            "{flag:?}"
        );
        assert_eq!(
            inbox(&context).items[0].action,
            OperatorAction::Drop,
            "{flag:?}"
        );
    }
}

#[test]
fn removed_tasks_are_hidden_from_operational_summaries() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Removed;
    task.add_side_flag(SideFlag::WorktreeMissing);
    task.add_side_flag(SideFlag::BranchMissing);
    task.live_status = Some(crate::models::LiveObservation::new(
        LiveStatusKind::WorktreeMissing,
        "worktree missing",
    ));

    assert!(list_tasks(&context, None).tasks.is_empty());
    assert!(inbox(&context).items.is_empty());
}

#[test]
fn missing_substrate_tasks_are_visible_but_not_actionable() {
    for flag in [
        SideFlag::WorktreeMissing,
        SideFlag::BranchMissing,
        SideFlag::TmuxMissing,
        SideFlag::TaskWindowMissing,
    ] {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(flag);

        assert_eq!(list_tasks(&context, None).tasks.len(), 1, "{flag:?}");
        assert_eq!(review_queue(&context).tasks.len(), 1, "{flag:?}");
        assert_eq!(inbox(&context).items.len(), 1, "{flag:?}");
        assert_eq!(cockpit(&context).tasks.tasks.len(), 1, "{flag:?}");
        assert_eq!(list_repos(&context).repos[0].active_tasks, 0, "{flag:?}");
        assert_eq!(
            list_repos(&context).repos[0].reviewable_tasks,
            1,
            "{flag:?}"
        );
    }
}

#[test]
fn review_queue_lists_reviewable_and_mergeable_tasks() {
    let mut context = context_with_tasks();
    let mut mergeable = Task::new(
        TaskId::new("task-2"),
        "api",
        "add-cache",
        "Add cache",
        "ajax/add-cache",
        "main",
        "/tmp/worktrees/api-add-cache",
        "ajax-api-add-cache",
        "task",
        AgentClient::Claude,
    );
    mergeable.lifecycle_status = LifecycleStatus::Mergeable;
    context.registry.create_task(mergeable).unwrap();

    let response = review_queue(&context);

    assert_eq!(response.tasks.len(), 2);
    assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
    assert_eq!(response.tasks[1].qualified_handle, "api/add-cache");
}

#[test]
fn review_slice_facade_lists_reviewable_and_mergeable_tasks() {
    let mut context = context_with_tasks();
    let mut mergeable = Task::new(
        TaskId::new("task-2"),
        "api",
        "add-cache",
        "Add cache",
        "ajax/add-cache",
        "main",
        "/tmp/worktrees/api-add-cache",
        "ajax-api-add-cache",
        "task",
        AgentClient::Claude,
    );
    mergeable.lifecycle_status = LifecycleStatus::Mergeable;
    context.registry.create_task(mergeable).unwrap();

    assert_eq!(review_queue(&context), review_queue(&context));
}

#[test]
fn cockpit_includes_review_queue() {
    let mut context = context_with_tasks();
    let mut mergeable = Task::new(
        TaskId::new("task-2"),
        "api",
        "add-cache",
        "Add cache",
        "ajax/add-cache",
        "main",
        "/tmp/worktrees/api-add-cache",
        "ajax-api-add-cache",
        "task",
        AgentClient::Claude,
    );
    mergeable.lifecycle_status = LifecycleStatus::Mergeable;
    context.registry.create_task(mergeable).unwrap();

    let response = cockpit(&context);

    assert_eq!(response.review.tasks.len(), 2);
    assert_eq!(response.review.tasks[0].qualified_handle, "web/fix-login");
    assert_eq!(response.review.tasks[1].qualified_handle, "api/add-cache");
}

#[test]
fn cockpit_inbox_includes_unacknowledged_reviewable_tasks() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.remove_side_flag(SideFlag::NeedsInput);

    let view = cockpit_view(&context);

    assert_eq!(view.inbox.items.len(), 1);
    assert_eq!(view.inbox.items[0].reason, "Ready for review");
    assert_eq!(view.cards.len(), 1);
    assert_eq!(view.cards[0].qualified_handle, "web/fix-login");
}

#[test]
fn cockpit_view_includes_missing_substrate_tasks_as_drop_only_cards() {
    let mut context = context_with_tasks();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("task-1"))
        .unwrap();
    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::TmuxMissing);

    let view = cockpit_view(&context);

    assert_eq!(view.cards.len(), 1);
    assert_eq!(view.cards[0].qualified_handle, "web/fix-login");
    assert_eq!(view.cards[0].primary_action, OperatorAction::Drop);
    assert_eq!(view.cards[0].available_actions, vec![OperatorAction::Drop]);
}

#[test]
fn cockpit_scans_registry_once() {
    let context = counting_context_with_tasks();

    let response = cockpit(&context);

    assert_eq!(response.summary.tasks, 1);
    assert_eq!(response.inbox.items.len(), 1);
    assert_eq!(context.registry.list_tasks_calls(), 1);
}

#[test]
fn cockpit_projection_scans_registry_once() {
    let context = counting_context_with_tasks();

    let response = cockpit_projection(&context);

    assert_eq!(response.counts.tasks, 1);
    assert_eq!(response.cards.len(), 1);
    assert_eq!(context.registry.list_tasks_calls(), 1);
}

#[test]
fn cockpit_view_scans_registry_once_for_repos_cards_and_inbox() {
    let context = counting_context_with_tasks();

    let view = cockpit_view(&context);

    assert_eq!(view.repos.repos.len(), 2);
    assert_eq!(view.cards.len(), 1);
    assert_eq!(view.inbox.items.len(), 1);
    assert_eq!(context.registry.list_tasks_calls(), 1);
}

#[test]
fn cockpit_summary_counts_operator_work() {
    let mut context = context_with_tasks();
    let mut cleanable = Task::new(
        TaskId::new("task-2"),
        "api",
        "remove-cache",
        "Remove cache",
        "ajax/remove-cache",
        "main",
        "/tmp/worktrees/api-remove-cache",
        "ajax-api-remove-cache",
        "task",
        AgentClient::Claude,
    );
    cleanable.lifecycle_status = LifecycleStatus::Cleanable;
    context.registry.create_task(cleanable).unwrap();

    let response = cockpit(&context);

    assert_eq!(
        response.summary,
        CockpitSummary {
            repos: 2,
            tasks: 2,
            active_tasks: 0,
            attention_items: 1,
            reviewable_tasks: 1,
            cleanable_tasks: 1,
        }
    );
}

#[test]
fn cockpit_next_matches_next_command() {
    let context = context_with_tasks();

    let response = cockpit(&context);

    assert_eq!(response.next, next(&context));
}

#[test]
fn inspect_returns_task_details_by_qualified_handle() {
    let context = context_with_tasks();

    let response = inspect_task(&context, "web/fix-login").unwrap();

    assert_eq!(response.task.qualified_handle, "web/fix-login");
    assert_eq!(response.branch, "ajax/fix-login");
    assert_eq!(response.tmux_session, "ajax-web-fix-login");
    assert_eq!(response.flags, vec!["NeedsInput"]);
}

#[test]
fn inspect_reports_missing_tasks() {
    let context = context_with_tasks();

    let error = inspect_task(&context, "web/missing").unwrap_err();

    assert_eq!(error, CommandError::TaskNotFound("web/missing".to_string()));
}

#[test]
fn inbox_returns_canonical_status_items() {
    let context = context_with_tasks();

    let response = inbox(&context);

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].task_handle, "web/fix-login");
    assert_eq!(response.items[0].reason, "needs_input");
    assert_eq!(response.items[0].severity, 1);
    assert_eq!(response.items[0].action, OperatorAction::Resume);
}

#[test]
fn next_returns_first_canonical_status_item() {
    let context = context_with_tasks();

    let response = next(&context);

    let item = response.item.unwrap();
    assert_eq!(item.task_handle, "web/fix-login");
    assert_eq!(item.reason, "needs_input");
}

#[test]
fn doctor_and_status_return_basic_health() {
    let mut context = context_with_tasks();
    context.config.test_commands = vec![
        TestCommand::new("web", "cargo test"),
        TestCommand::new("api", "cargo test"),
    ];
    // A healthy host also has the ACP adapters that back browser sessions.
    let environment = DoctorEnvironment::from_available_tools([
        "git",
        "tmux",
        "codex",
        "codex-acp",
        "claude-agent-acp",
        "pi-acp",
    ])
    .with_existing_paths(["/Users/matt/projects/web", "/Users/matt/projects/api"]);

    let doctor = doctor_with_environment(&context, &environment);
    let status = status(&context);

    assert!(doctor.checks.iter().all(|check| check.ok));
    assert_eq!(status.tasks.len(), 1);
}

// Sessions for Codex, Claude, and Pi need their Agent Client Protocol adapters;
// a missing one is an install the operator can do, so name it.
#[test]
fn doctor_names_the_missing_acp_adapter_package() {
    let context = context_with_tasks();
    let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"]);

    let doctor = doctor_with_environment(&context, &environment);

    for (agent, package) in [
        ("codex", "@agentclientprotocol/codex-acp"),
        ("claude", "@agentclientprotocol/claude-agent-acp"),
        ("pi", "pi-acp"),
    ] {
        let check = doctor
            .checks
            .iter()
            .find(|check| check.name == format!("acp:{agent}"))
            .unwrap_or_else(|| panic!("expected an acp:{agent} check"));
        assert!(!check.ok, "{agent} adapter should report missing");
        assert!(
            check.message.contains(package),
            "{agent} check should name {package}: {}",
            check.message
        );
    }
}

#[test]
fn doctor_reports_required_tool_availability() {
    let context = context_with_tasks();
    let environment = DoctorEnvironment::from_available_tools(["git", "tmux"]);

    let doctor = doctor_with_environment(&context, &environment);

    assert_eq!(
        doctor
            .checks
            .iter()
            .find(|check| check.name == "tool:git")
            .map(|check| (check.ok, check.message.as_str())),
        Some((true, "available"))
    );
    assert_eq!(
        doctor
            .checks
            .iter()
            .find(|check| check.name == "tool:codex")
            .map(|check| (check.ok, check.message.as_str())),
        Some((false, "not found on PATH"))
    );
}
