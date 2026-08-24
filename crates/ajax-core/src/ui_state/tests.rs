use super::{
    attention_band, derive_operator_status, AttentionBand, TaskStatus, AGENT_PROCESS_ALIVE_KEY,
};
use crate::{
    lifecycle::{
        mark_active, mark_cleanable, mark_error, mark_mergeable, mark_merged, mark_removed,
        mark_reviewable,
    },
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind,
        RuntimeObservationSource, SideFlag, Task, TaskId,
    },
};

fn base_task() -> Task {
    Task::new(
        TaskId::new("task-1"),
        "web",
        "fix-login",
        "Fix login",
        "ajax/fix-login",
        "main",
        "/tmp/worktrees/web-fix-login",
        "ajax-web-fix-login",
        "task",
        AgentClient::Codex,
    )
}

fn claude_active_task() -> Task {
    let mut task = base_task();
    task.selected_agent = AgentClient::Claude;
    task.lifecycle_status = crate::models::LifecycleStatus::Active;
    task
}

#[test]
fn acknowledged_claude_waiting_projects_idle() {
    let mut task = claude_active_task();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
    );
    crate::live::acknowledge_attention(
        &mut task,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
    );

    let status = super::derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Idle);
    assert_eq!(status.explanation, None);
    assert_eq!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Active
    );
}

#[test]
fn new_claude_waiting_after_acknowledgment_projects_needs_input() {
    let mut task = claude_active_task();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
    );
    crate::live::acknowledge_attention(
        &mut task,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
    );
    // Waiting evidence newer than the acknowledgment.
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(600),
    );

    let status = super::derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Waiting);
    assert_eq!(status.explanation.as_deref(), Some("Waiting for input"));
}

#[test]
fn acknowledgment_does_not_hide_failure_or_missing_substrate() {
    // CommandFailed surfaces as a NeedsInput attention state and TmuxMissing
    // as Failed; acknowledgment must change neither, so neither becomes Idle.
    for status in [LiveStatusKind::CommandFailed, LiveStatusKind::TmuxMissing] {
        let mut task = claude_active_task();
        crate::live::apply_observation(&mut task, LiveObservation::new(status, "evidence"));
        let before = super::derive_operator_status(&task);

        crate::live::acknowledge_attention(
            &mut task,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
        );
        let after = super::derive_operator_status(&task);

        assert_eq!(after, before, "{status:?}");
        assert_ne!(after.status, TaskStatus::Idle, "{status:?}");
    }
}

fn clean_git_status() -> GitStatus {
    GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("ajax/fix-login".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: true,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: Some("abc123".to_string()),
    }
}

#[test]
fn removed_lifecycle_becomes_archived_regardless_of_other_signals() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_merged(&mut task).unwrap();
    mark_removed(&mut task).unwrap();
    task.add_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::Dirty);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
}

#[test]
fn needs_input_dominates_active_lifecycle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.add_side_flag(SideFlag::NeedsInput);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
}

#[test]
fn blocker_signals_outrank_review_ready_lifecycle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    task.add_side_flag(SideFlag::Conflicted);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
}

#[test]
fn waiting_agent_status_needs_input() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Waiting;

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
}

#[test]
fn merge_conflict_live_status_is_blocked() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::MergeConflict,
        "conflict",
    ));

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
}

#[test]
fn missing_substrate_is_failed_even_with_otherwise_clean_lifecycle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.mark_resource_missing(SideFlag::WorktreeMissing);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
}

#[test]
fn runtime_probe_failure_is_failed_without_changing_lifecycle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.record_runtime_probe_failure(
        RuntimeObservationSource::TmuxProbe,
        "tmux server unavailable",
    );

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
    assert_eq!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Active
    );
}

#[test]
fn waiting_live_statuses_need_input_instead_of_blocking() {
    for live_status in [
        LiveStatusKind::WaitingForApproval,
        LiveStatusKind::WaitingForInput,
    ] {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(live_status, "waiting"));

        assert_eq!(
            derive_operator_status(&task).status,
            TaskStatus::Waiting,
            "{live_status:?}"
        );
    }
}

#[test]
fn failure_live_statuses_project_error_and_operator_boundaries_project_waiting() {
    for live_status in [
        LiveStatusKind::CiFailed,
        LiveStatusKind::MergeConflict,
        LiveStatusKind::CommandFailed,
        LiveStatusKind::Blocked,
    ] {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(live_status, "blocked"));

        assert_eq!(
            derive_operator_status(&task).status,
            TaskStatus::Error,
            "{live_status:?}"
        );
    }

    for live_status in [
        LiveStatusKind::AuthRequired,
        LiveStatusKind::RateLimited,
        LiveStatusKind::ContextLimit,
    ] {
        let mut task = base_task();
        mark_active(&mut task).unwrap();
        task.live_status = Some(LiveObservation::new(live_status, "attention"));

        assert_eq!(
            derive_operator_status(&task).status,
            TaskStatus::Waiting,
            "{live_status:?}"
        );
    }
}

#[test]
fn error_lifecycle_without_blocker_is_failed() {
    let mut task = base_task();
    mark_error(&mut task).unwrap();

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
}

#[test]
fn mergeable_lifecycle_is_safe_merge() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_mergeable(&mut task).unwrap();

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
}

#[test]
fn mergeable_lifecycle_with_blocker_is_blocked() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_mergeable(&mut task).unwrap();
    task.add_side_flag(SideFlag::Conflicted);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Error);
}

#[test]
fn cleanable_lifecycle_is_cleanable() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_merged(&mut task).unwrap();
    mark_cleanable(&mut task).unwrap();

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
}

#[test]
fn merged_lifecycle_with_clean_git_is_cleanable() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_merged(&mut task).unwrap();
    task.git_status = Some(clean_git_status());

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
}

#[test]
fn merged_lifecycle_with_dirty_git_falls_back_to_idle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_merged(&mut task).unwrap();
    let mut git = clean_git_status();
    git.dirty = true;
    task.git_status = Some(git);
    task.add_side_flag(SideFlag::Dirty);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
}

#[test]
fn reviewable_lifecycle_with_safe_merge_promotes_to_safe_merge() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    let mut git = clean_git_status();
    git.merged = false;
    task.git_status = Some(git);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
}

#[test]
fn reviewable_lifecycle_without_blocker_is_review_ready() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Waiting);
}

#[test]
fn running_evidence_outranks_reviewable_lifecycle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
}

#[test]
fn active_lifecycle_with_agent_running_is_running() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
}

#[test]
fn active_lifecycle_with_tests_running_live_status_is_running() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.live_status = Some(LiveObservation::new(LiveStatusKind::TestsRunning, "tests"));

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Running);
}

#[test]
fn active_lifecycle_without_signals_is_unknown() {
    // An active task with no live status, an unstarted agent, and no flags
    // has no source that can prove Running/Waiting/Done/Error — it projects
    // Unknown rather than a fabricated Idle (precedence step 6).
    let mut task = base_task();
    mark_active(&mut task).unwrap();

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Unknown);
}

#[test]
fn live_process_without_native_events_is_idle_not_unknown() {
    // Precedence tier 3: a confirmed live wrapper process is real evidence,
    // so the task is at rest — not unprovable. It must never read Running,
    // because liveness alone never becomes AgentRunning.
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    assert_eq!(
        derive_operator_status(&task).status,
        TaskStatus::Unknown,
        "no evidence at all is still Unknown"
    );

    task.metadata.insert(
        AGENT_PROCESS_ALIVE_KEY.to_string(),
        "1700000000".to_string(),
    );

    let projected = derive_operator_status(&task);
    assert_eq!(projected.status, TaskStatus::Idle);
    assert_ne!(projected.status, TaskStatus::Running);
    assert!(!projected.actionable);

    // Refresh removes the key once the heartbeat goes stale, and the task
    // falls back to Unknown.
    task.metadata.remove(AGENT_PROCESS_ALIVE_KEY);
    assert_eq!(derive_operator_status(&task).status, TaskStatus::Unknown);
}

#[test]
fn active_lifecycle_with_acknowledged_waiting_is_idle_not_unknown() {
    // Positive evidence of rest (an acknowledged waiting live status) keeps
    // the task Idle; only the true no-evidence case becomes Unknown.
    let mut task = claude_active_task();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
    );
    crate::live::acknowledge_attention(
        &mut task,
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(500),
    );

    assert_eq!(derive_operator_status(&task).status, TaskStatus::Idle);
}

#[test]
fn actionable_flag_is_set_structurally_per_evidence() {
    // Genuine input/approval waiting and errors are actionable; soft waits
    // (rate limit) and the review boundary are not; running/idle are not.
    let mut approval = claude_active_task();
    crate::live::apply_observation(
        &mut approval,
        LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
    );
    assert!(derive_operator_status(&approval).actionable);

    let mut ci = claude_active_task();
    crate::live::apply_observation(
        &mut ci,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed: ci"),
    );
    assert!(derive_operator_status(&ci).actionable);

    let mut rate_limited = claude_active_task();
    crate::live::apply_observation(
        &mut rate_limited,
        LiveObservation::new(LiveStatusKind::RateLimited, "rate limited"),
    );
    let rate_limited = derive_operator_status(&rate_limited);
    assert_eq!(rate_limited.status, TaskStatus::Waiting);
    assert!(!rate_limited.actionable);

    let mut reviewable = claude_active_task();
    crate::lifecycle::mark_reviewable(&mut reviewable).unwrap();
    let reviewable = derive_operator_status(&reviewable);
    assert_eq!(reviewable.status, TaskStatus::Waiting);
    assert!(!reviewable.actionable);

    let mut running = claude_active_task();
    crate::live::apply_observation(
        &mut running,
        LiveObservation::new(LiveStatusKind::AgentRunning, "agent running"),
    );
    assert!(!derive_operator_status(&running).actionable);
}

#[rstest::rstest]
#[case(
    LiveStatusKind::AgentRunning,
    TaskStatus::Running,
    Some("Agent working")
)]
#[case(
    LiveStatusKind::CommandRunning,
    TaskStatus::Running,
    Some("Running command")
)]
#[case(
    LiveStatusKind::TestsRunning,
    TaskStatus::Running,
    Some("Running tests")
)]
#[case(
    LiveStatusKind::WaitingForApproval,
    TaskStatus::Waiting,
    Some("Waiting for approval")
)]
#[case(
    LiveStatusKind::WaitingForInput,
    TaskStatus::Waiting,
    Some("Waiting for input")
)]
#[case(LiveStatusKind::Done, TaskStatus::Waiting, Some("Response ready"))]
#[case(
    LiveStatusKind::CommandFailed,
    TaskStatus::Error,
    Some("Command failed")
)]
#[case(LiveStatusKind::CiFailed, TaskStatus::Error, Some("CI failed"))]
#[case(
    LiveStatusKind::MergeConflict,
    TaskStatus::Error,
    Some("Merge conflict")
)]
fn canonical_status_maps_live_evidence(
    #[case] live_kind: LiveStatusKind,
    #[case] expected_status: TaskStatus,
    #[case] expected_explanation: Option<&str>,
) {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(live_kind, "raw summary"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(100),
    );

    let status = super::derive_operator_status(&task);

    assert_eq!(status.status, expected_status);
    assert_eq!(status.explanation.as_deref(), expected_explanation);
}

#[test]
fn acknowledged_waiting_evidence_projects_idle_without_deleting_evidence() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    let observed_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting"),
        observed_at,
    );
    crate::live::acknowledge_attention(&mut task, observed_at + std::time::Duration::from_secs(1));

    let status = super::derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Idle);
    assert_eq!(status.explanation, None);
    assert_eq!(
        task.live_status.as_ref().map(|live| live.kind),
        Some(LiveStatusKind::WaitingForInput)
    );
}

#[test]
fn reviewable_lifecycle_is_waiting_until_acknowledged() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();

    let before = super::derive_operator_status(&task);
    assert_eq!(before.status, TaskStatus::Waiting);
    assert_eq!(before.explanation.as_deref(), Some("Ready for review"));

    let acknowledged_at = task.last_activity_at + std::time::Duration::from_secs(1);
    crate::live::acknowledge_attention(&mut task, acknowledged_at);
    let after = super::derive_operator_status(&task);
    assert_eq!(after.status, TaskStatus::Idle);
    assert_eq!(after.explanation, None);
}

#[test]
fn live_status_class_matches_canonical_explanations() {
    use crate::models::{LiveStatusClass, LiveStatusKind};
    let all = [
        LiveStatusKind::WorktreeMissing,
        LiveStatusKind::TmuxMissing,
        LiveStatusKind::TaskWindowMissing,
        LiveStatusKind::ShellIdle,
        LiveStatusKind::CommandRunning,
        LiveStatusKind::TestsRunning,
        LiveStatusKind::AgentRunning,
        LiveStatusKind::WaitingForApproval,
        LiveStatusKind::WaitingForInput,
        LiveStatusKind::Blocked,
        LiveStatusKind::RateLimited,
        LiveStatusKind::AuthRequired,
        LiveStatusKind::MergeConflict,
        LiveStatusKind::CiFailed,
        LiveStatusKind::ContextLimit,
        LiveStatusKind::CommandFailed,
        LiveStatusKind::Done,
        LiveStatusKind::Unknown,
    ];
    for kind in all {
        assert_eq!(
            super::canonical_waiting_explanation(kind).is_some(),
            kind.class() == LiveStatusClass::Waiting,
            "waiting membership diverged for {kind:?}"
        );
        assert_eq!(
            super::canonical_error_explanation(kind).is_some(),
            kind.class() == LiveStatusClass::Error,
            "error membership diverged for {kind:?}"
        );
        assert_eq!(
            super::canonical_running_explanation(kind).is_some(),
            kind.class() == LiveStatusClass::Running,
            "running membership diverged for {kind:?}"
        );
    }
}

#[test]
fn stale_checkout_mismatch_health_defers_to_missing_worktree_status() {
    use crate::lifecycle::mark_active;
    use crate::models::RuntimeHealth;

    let mut task = base_task();
    mark_active(&mut task).unwrap();
    let mut git = clean_git_status();
    git.worktree_exists = false;
    git.current_branch = Some("fix/pane-stuck".to_string());
    task.git_status = Some(git);
    task.mark_resource_missing(SideFlag::WorktreeMissing);
    task.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Error);
    assert_eq!(status.explanation.as_deref(), Some("Worktree missing"));
    assert!(!status
        .explanation
        .as_deref()
        .is_some_and(|explanation| explanation.contains("expected")));
    assert!(task.has_missing_substrate());
}

#[test]
fn checkout_mismatch_status_names_observed_and_expected_checkout() {
    use crate::lifecycle::mark_active;
    use crate::models::RuntimeHealth;

    let mut named_branch = base_task();
    mark_active(&mut named_branch).unwrap();
    let mut git = clean_git_status();
    git.current_branch = Some("fix/pane-stuck".to_string());
    named_branch.git_status = Some(git);
    named_branch.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

    let named_status = derive_operator_status(&named_branch);
    assert_eq!(named_status.status, TaskStatus::Error);
    assert_eq!(
        named_status.explanation.as_deref(),
        Some("Worktree on fix/pane-stuck; expected ajax/fix-login")
    );
    assert!(!named_status
        .explanation
        .as_deref()
        .is_some_and(|explanation| explanation.contains("missing")));
    assert!(!named_branch.has_missing_substrate());

    let mut detached = base_task();
    mark_active(&mut detached).unwrap();
    let mut detached_git = clean_git_status();
    detached_git.current_branch = None;
    detached.git_status = Some(detached_git);

    let detached_status = derive_operator_status(&detached);
    assert_eq!(detached_status.status, TaskStatus::Error);
    assert_eq!(
        detached_status.explanation.as_deref(),
        Some("Worktree detached; expected ajax/fix-login")
    );
    assert!(!detached_status
        .explanation
        .as_deref()
        .is_some_and(|explanation| explanation.contains("missing")));
    assert!(!detached.has_missing_substrate());
}

#[test]
fn error_status_bands_as_needs_you() {
    let mut task = base_task();
    mark_error(&mut task).unwrap();

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Error);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::NeedsYou
    );
}

#[test]
fn actionable_waiting_bands_as_needs_you() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Waiting;

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Waiting);
    assert!(status.actionable);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::NeedsYou
    );
}

#[test]
fn soft_waiting_bands_as_active() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::RateLimited,
        "rate limited",
    ));

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Waiting);
    assert!(!status.actionable);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::Active
    );
}

#[test]
fn acknowledged_reviewable_still_bands_as_review() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    let acknowledged_at = task.last_activity_at + std::time::Duration::from_secs(1);
    crate::live::acknowledge_attention(&mut task, acknowledged_at);

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Idle);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::Review,
        "an acknowledged reviewable status sinks to Idle, but the band must read lifecycle and stay Review"
    );
}

#[test]
fn actionable_waiting_outranks_review_boundary() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
    );

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Waiting);
    assert!(status.actionable);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::NeedsYou,
        "an actionable approval gate must outrank the review boundary"
    );
}

#[test]
fn running_bands_as_active() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Running;

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Running);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::Active
    );
}

#[test]
fn resting_task_bands_as_idle() {
    let mut task = base_task();
    mark_active(&mut task).unwrap();
    let observed_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(100);
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting"),
        observed_at,
    );
    crate::live::acknowledge_attention(&mut task, observed_at + std::time::Duration::from_secs(1));

    let status = derive_operator_status(&task);

    assert_eq!(status.status, TaskStatus::Idle);
    assert_eq!(
        attention_band(&status, task.lifecycle_status),
        AttentionBand::Idle
    );
}

#[test]
fn canonical_status_labels_are_stable_and_unique() {
    let labels = [
        TaskStatus::Running,
        TaskStatus::Waiting,
        TaskStatus::Idle,
        TaskStatus::Error,
    ]
    .map(TaskStatus::as_str);

    let mut sorted = labels.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), labels.len());
}
