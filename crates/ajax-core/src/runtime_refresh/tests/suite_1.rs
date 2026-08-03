use super::super::*;
use super::*;

#[test]
fn native_running_observation_drives_agent_running() {
    let mut context = context_with_active_task();
    let mut runner = RuntimeRefreshRunner;
    let cache = ObsSource::new(vec![lifecycle_obs(ActivityKind::Working, 1, 120)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(derive_operator_status(task).status, TaskStatus::Running);
}

#[test]
fn wrapper_exit_success_is_terminal_done_fallback() {
    let mut context = context_with_active_task();
    let mut runner = RuntimeRefreshRunner;
    // No native lifecycle events; only a confirmed wrapper exit. Requirement
    // 12: confirmed exit 0 is a Done fallback where native evidence is absent.
    let cache = ObsSource::new(vec![exit_obs(ActivityKind::Done, 1)]);

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::Done)
    );
}

#[test]
fn wrapper_liveness_alone_does_not_set_agent_running() {
    let mut context = context_with_active_task();
    let mut runner = RuntimeRefreshRunner;
    // Requirement 12: wrapper Running is liveness only, never Agent Running.
    let cache = ObsSource::new(vec![]).with_liveness(ProcessLiveness {
        alive: true,
        observed_at: SystemTime::now(),
    });

    refresh_runtime_context_with_tier(&mut context, &mut runner, &cache, RefreshTier::Full)
        .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_ne!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    assert!(!task.has_side_flag(SideFlag::AgentRunning));
}

#[test]
fn missing_session_refresh_updates_task_evidence_once() {
    let mut context = context_with_task_for_missing_session();
    let mut runner = MissingSessionRunner::default();

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert_eq!(context.registry.task_window_status_updates(), 1);
    assert!(!runner.commands.iter().any(
        |command| matches!(command.args.as_slice(), [command, ..] if command == "capture-pane")
    ));
}

#[test]
fn missing_session_refresh_preserves_teardown_incomplete_failure_status() {
    let mut context = context_with_teardown_incomplete_task();
    let mut runner = MissingSessionRunner::default();

    refresh_runtime_context(&mut context, &mut runner).unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::TeardownIncomplete);
    assert_eq!(
        task.tmux_status.as_ref().map(|status| status.exists),
        Some(false)
    );
    assert!(task.has_side_flag(SideFlag::TmuxMissing));
    assert_eq!(
        task.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::CommandFailed)
    );
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| status.summary.as_str()),
        Some("drop incomplete at delete branch")
    );
}

#[test]
fn orphan_recovery_uses_one_registry_snapshot_for_discovered_worktrees() {
    let base = context_with_unchanged_running_task();
    let mut context =
        CommandContext::new(base.config, CountingRegistry::from_registry(base.registry));
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::UNIX_EPOCH,
        RuntimeObservationSource::TmuxProbe,
    );
    let mut runner = OrphanRecoveryRunner::default();

    let changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert!(changed);
    assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
    assert!(context.registry.get_task(&TaskId::new("web/b")).is_some());
    assert!(context.registry.get_task(&TaskId::new("web/c")).is_some());
    assert_eq!(
        context.registry.list_tasks_calls(),
        2,
        "expected refresh to reuse the initial task snapshot plus one git refresh scan, got {} list_tasks calls",
        context.registry.list_tasks_calls()
    );
}

#[test]
fn steady_state_refresh_recovers_orphan_worktrees() {
    let base = context_with_unchanged_running_task();
    let mut context =
        CommandContext::new(base.config, CountingRegistry::from_registry(base.registry));
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::UNIX_EPOCH,
        RuntimeObservationSource::TmuxProbe,
    );
    let mut runner = OrphanRecoveryRunner::default();

    let changed = refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();

    assert!(changed);
    assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
    assert!(context.registry.get_task(&TaskId::new("web/b")).is_some());
    assert!(context.registry.get_task(&TaskId::new("web/c")).is_some());
}

#[test]
fn github_failed_check_records_ci_failure_evidence_and_attention() {
    let mut context = context_with_active_task();
    let stdout = ci_failed_stdout("lint");
    let mut runner = CiChecksRunner::with_gh(&stdout, "", 1);

    let changed = refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();

    let task = context.registry.get_task(&TaskId::new(TASK_ID)).unwrap();
    assert!(changed);
    assert_eq!(runner.gh_command_count(), 1);
    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "ci failed: lint"))
    );
    let probed_at = task
        .metadata
        .get("ci_checks_probed_at")
        .and_then(|value| value.parse::<u64>().ok())
        .expect("ci probe timestamp should be stamped");
    assert!(unix_seconds_for_test(SystemTime::now()).saturating_sub(probed_at) <= 5);

    let mut task_for_attention = task.clone();
    let now_secs = unix_seconds_for_test(SystemTime::now());
    task_for_attention.metadata.insert(
        crate::attention::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        (now_secs.saturating_sub(20)).to_string(),
    );
    let transition = crate::attention::take_attention_transition(&mut task_for_attention);
    assert_eq!(
        transition.map(|transition| transition.status),
        Some(TaskStatus::Error)
    );
}

#[test]
fn github_pending_checks_surface_ci_running_over_github_failure() {
    // Relevant pending checks override the native/github failure with a
    // Running "ci running" state, but never override a local check failure
    // or a merge conflict (requirement 6).
    let now = SystemTime::now();
    let mut github = task_with_live(LiveStatusKind::CiFailed, "ci failed: ci");
    github.add_side_flag(SideFlag::TestsFailed);
    let mut local = task_with_live(LiveStatusKind::CiFailed, "check failed");
    local.add_side_flag(SideFlag::TestsFailed);
    let mut conflict = task_with_live(LiveStatusKind::MergeConflict, "merge failed");

    apply_github_checks_observation(&mut github, CiChecksObservation::Pending, now);
    apply_github_checks_observation(&mut local, CiChecksObservation::Pending, now);
    apply_github_checks_observation(&mut conflict, CiChecksObservation::Pending, now);

    assert_eq!(
        github
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiPending, "ci running"))
    );
    assert!(!github.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        local
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "check failed"))
    );
    assert!(local.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        conflict
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::MergeConflict, "merge failed"))
    );
}

#[test]
fn github_pending_checks_do_not_mask_unacknowledged_attention_gate() {
    // A `Running "CI running"` projection can never notify (attention.rs
    // clears the notify candidate for Running), so letting pending CI
    // overwrite an unacknowledged approval/input gate would make the only
    // actionable signal both invisible and unnotified. Narrow deviation
    // from plan §6 row 6, which ranks display and ignores notification.
    let now = SystemTime::now();
    let observed_at = now - Duration::from_secs(60);

    let mut gate = task_with_live(LiveStatusKind::WaitingForApproval, "waiting for approval");
    gate.live_status_observed_at = Some(observed_at);
    gate.add_side_flag(SideFlag::NeedsInput);

    apply_github_checks_observation(&mut gate, CiChecksObservation::Pending, now);

    assert_eq!(
        gate.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::WaitingForApproval),
        "pending CI must not overwrite an unacknowledged approval gate"
    );
    assert!(
        gate.has_side_flag(SideFlag::NeedsInput),
        "GitHub CI state must not clear the agent's needs-input flag"
    );
    let projected = crate::ui_state::derive_operator_status(&gate);
    assert_eq!(projected.status, crate::ui_state::TaskStatus::Waiting);
    assert!(projected.actionable, "the operator still has to act");

    // Once acknowledged, the gate is no longer an actionable signal and CI
    // takes the display back (plan §6 row 6).
    let mut acknowledged =
        task_with_live(LiveStatusKind::WaitingForApproval, "waiting for approval");
    acknowledged.live_status_observed_at = Some(observed_at);
    acknowledged.record_attention_acknowledgment(now);

    apply_github_checks_observation(&mut acknowledged, CiChecksObservation::Pending, now);

    assert_eq!(
        acknowledged.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::CiPending)
    );
}

#[test]
fn github_ci_evidence_is_cleared_when_probing_stops() {
    // Plan §7: rows 5/6 apply only while evidence is "relevant + not
    // stale". Once the lifecycle retires the probe, CI evidence can never
    // be confirmed again and must not keep projecting.
    let now = SystemTime::now();

    let mut merged = task_with_live(LiveStatusKind::CiPending, "ci running");
    merged.lifecycle_status = LifecycleStatus::Merged;
    assert!(github_probe_is_retired(&merged));
    clear_github_ci_evidence(&mut merged);
    assert!(merged.live_status.is_none());
    assert_eq!(
        crate::ui_state::derive_operator_status(&merged).status,
        crate::ui_state::TaskStatus::Idle,
        "a merged task must not report CI running forever"
    );

    let mut active = task_with_live(LiveStatusKind::CiPending, "ci running");
    assert!(!github_probe_is_retired(&active));

    // An unobservable probe can no longer vouch for a pending run.
    apply_github_checks_observation(
        &mut active,
        CiChecksObservation::Unobservable {
            reason: "no pull request for branch".to_string(),
        },
        now,
    );
    assert!(active.live_status.is_none());
    assert!(active.metadata.contains_key(CI_PROBE_ERROR_KEY));
}

#[test]
fn github_healthy_checks_clear_only_github_ci_live_status() {
    let now = SystemTime::now();
    let mut github = task_with_live(LiveStatusKind::CiFailed, "ci failed: ci");
    github.add_side_flag(SideFlag::TestsFailed);
    let mut local = task_with_live(LiveStatusKind::CiFailed, "check failed");
    local.add_side_flag(SideFlag::TestsFailed);
    let mut conflict = task_with_live(LiveStatusKind::MergeConflict, "merge failed");
    let mut running = task_with_live(LiveStatusKind::AgentRunning, "running");
    running.add_side_flag(SideFlag::TestsFailed);

    apply_github_checks_observation(&mut github, CiChecksObservation::Healthy, now);
    apply_github_checks_observation(&mut local, CiChecksObservation::Healthy, now);
    apply_github_checks_observation(&mut conflict, CiChecksObservation::Healthy, now);
    apply_github_checks_observation(&mut running, CiChecksObservation::Healthy, now);

    assert!(github.live_status.is_none());
    assert!(!github.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        local
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "check failed"))
    );
    assert!(local.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        conflict
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::MergeConflict, "merge failed"))
    );
    assert_eq!(
        running.live_status.as_ref().map(|status| status.kind),
        Some(LiveStatusKind::AgentRunning)
    );
    assert!(!running.has_side_flag(SideFlag::TestsFailed));
}

#[test]
fn github_unobservable_records_metadata_without_runtime_projection_error() {
    for (stderr, reason) in [
        ("HTTP 401: gh auth failed", "HTTP 401"),
        (
            "no pull requests found for branch ajax/fix-login",
            "no pull request for branch",
        ),
    ] {
        let output = Ok(CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: stderr.to_string(),
        });
        let observation = GithubChecksAdapter::parse_pr_checks(&output);
        let mut task = task_with_live(LiveStatusKind::CiFailed, "ci failed: ci");
        let previous_projection = task.runtime_projection.clone();

        apply_github_checks_observation(&mut task, observation, SystemTime::now());

        assert_eq!(
            task.live_status
                .as_ref()
                .map(|status| (status.kind, status.summary.as_str())),
            Some((LiveStatusKind::CiFailed, "ci failed: ci"))
        );
        assert_eq!(task.runtime_projection, previous_projection);
        assert_ne!(
            derive_operator_status(&task).explanation.as_deref(),
            Some("Status unavailable")
        );
        assert!(task
            .metadata
            .get("ci_probe_error")
            .is_some_and(|error| error.contains(reason)));
        assert!(task.metadata.contains_key("ci_checks_probed_at"));
    }
}

#[test]
fn github_ci_probe_reuses_fresh_timestamp_and_refreshes_stale_timestamp() {
    let now = unix_seconds_for_test(SystemTime::now());

    let mut fresh_context = context_with_active_task();
    fresh_context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap()
        .metadata
        .insert("ci_checks_probed_at".to_string(), (now - 60).to_string());
    let failed_stdout = ci_failed_stdout("ci");
    let mut fresh_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);

    refresh_runtime_context_with_tier(
        &mut fresh_context,
        &mut fresh_runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();

    assert_eq!(fresh_runner.gh_command_count(), 0);

    let mut stale_context = context_with_active_task();
    stale_context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap()
        .metadata
        .insert("ci_checks_probed_at".to_string(), (now - 301).to_string());
    let mut stale_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);

    refresh_runtime_context_with_tier(
        &mut stale_context,
        &mut stale_runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();

    assert_eq!(stale_runner.gh_command_count(), 1);
}

#[test]
fn github_ci_failure_reprobes_sooner_than_default_interval() {
    let now = unix_seconds_for_test(SystemTime::now());
    let failed_stdout = ci_failed_stdout("ci");

    let mut failed_context = context_with_active_task();
    {
        let task = failed_context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CiFailed,
            "ci failed: ci",
        ));
        task.metadata
            .insert("ci_checks_probed_at".to_string(), (now - 31).to_string());
    }
    let mut failed_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);
    refresh_runtime_context_with_tier(
        &mut failed_context,
        &mut failed_runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();
    assert_eq!(failed_runner.gh_command_count(), 1);

    let mut healthy_context = context_with_active_task();
    healthy_context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap()
        .metadata
        .insert("ci_checks_probed_at".to_string(), (now - 31).to_string());
    let mut healthy_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);
    refresh_runtime_context_with_tier(
        &mut healthy_context,
        &mut healthy_runner,
        &NoAgentStatusSource,
        RefreshTier::Full,
    )
    .unwrap();
    assert_eq!(healthy_runner.gh_command_count(), 0);
}

#[test]
fn github_failed_check_does_not_overwrite_merge_conflict() {
    let mut task = task_with_live(LiveStatusKind::MergeConflict, "merge failed");

    apply_github_checks_observation(
        &mut task,
        CiChecksObservation::Failed {
            summary: "ci".to_string(),
        },
        SystemTime::now(),
    );

    assert_eq!(
        task.live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::MergeConflict, "merge failed"))
    );
    assert!(task.metadata.contains_key("ci_checks_probed_at"));
}

#[test]
fn steady_state_fresh_projections_skip_orphan_git_scan_on_live_tier() {
    let mut context = context_with_unchanged_running_task();
    let mut runner = OrphanRecoveryRunner::default();

    refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Live,
    )
    .unwrap();

    assert!(
        !runner.commands.iter().any(|command| {
            command.args.len() >= 5 && command.args[2] == "worktree" && command.args[3] == "list"
        }),
        "live tier with fresh projections should not list worktrees: {:?}",
        runner.commands
    );
}

#[test]
fn steady_state_recovers_orphan_when_tmux_lists_unregistered_ajax_session() {
    let base = context_with_unchanged_running_task();
    let mut context = CommandContext::new(base.config, base.registry);
    let mut runner = OrphanRecoveryRunner {
        sessions_output: Some("ajax-web-fix-login\najax-web-a\n".to_string()),
        ..Default::default()
    };

    let changed = refresh_runtime_context_with_tier(
        &mut context,
        &mut runner,
        &NoAgentStatusSource,
        RefreshTier::Live,
    )
    .unwrap();

    assert!(changed);
    assert!(context.registry.get_task(&TaskId::new("web/a")).is_some());
}

#[test]
fn steady_state_refresh_reuses_initial_task_snapshot() {
    let base = context_with_unchanged_running_task();
    let mut context =
        CommandContext::new(base.config, CountingRegistry::from_registry(base.registry));
    seed_fresh_ci_probe(&mut context);
    let mut runner = GitSkippingRunner::default();

    let _changed = refresh_runtime_context(&mut context, &mut runner).unwrap();

    assert_eq!(
        context.registry.get_task_calls(),
        0,
        "refresh should reuse the initial list_tasks snapshot, got {} get_task calls",
        context.registry.get_task_calls()
    );
}
