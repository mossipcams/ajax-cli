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

// #1457: Mergeable tasks were never probed, so their agent status observations
// (e.g. a new turn after CI turned green) were never folded.
#[test]
fn issue_1457_idle_mergeable_task_folds_turn_started_observation() {
    let mut context = context_with_active_task();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Mergeable;
        task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
        task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            SystemTime::now(),
            RuntimeObservationSource::TmuxProbe,
        );
    }
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);
    let mut runner = HealthyRefreshRunner::default();

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Live)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.agent_status,
        AgentRuntimeStatus::Running,
        "a TurnStarted observation must fold into the Mergeable task's agent status"
    );
    assert_eq!(
        task.lifecycle_status,
        LifecycleStatus::Active,
        "the agent running observation must move Mergeable back to Active"
    );
}

const MERGED_PR_JSON: &str = r#"[{"number":42,"title":"Fix login","url":"https://example.test/pull/42","state":"MERGED","headRefName":"ajax/fix-login","headRefOid":"2222222"}]"#;

// #1456: the CI/PR monitor (the only path that moves a task to Merged when its PR
// merges) used to run only after the tmux probe, so a failed list-sessions returned
// early and skipped it.
struct TmuxSessionsFailRunner {
    prs: String,
}

impl CommandRunner for TmuxSessionsFailRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let (status_code, stdout, stderr) = match command.program.as_str() {
            "gh" => (0, self.prs.clone(), String::new()),
            _ if arg(&command.args, 0) == "list-sessions" => (1, String::new(), "boom".to_string()),
            _ => (0, runtime_stdout(&command.args).to_string(), String::new()),
        };
        Ok(CommandOutput {
            status_code,
            stdout,
            stderr,
        })
    }
}

#[test]
fn issue_1456_full_refresh_monitors_merged_pr_when_tmux_probe_fails() {
    let mut context = context_with_active_task();
    {
        let task = context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Mergeable;
        task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
        task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
        task.metadata
            .remove(crate::agent_notification::CI_MONITOR_STATE_KEY);
    }
    let mut runner = TmuxSessionsFailRunner {
        prs: MERGED_PR_JSON.to_string(),
    };

    refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new(TASK_ID))
            .unwrap()
            .lifecycle_status,
        LifecycleStatus::Merged,
        "a Full refresh must run the CI monitor even when the tmux probe fails"
    );
}

// #1456: when no task is probe-eligible the refresh used to return before the CI
// monitor. Merged/Cleanable tasks are retired by the monitor (CI evidence cleared).
fn context_with_merged_task_holding_ci_failure() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Merged;
    crate::runtime_refresh::ci_monitor::store_state(
        task,
        &crate::runtime_refresh::ci_monitor::CiMonitorState {
            pr_number: Some(42),
            head_sha: Some("2222222".into()),
            status: crate::runtime_refresh::ci_monitor::CiAttemptStatus::Failed,
            episode_id: Some("ci-failed:task-1:42:2222222:run-1".into()),
            ..Default::default()
        },
    );
    context
}

#[test]
fn issue_1456_full_refresh_runs_ci_monitor_when_no_task_is_probe_eligible() {
    let mut context = context_with_merged_task_holding_ci_failure();
    let mut runner = HealthyRefreshRunner::default();

    refresh_runtime_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(
        crate::runtime_refresh::ci_monitor::load_state(task)
            .episode_id
            .is_none(),
        "a Full refresh must run the CI monitor, which retires the Merged task's CI evidence"
    );
}

#[test]
fn issue_1456_live_refresh_does_not_run_ci_monitor_when_no_task_is_probe_eligible() {
    let mut context = context_with_merged_task_holding_ci_failure();
    let mut runner = HealthyRefreshRunner::default();

    refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Live,
    )
    .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(
        crate::runtime_refresh::ci_monitor::load_state(task)
            .episode_id
            .is_some(),
        "the Live tier must keep skipping the CI monitor"
    );
}
