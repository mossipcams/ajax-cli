use crate::lifecycle::{mark_active, mark_cleanable, mark_merged, mark_reviewable};
use crate::models::{
    AgentClient, AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LifecycleStatus,
    LiveObservation, LiveStatusKind, OperatorAction, RuntimeHealth, RuntimeObservationSource,
    SideFlag, SubstrateGap, Task, TaskId,
};
use crate::ui_state::TaskStatus;

fn task_with_flags(handle: &str, flags: &[SideFlag]) -> Task {
    let mut task = Task::new(
        TaskId::new(format!("task-{handle}")),
        "web",
        handle,
        format!("Task {handle}"),
        format!("ajax/{handle}"),
        "main",
        format!("/tmp/worktrees/{handle}"),
        format!("ajax-web-{handle}"),
        "task",
        AgentClient::Codex,
    );

    for flag in flags {
        task.add_side_flag(*flag);
    }

    task
}

fn cleanable_task(handle: &str) -> Task {
    let mut task = task_with_flags(handle, &[]);
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();
    mark_merged(&mut task).unwrap();
    mark_cleanable(&mut task).unwrap();
    task
}

fn claude_active_task() -> Task {
    let mut task = task_with_flags("ack", &[]);
    task.selected_agent = AgentClient::Claude;
    mark_active(&mut task).unwrap();
    task
}

fn ack_at() -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(500)
}

#[test]
fn acknowledged_claude_waiting_has_no_needs_me_annotation() {
    let mut task = claude_active_task();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
    );
    crate::live::acknowledge_attention(&mut task, ack_at());

    let annotations = super::annotate(&task);

    assert!(!annotations
        .iter()
        .any(|annotation| annotation.kind == AnnotationKind::NeedsMe));
}

#[test]
fn new_waiting_after_acknowledgment_restores_needs_me_annotation() {
    let mut task = claude_active_task();
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(400),
    );
    crate::live::acknowledge_attention(&mut task, ack_at());
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
    );

    let annotations = super::annotate(&task);

    assert_eq!(
        annotations
            .iter()
            .filter(|annotation| annotation.kind == AnnotationKind::NeedsMe)
            .count(),
        1
    );
}

#[test]
fn acknowledgment_does_not_remove_broken_or_reviewable_annotations() {
    let mut conflict = claude_active_task();
    crate::live::apply_observation(
        &mut conflict,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
    );
    crate::live::acknowledge_attention(&mut conflict, ack_at());
    assert!(super::annotate(&conflict)
        .iter()
        .any(|annotation| annotation.kind == AnnotationKind::Broken));

    let mut reviewable = claude_active_task();
    mark_reviewable(&mut reviewable).unwrap();
    crate::live::acknowledge_attention(&mut reviewable, ack_at());
    assert!(super::annotate(&reviewable)
        .iter()
        .any(|annotation| annotation.kind == AnnotationKind::Reviewable));
}

#[test]
fn dead_agent_error_outranks_waiting_evidence() {
    let mut task = task_with_flags("blocked", &[SideFlag::NeedsInput, SideFlag::AgentDead]);
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::WaitingForApproval,
        "waiting for approval",
    ));

    let annotations = super::annotate(&task);

    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Broken,
            Evidence::SideFlag(SideFlag::AgentDead),
        )]
    );
    assert_eq!(annotations[0].suggests, OperatorAction::Repair);
}

#[test]
fn checkout_mismatch_runtime_health_is_not_substrate_gap() {
    assert_eq!(
        super::substrate_gap_for_runtime_health(RuntimeHealth::CheckoutMismatch),
        None
    );
}

#[test]
fn stale_checkout_mismatch_health_defers_to_missing_worktree_annotation() {
    use crate::models::GitStatus;

    let mut task = task_with_flags("stale-mismatch", &[SideFlag::WorktreeMissing]);
    mark_active(&mut task).unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
        branch_exists: true,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: Some("abc123".to_string()),
    });
    task.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

    let annotations = super::annotate(&task);

    assert!(task.has_missing_substrate());
    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Broken,
            Evidence::Substrate(SubstrateGap::WorktreeMissing),
        )]
    );
    assert!(!annotations
        .iter()
        .any(|annotation| annotation.evidence == Evidence::CheckoutMismatch));
}

#[test]
fn annotate_emits_broken_for_checkout_mismatch_without_substrate_gap() {
    use crate::models::GitStatus;

    let mut task = task_with_flags("checkout-mismatch", &[]);
    mark_active(&mut task).unwrap();
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
        last_commit: Some("abc123".to_string()),
    });
    task.runtime_projection.health = RuntimeHealth::CheckoutMismatch;

    let annotations = super::annotate(&task);

    assert!(!task.has_missing_substrate());
    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Broken,
            Evidence::CheckoutMismatch,
        )]
    );
    assert_eq!(annotations[0].suggests, OperatorAction::Repair);
}

#[test]
fn annotate_emits_broken_for_missing_substrate() {
    let task = task_with_flags("broken", &[SideFlag::WorktreeMissing]);

    let annotations = super::annotate(&task);

    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Broken,
            Evidence::Substrate(SubstrateGap::WorktreeMissing),
        )]
    );
}

#[test]
fn annotate_emits_broken_for_runtime_probe_failure() {
    let mut task = task_with_flags("probe-failed", &[]);
    task.record_runtime_probe_failure(
        RuntimeObservationSource::TmuxProbe,
        "tmux server unavailable",
    );

    let annotations = super::annotate(&task);

    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].kind, AnnotationKind::Broken);
    assert_eq!(annotations[0].suggests, OperatorAction::Repair);
}

#[test]
fn blocked_agent_is_broken_without_lifecycle_error() {
    let mut task = task_with_flags("blocked", &[]);
    mark_active(&mut task).unwrap();
    task.agent_status = AgentRuntimeStatus::Blocked;

    let annotations = super::annotate(&task);

    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].kind, AnnotationKind::Broken);
    assert_eq!(
        task.lifecycle_status,
        crate::models::LifecycleStatus::Active
    );
}

#[test]
fn annotate_emits_reviewable_when_lifecycle_reviewable() {
    let mut task = task_with_flags("review", &[]);
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();

    let annotations = super::annotate(&task);

    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Reviewable,
            Evidence::Lifecycle(crate::models::LifecycleStatus::Reviewable),
        )]
    );
}

#[test]
fn annotate_emits_cleanable_when_lifecycle_cleanable() {
    let task = cleanable_task("clean");

    let annotations = super::annotate(&task);

    assert_eq!(
        annotations,
        vec![Annotation::new(
            AnnotationKind::Cleanable,
            Evidence::Lifecycle(crate::models::LifecycleStatus::Cleanable),
        )]
    );
}

fn waiting_task(handle: &str) -> Task {
    let mut task = task_with_flags(handle, &[]);
    mark_active(&mut task).unwrap();
    task.add_side_flag(SideFlag::NeedsInput);
    task
}

fn active_task(handle: &str) -> Task {
    let mut task = task_with_flags(handle, &[]);
    mark_active(&mut task).unwrap();
    task
}

fn at(seconds: u64) -> std::time::SystemTime {
    std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
}

fn confirm_at(task: &mut Task, t: u64) {
    assert_eq!(super::take_attention_transition_at(task, at(t)), None);
    assert!(super::take_attention_transition_at(task, at(t + 15)).is_some());
}

#[test]
fn ci_failed_agent_turn_does_not_suppress_settled_failure_attention() {
    use crate::runtime_refresh::ci_monitor::pending_notification;

    let mut task = active_task("ci-busy");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed: lint"),
        at(1_000),
    );
    reduce_report_for_ci(&mut task, false);
    task.agent_status = AgentRuntimeStatus::Running;
    task.add_side_flag(SideFlag::AgentRunning);
    task.metadata.insert(
        super::NOTIFY_CANDIDATE_SINCE_KEY.to_string(),
        "985".to_string(),
    );
    assert!(
        super::take_attention_transition_at(&mut task, at(1_015))
            .is_some(),
        "settled CI failure must phone-ping even while the agent is running"
    );
    assert!(pending_notification(&task).is_some());
}

fn reduce_report_for_ci(task: &mut Task, pending: bool) {
    use crate::runtime_refresh::ci_monitor::reduce_report;
    use crate::{
        adapters::{CiChecksReport, CiChecksState},
        diff_review::{PullRequestRef, PullRequestState},
    };
    let pr = PullRequestRef {
        number: 42,
        title: "Fix".to_string(),
        url: "https://github.test/pull/42".to_string(),
        state: PullRequestState::Open,
        head_ref: "ajax/fix".to_string(),
        head_sha: Some("aaa".to_string()),
    };
    reduce_report(
        task,
        &pr,
        CiChecksReport {
            state: CiChecksState::Failed,
            failed_checks: vec![crate::agent_notification::CiFailedCheck {
                name: "lint".to_string(),
                link: Some("https://github.test/runs/1".to_string()),
                identity: Some("run:1".to_string()),
            }],
            check_identities: vec!["run:1".to_string()],
            has_pending: pending,
            error: None,
        },
        100,
    );
}

#[test]
fn idle_to_waiting_fires_once() {
    let mut task = waiting_task("notify");

    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        Some(super::AttentionTransition {
            repo: "web".to_string(),
            handle: "notify".to_string(),
            status: TaskStatus::Waiting,
            explanation: Some("Waiting for input".to_string()),
            client: "codex".to_string(),
        })
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_016)),
        None
    );
}

#[test]
fn class_change_keeps_shared_debounce() {
    let mut task = waiting_task("class-change");
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );

    task.add_side_flag(SideFlag::Conflicted);
    // Waiting→Error mid-dwell keeps the shared 15s clock.
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_010)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)).map(|t| t.status),
        Some(TaskStatus::Error)
    );
}

#[test]
fn waiting_then_idle_past_episode_clear_then_waiting_fires_again() {
    let mut task = waiting_task("notify");
    confirm_at(&mut task, 1_000);

    task.remove_side_flag(SideFlag::NeedsInput);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_010)),
        None
    );
    assert!(task.metadata.contains_key(super::LAST_NOTIFIED_STATUS_KEY));
    assert!(task.metadata.contains_key(super::NOTIFY_QUIET_SINCE_KEY));

    // Still within the 30s quiet window: stamps remain.
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_039)),
        None
    );
    assert!(task.metadata.contains_key(super::LAST_NOTIFIED_STATUS_KEY));

    // Quiet dwell elapsed: episode clears.
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_040)),
        None
    );
    assert!(!task.metadata.contains_key(super::LAST_NOTIFIED_STATUS_KEY));
    assert!(!task.metadata.contains_key(super::LAST_NOTIFIED_AT_KEY));
    assert!(!task.metadata.contains_key(super::NOTIFY_QUIET_SINCE_KEY));

    task.add_side_flag(SideFlag::NeedsInput);
    confirm_at(&mut task, 1_041);
}

#[test]
fn waiting_cycle_within_episode_clear_fires_once() {
    let mut task = waiting_task("notify");
    confirm_at(&mut task, 1_000);

    // Agent turn boundary: brief Running, then waiting again before clear.
    task.remove_side_flag(SideFlag::NeedsInput);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_010)),
        None
    );
    task.add_side_flag(SideFlag::NeedsInput);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_020)),
        None
    );

    // Sustained Idle past episode clear, then Waiting again → re-fire.
    task.remove_side_flag(SideFlag::NeedsInput);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_030)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_060)),
        None
    );
    task.add_side_flag(SideFlag::NeedsInput);
    confirm_at(&mut task, 1_061);
}

#[test]
fn error_within_episode_still_fires() {
    let mut task = waiting_task("notify");
    confirm_at(&mut task, 1_000);

    task.add_side_flag(SideFlag::Conflicted);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_030)),
        None
    );
    let transition = super::take_attention_transition_at(&mut task, at(1_045));
    assert_eq!(
        transition.map(|transition| transition.status),
        Some(TaskStatus::Error)
    );
}

#[test]
fn waiting_to_error_fires() {
    let mut task = waiting_task("notify");
    confirm_at(&mut task, 1_000);

    task.add_side_flag(SideFlag::Conflicted);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_010)),
        None
    );
    let transition = super::take_attention_transition_at(&mut task, at(1_025));

    assert_eq!(
        transition.map(|transition| transition.status),
        Some(TaskStatus::Error)
    );
}

#[test]
fn repeated_identical_ci_evidence_fires_once() {
    let mut task = active_task("ci");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        at(1_000),
    );
    let first = super::take_attention_transition_at(&mut task, at(1_001));
    assert_eq!(first, None);
    let first = super::take_attention_transition_at(&mut task, at(1_016));
    assert_eq!(
        first.as_ref().map(|t| (t.status, t.explanation.as_deref())),
        Some((TaskStatus::Error, Some("CI failed")))
    );

    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed again"),
        at(1_010),
    );
    let second = super::take_attention_transition_at(&mut task, at(1_011));
    assert_eq!(second, None);
}

#[test]
fn distinct_error_reasons_do_not_refire_within_error_class() {
    let mut task = active_task("err");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        at(1_000),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_001)),
        None
    );
    assert!(super::take_attention_transition_at(&mut task, at(1_016)).is_some());

    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
        at(1_010),
    );
    let second = super::take_attention_transition_at(&mut task, at(1_011));
    assert_eq!(second, None);
}

#[test]
fn acknowledgment_stamp_matches_current_reason() {
    let mut task = active_task("ack-reason");
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
        at(1_000),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_001)),
        None
    );
    assert!(super::take_attention_transition_at(&mut task, at(1_016)).is_some());
    crate::live::acknowledge_attention(&mut task, at(1_020));
    assert_eq!(
        task.metadata
            .get(super::LAST_NOTIFIED_STATUS_KEY)
            .map(String::as_str),
        Some("Error")
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_021)),
        None
    );
}

#[test]
fn ready_for_review_does_not_notify() {
    let mut task = task_with_flags("review", &[]);
    mark_active(&mut task).unwrap();
    mark_reviewable(&mut task).unwrap();

    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some("Ready for review")
    );
    assert_eq!(super::take_attention_transition(&mut task), None);
    assert!(task.metadata.is_empty());
}

#[test]
fn acknowledge_silences_current_episode() {
    let mut task = waiting_task("notify");
    crate::live::acknowledge_attention(&mut task, at(1_010));

    assert_eq!(
        task.metadata
            .get(super::LAST_NOTIFIED_STATUS_KEY)
            .map(String::as_str),
        Some("Waiting")
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_011)),
        None
    );
}

#[test]
fn running_and_idle_never_fire() {
    let mut running = task_with_flags("running", &[SideFlag::AgentRunning]);
    mark_active(&mut running).unwrap();
    running.agent_status = AgentRuntimeStatus::Running;
    assert_eq!(super::take_attention_transition(&mut running), None);
    assert!(running.metadata.is_empty());

    let mut idle = task_with_flags("idle", &[]);
    mark_active(&mut idle).unwrap();
    assert_eq!(super::take_attention_transition(&mut idle), None);
    assert!(idle.metadata.is_empty());
}

#[test]
fn removing_with_missing_substrate_does_not_notify() {
    let mut task = active_task("removing");
    task.mark_resource_missing(SideFlag::TmuxMissing);
    task.lifecycle_status = LifecycleStatus::Removing;

    assert_eq!(super::take_attention_transition(&mut task), None);
    assert!(!task.metadata.contains_key(super::LAST_NOTIFIED_STATUS_KEY));
}

#[test]
fn teardown_incomplete_still_notifies() {
    let mut task = active_task("teardown");
    task.lifecycle_status = LifecycleStatus::TeardownIncomplete;

    confirm_at(&mut task, 1_000);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_015)),
        None
    );
}

#[test]
fn delegated_waiting_does_not_notify() {
    let mut task = active_task("delegated");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            crate::agent_status::SUMMARY_WAITING_ON_DELEGATED,
        ),
    );

    assert!(!task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some(crate::agent_status::EXPLANATION_WAITING_ON_DELEGATED)
    );
    assert_eq!(super::take_attention_transition(&mut task), None);
}

#[test]
fn delegated_still_active_does_not_notify() {
    let mut task = active_task("children");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(
            LiveStatusKind::WaitingForInput,
            crate::agent_status::SUMMARY_DELEGATED_STILL_ACTIVE,
        ),
    );

    assert!(!task.has_side_flag(SideFlag::NeedsInput));
    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some(crate::agent_status::EXPLANATION_DELEGATED_STILL_ACTIVE)
    );
    assert_eq!(super::take_attention_transition(&mut task), None);
}

#[test]
fn real_user_waiting_still_notifies() {
    let mut task = active_task("ask");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
    );

    assert!(task.has_side_flag(SideFlag::NeedsInput));
    confirm_at(&mut task, 1_000);
}

/// A rate-limited wait is transient and retryable, not actionable operator
/// input. It still shows as Waiting/"Rate limited" in the UI but must not
/// phone-ping or stamp a notify episode.
#[test]
fn rate_limited_waiting_does_not_notify() {
    let mut task = active_task("rate-limited");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::RateLimited, "rate limited"),
    );

    assert_eq!(
        crate::ui_state::derive_operator_status(&task).status,
        TaskStatus::Waiting
    );
    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some("Rate limited")
    );
    assert_eq!(super::take_attention_transition(&mut task), None);
    assert!(task.metadata.is_empty());
}

/// Turn-settled Done (Cursor stop, Claude/Codex/Pi settle) shows as
/// Waiting/"Response ready" in the UI but must not phone-ping or stamp a
/// notify episode — same as "Ready for review" / "Rate limited".
#[test]
fn response_ready_waiting_does_not_notify() {
    let mut task = active_task("response-ready");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::Done, "done"),
    );

    assert_eq!(
        crate::ui_state::derive_operator_status(&task).status,
        TaskStatus::Waiting
    );
    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some("Response ready")
    );
    assert_eq!(super::take_attention_transition(&mut task), None);
    assert!(task.metadata.is_empty());
}

#[test]
fn waiting_explanation_churn_does_not_refire_within_episode() {
    let mut task = active_task("churn");

    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
        at(1_000),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_001)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_016))
            .map(|t| t.explanation.unwrap_or_default()),
        Some("Waiting for input".to_string())
    );

    // Same Waiting class, different explanation — no re-fire.
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::WaitingForApproval, "waiting for approval"),
        at(1_002),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_002)),
        None
    );

    // Class change Waiting → Error still fires once after a fresh shared dwell
    // (prior delivery cleared the candidate).
    crate::live::apply_observation_at(
        &mut task,
        LiveObservation::new(LiveStatusKind::Blocked, "blocked"),
        at(1_003),
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_017)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_032))
            .map(|t| (t.status, t.explanation.unwrap_or_default())),
        Some((TaskStatus::Error, "Agent blocked".to_string()))
    );
}

#[test]
fn notify_debounce_holds_then_fires_once() {
    let mut task = waiting_task("debounce");

    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );
    assert!(task
        .metadata
        .contains_key(super::NOTIFY_CANDIDATE_SINCE_KEY));

    let transition = super::take_attention_transition_at(&mut task, at(1_015));
    assert_eq!(
        transition.as_ref().and_then(|t| t.explanation.as_deref()),
        Some("Waiting for input")
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_016)),
        None
    );
}

#[test]
fn auth_required_waiting_does_not_notify() {
    let mut task = active_task("auth");
    crate::live::apply_observation(
        &mut task,
        LiveObservation::new(LiveStatusKind::AuthRequired, "auth required"),
    );

    assert_eq!(
        crate::ui_state::derive_operator_status(&task).status,
        TaskStatus::Waiting
    );
    assert_eq!(
        crate::ui_state::derive_operator_status(&task)
            .explanation
            .as_deref(),
        Some("Authentication required")
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_020)),
        None
    );
    assert!(task.metadata.is_empty());
}

#[test]
fn debounce_clears_when_returns_to_running() {
    let mut task = waiting_task("debounce-clear");

    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_000)),
        None
    );
    assert!(task
        .metadata
        .contains_key(super::NOTIFY_CANDIDATE_SINCE_KEY));

    task.remove_side_flag(SideFlag::NeedsInput);
    task.add_side_flag(SideFlag::AgentRunning);
    task.agent_status = AgentRuntimeStatus::Running;
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_005)),
        None
    );
    assert!(!task
        .metadata
        .contains_key(super::NOTIFY_CANDIDATE_SINCE_KEY));

    task.remove_side_flag(SideFlag::AgentRunning);
    task.agent_status = AgentRuntimeStatus::Waiting;
    task.add_side_flag(SideFlag::NeedsInput);
    assert_eq!(
        super::take_attention_transition_at(&mut task, at(1_010)),
        None
    );
    assert!(super::take_attention_transition_at(&mut task, at(1_025)).is_some());
}
