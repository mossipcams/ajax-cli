use super::super::*;
use super::*;

fn context_with_live_task(stored_kind: LiveStatusKind) -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let task = task_with_live(stored_kind, "waiting for input");
    let mut registry = InMemoryRegistry::default();
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

fn delegated_child_obs() -> StatusObservation {
    let observed_at = SystemTime::now() - Duration::from_secs(1);
    StatusObservation {
        source: ObservationSource::ProviderLifecycle,
        observed_at,
        expires_at: observed_at + Duration::from_secs(120),
        confidence: Confidence::High,
        run_id: "child-1".to_string(),
        parent_run_id: Some(PRIMARY_RUN_ID.to_string()),
        kind: ActivityKind::Working,
    }
}

// #1355: with no primary observation, the projection is WaitingForInput with no
// selected source timestamp. The `now` fallback used to re-apply the unchanged
// status on every refresh, stamping a fresh live_status_observed_at each poll.
#[test]
fn issue_1355_untimestamped_unchanged_live_status_is_not_reapplied() {
    let mut context = context_with_live_task(LiveStatusKind::WaitingForInput);
    let cache = ObsSource::new(vec![delegated_child_obs()]);
    let mut runner = HealthyRefreshRunner::default();

    let first =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
            .unwrap();
    assert!(first, "the first refresh must apply the waiting projection");
    let stamped_at = context
        .registry
        .get_task(&TaskId::new(TASK_ID))
        .unwrap()
        .live_status_observed_at
        .expect("the first refresh stamps live_status_observed_at");

    // Ensure the next refresh's `now` is strictly newer than the stamped time.
    std::thread::sleep(Duration::from_millis(2));

    let second =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
            .unwrap();
    assert!(
        !second,
        "an unchanged untimestamped projection must not mark the context changed"
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status_observed_at,
        Some(stamped_at),
        "the original observation timestamp must be preserved"
    );
}

#[test]
fn issue_1355_kind_change_from_untimestamped_projection_still_applies() {
    let mut context = context_with_live_task(LiveStatusKind::AgentRunning);
    let cache = ObsSource::new(vec![delegated_child_obs()]);
    let mut runner = HealthyRefreshRunner::default();

    let before = SystemTime::now();
    let changed =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
            .unwrap();

    assert!(changed, "a kind change must mark the context changed");
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForInput),
        "the new kind must be applied"
    );
    let stamped_at = task
        .live_status_observed_at
        .expect("kind change stamps observed_at");
    assert!(
        stamped_at >= before,
        "observed_at falls back to now for untimestamped projections"
    );
}

#[test]
fn issue_1355_unchanged_kind_without_stored_timestamp_is_stamped() {
    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    registry
        .create_task(task_with_live(
            LiveStatusKind::WaitingForInput,
            "waiting for input",
        ))
        .unwrap();
    let mut context = CommandContext::new(config, registry);
    let cache = ObsSource::new(vec![delegated_child_obs()]);
    let mut runner = HealthyRefreshRunner::default();

    let before = SystemTime::now();
    let changed =
        refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
            .unwrap();

    assert!(
        changed,
        "a task without a stored timestamp must be stamped even when the kind is unchanged"
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    let stamped_at = task
        .live_status_observed_at
        .expect("refresh stamps a missing live_status_observed_at");
    assert!(stamped_at >= before);
}

// #1355 (second site): the missing-tmux-session branch used to re-apply
// TmuxMissing and force changed = true on every refresh, re-stamping
// live_status_observed_at for tasks whose session is gone.
#[test]
fn issue_1355_missing_tmux_session_is_not_reapplied_on_second_refresh() {
    let mut context = context_with_task_for_missing_session();
    let mut runner = MissingSessionRunner::default();

    let first = refresh_runtime_context(&mut context, &mut runner).unwrap();
    assert!(
        first,
        "the first refresh must apply TmuxMissing and mark changed"
    );
    let stamped_at = context
        .registry
        .get_task(&TaskId::new(TASK_ID))
        .unwrap()
        .live_status_observed_at
        .expect("the first refresh stamps live_status_observed_at");
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new(TASK_ID))
            .unwrap()
            .live_status
            .as_ref()
            .map(|status| status.kind),
        Some(LiveStatusKind::TmuxMissing),
        "the first refresh applies TmuxMissing"
    );

    // Ensure the next refresh's `now` is strictly newer than the stamped time.
    std::thread::sleep(Duration::from_millis(2));

    let second = refresh_runtime_context(&mut context, &mut runner).unwrap();
    assert!(
        !second,
        "an unchanged missing session must not mark the context changed"
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status_observed_at,
        Some(stamped_at),
        "the original observation timestamp must be preserved"
    );
}

#[test]
fn issue_1355_tmux_missing_applied_on_kind_transition() {
    let mut context = context_with_task_for_missing_session();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::AgentRunning,
            "agent running",
        ));
        task.live_status_observed_at = Some(SystemTime::now() - Duration::from_secs(60));
    }
    let mut runner = MissingSessionRunner::default();

    let before = SystemTime::now();
    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(
        changed,
        "a transition to TmuxMissing must mark the context changed"
    );
    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::TmuxMissing),
        "the new kind must be applied"
    );
    let stamped_at = task
        .live_status_observed_at
        .expect("kind change stamps observed_at");
    assert!(
        stamped_at >= before,
        "observed_at must be re-stamped for the new episode"
    );
}
