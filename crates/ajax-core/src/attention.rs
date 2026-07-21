use crate::models::{
    AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LifecycleStatus, LiveStatusClass,
    LiveStatusKind, RuntimeHealth, SideFlag, SubstrateGap, Task,
};
use crate::ui_state::{derive_operator_status, TaskStatus};

pub const LAST_NOTIFIED_STATUS_KEY: &str = "last_notified_status";
pub const LAST_NOTIFIED_AT_KEY: &str = "last_notified_at";
/// First Running/Idle sighting after a notified episode; stamp clears once this
/// quiet window reaches the episode-clear dwell (30s).
pub const NOTIFY_QUIET_SINCE_KEY: &str = "notify_quiet_since";

/// How long Running/Idle must persist after a delivery before the detector
/// re-arms. Brief turn-boundary Running samples stay inside one episode.
// ponytail: 30s constant; gate on tmux client activity if still too chatty.
const EPISODE_CLEAR_DWELL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionTransition {
    pub repo: String,
    pub handle: String,
    pub status: TaskStatus,
    pub explanation: Option<String>,
}

/// Episode detector for operator attention webhooks. Fires once when a task
/// enters actionable Waiting (needs input) or Error; lifecycle-only
/// "Ready for review" stays inbox-visible but does not phone-ping. In-flight
/// drop (`Removing` / `Removed`) never pings — teardown substrate gaps are
/// expected; durable `TeardownIncomplete` still does. Returning to
/// Running/Idle clears the stamp only after the episode-clear dwell (30s),
/// so one Waiting episode interrupted by short Running bursts delivers one
/// ping. [`silence_notify_episode`] (from acknowledge) stamps the current
/// episode without delivering so opening a task stops further pings until new
/// evidence.
/// ponytail: best-effort dedup; a concurrent first observation can produce
/// one duplicate delivery — add per-key CAS only if duplicates ever annoy.
pub fn take_attention_transition(task: &mut Task) -> Option<AttentionTransition> {
    take_attention_transition_at(task, std::time::SystemTime::now())
}

pub fn take_attention_transition_at(
    task: &mut Task,
    now: std::time::SystemTime,
) -> Option<AttentionTransition> {
    // Drop teardown intentionally removes tmux/worktree; missing substrate
    // during `Removing`/`Removed` would otherwise project as Error and ping.
    // `TeardownIncomplete` is a durable lifecycle error and still notifies.
    if matches!(
        task.lifecycle_status,
        LifecycleStatus::Removing | LifecycleStatus::Removed
    ) {
        return None;
    }
    let operator_status = derive_operator_status(task);
    match operator_status.status {
        TaskStatus::Waiting | TaskStatus::Error => {
            if !is_actionable_attention(&operator_status) {
                return None;
            }
            // Still in (or back in) attention: cancel any quiet countdown.
            task.metadata.remove(NOTIFY_QUIET_SINCE_KEY);
            let stamp = episode_stamp(&operator_status);
            if task
                .metadata
                .get(LAST_NOTIFIED_STATUS_KEY)
                .is_some_and(|last| last == &stamp)
            {
                return None;
            }
            task.metadata
                .insert(LAST_NOTIFIED_STATUS_KEY.to_string(), stamp);
            task.metadata.insert(
                LAST_NOTIFIED_AT_KEY.to_string(),
                unix_seconds(now).to_string(),
            );
            Some(AttentionTransition {
                repo: task.repo.clone(),
                handle: task.handle.clone(),
                status: operator_status.status,
                explanation: operator_status.explanation,
            })
        }
        TaskStatus::Running | TaskStatus::Idle => {
            clear_notify_episode_if_quiet(task, now);
            None
        }
    }
}

/// Mark the current attention episode as already notified so ack/open stops
/// further webhook deliveries until new actionable evidence appears.
pub fn silence_notify_episode(task: &mut Task, now: std::time::SystemTime) {
    let operator_status = derive_operator_status(task);
    if !is_actionable_attention(&operator_status) {
        return;
    }
    task.metadata.insert(
        LAST_NOTIFIED_STATUS_KEY.to_string(),
        episode_stamp(&operator_status),
    );
    task.metadata.insert(
        LAST_NOTIFIED_AT_KEY.to_string(),
        unix_seconds(now).to_string(),
    );
    task.metadata.remove(NOTIFY_QUIET_SINCE_KEY);
}

fn episode_stamp(status: &crate::ui_state::OperatorStatus) -> String {
    format!(
        "{}|{}",
        status.status.as_str(),
        status.explanation.as_deref().unwrap_or("")
    )
}

fn is_actionable_attention(status: &crate::ui_state::OperatorStatus) -> bool {
    match status.status {
        TaskStatus::Error => true,
        TaskStatus::Waiting => {
            let explanation = status.explanation.as_deref().unwrap_or("");
            explanation != "Ready for review"
                && explanation != "Rate limited"
                && !crate::agent_status::is_delegated_waiting_summary(explanation)
        }
        TaskStatus::Running | TaskStatus::Idle => false,
    }
}

fn clear_notify_episode_if_quiet(task: &mut Task, now: std::time::SystemTime) {
    if !task.metadata.contains_key(LAST_NOTIFIED_STATUS_KEY) {
        task.metadata.remove(NOTIFY_QUIET_SINCE_KEY);
        return;
    }
    let now_secs = unix_seconds(now);
    let quiet_since = task
        .metadata
        .get(NOTIFY_QUIET_SINCE_KEY)
        .and_then(|value| value.parse::<u64>().ok());
    match quiet_since {
        Some(since) if now_secs >= since + EPISODE_CLEAR_DWELL.as_secs() => {
            task.metadata.remove(LAST_NOTIFIED_STATUS_KEY);
            task.metadata.remove(LAST_NOTIFIED_AT_KEY);
            task.metadata.remove(NOTIFY_QUIET_SINCE_KEY);
        }
        Some(_) => {}
        None => {
            task.metadata
                .insert(NOTIFY_QUIET_SINCE_KEY.to_string(), now_secs.to_string());
        }
    }
}

fn unix_seconds(time: std::time::SystemTime) -> u64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

pub fn annotate(task: &Task) -> Vec<Annotation> {
    let mut annotations = Vec::new();
    let operator_status = derive_operator_status(task);

    if task.runtime_projection.observation_error.is_some() {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(AnnotationKind::Broken, Evidence::RuntimeObservationFailed),
        );
    }

    if operator_status.status == TaskStatus::Waiting
        && matches!(
            task.agent_status,
            AgentRuntimeStatus::Waiting | AgentRuntimeStatus::Blocked
        )
        && !crate::agent_status::is_delegated_waiting_summary(
            operator_status.explanation.as_deref().unwrap_or(""),
        )
    {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(
                AnnotationKind::NeedsMe,
                Evidence::AgentStatus(task.agent_status),
            ),
        );
    }

    if let Some(live_status) = task.live_status.as_ref() {
        if !crate::agent_status::is_delegated_waiting_summary(&live_status.summary) {
            if let Some(kind) = annotation_kind_for_live_status(live_status.kind) {
                push_collapsed_annotation(
                    &mut annotations,
                    Annotation::new(kind, Evidence::LiveStatus(live_status.kind)),
                );
            }
        }
    }

    for flag in task.side_flags() {
        if let Some(kind) = annotation_kind_for_side_flag(flag) {
            let evidence = substrate_gap_for_side_flag(flag)
                .map(Evidence::Substrate)
                .unwrap_or(Evidence::SideFlag(flag));
            push_collapsed_annotation(&mut annotations, Annotation::new(kind, evidence));
        }
    }

    if let Some(gap) = substrate_gap_for_runtime_health(task.runtime_projection.health) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(AnnotationKind::Broken, Evidence::Substrate(gap)),
        );
    }

    if !task.has_missing_substrate()
        && (task.has_checkout_mismatch()
            || task.runtime_projection.health == RuntimeHealth::CheckoutMismatch)
    {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(AnnotationKind::Broken, Evidence::CheckoutMismatch),
        );
    }

    if let Some(kind) = annotation_kind_for_agent_status(task.agent_status) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(kind, Evidence::Lifecycle(task.lifecycle_status)),
        );
    }

    if let Some(kind) = annotation_kind_for_lifecycle(task.lifecycle_status) {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(kind, Evidence::Lifecycle(task.lifecycle_status)),
        );
    }

    if operator_status.status == TaskStatus::Error
        && !annotations
            .iter()
            .any(|annotation| annotation.kind == AnnotationKind::Broken)
    {
        push_collapsed_annotation(
            &mut annotations,
            Annotation::new(
                AnnotationKind::Broken,
                Evidence::Lifecycle(task.lifecycle_status),
            ),
        );
    }

    annotations.retain(|annotation| match annotation.kind {
        AnnotationKind::NeedsMe => operator_status.status == TaskStatus::Waiting,
        AnnotationKind::Broken => operator_status.status == TaskStatus::Error,
        AnnotationKind::Reviewable | AnnotationKind::Cleanable => true,
    });

    annotations.sort_by_key(|annotation| annotation.severity);
    annotations
}

fn push_collapsed_annotation(annotations: &mut Vec<Annotation>, annotation: Annotation) {
    if let Some(existing) = annotations
        .iter_mut()
        .find(|existing| existing.kind == annotation.kind)
    {
        if evidence_preference(annotation.kind, &annotation.evidence)
            < evidence_preference(existing.kind, &existing.evidence)
        {
            *existing = annotation;
        }
    } else {
        annotations.push(annotation);
    }
}

fn evidence_preference(kind: AnnotationKind, evidence: &Evidence) -> u32 {
    match kind {
        AnnotationKind::NeedsMe => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::AgentStatus(_) => 1,
            Evidence::SideFlag(_) => 2,
            Evidence::Lifecycle(_) => 3,
            Evidence::Substrate(_)
            | Evidence::RuntimeObservationFailed
            | Evidence::CheckoutMismatch => 4,
        },
        AnnotationKind::Broken => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::Substrate(_) | Evidence::CheckoutMismatch => 1,
            Evidence::RuntimeObservationFailed => 2,
            Evidence::SideFlag(_) => 3,
            Evidence::AgentStatus(_) => 4,
            Evidence::Lifecycle(_) => 5,
        },
        AnnotationKind::Reviewable | AnnotationKind::Cleanable => match evidence {
            Evidence::Lifecycle(_) => 0,
            Evidence::LiveStatus(_) => 1,
            Evidence::AgentStatus(_) => 2,
            Evidence::SideFlag(_) => 3,
            Evidence::Substrate(_)
            | Evidence::RuntimeObservationFailed
            | Evidence::CheckoutMismatch => 4,
        },
    }
}

fn annotation_kind_for_live_status(status: LiveStatusKind) -> Option<AnnotationKind> {
    // Done is Waiting-class for status reduction but reads as Reviewable here.
    if status == LiveStatusKind::Done {
        return Some(AnnotationKind::Reviewable);
    }
    match status.class() {
        LiveStatusClass::Waiting => Some(AnnotationKind::NeedsMe),
        LiveStatusClass::Error | LiveStatusClass::MissingSubstrate => Some(AnnotationKind::Broken),
        LiveStatusClass::Running | LiveStatusClass::Neutral => None,
    }
}

fn annotation_kind_for_side_flag(flag: SideFlag) -> Option<AnnotationKind> {
    match flag {
        SideFlag::NeedsInput => Some(AnnotationKind::NeedsMe),
        SideFlag::AgentDead => Some(AnnotationKind::Broken),
        SideFlag::TmuxMissing
        | SideFlag::WorktreeMissing
        | SideFlag::TaskWindowMissing
        | SideFlag::BranchMissing
        | SideFlag::Conflicted => Some(AnnotationKind::Broken),
        SideFlag::TestsFailed => Some(AnnotationKind::Broken),
        SideFlag::Dirty | SideFlag::AgentRunning | SideFlag::Stale | SideFlag::Unpushed => None,
    }
}

fn annotation_kind_for_agent_status(status: AgentRuntimeStatus) -> Option<AnnotationKind> {
    match status {
        AgentRuntimeStatus::Waiting => Some(AnnotationKind::NeedsMe),
        AgentRuntimeStatus::Dead => Some(AnnotationKind::Broken),
        AgentRuntimeStatus::Blocked => Some(AnnotationKind::Broken),
        AgentRuntimeStatus::NotStarted
        | AgentRuntimeStatus::Running
        | AgentRuntimeStatus::Done
        | AgentRuntimeStatus::Unknown => None,
    }
}

fn annotation_kind_for_lifecycle(status: LifecycleStatus) -> Option<AnnotationKind> {
    match status {
        LifecycleStatus::Reviewable | LifecycleStatus::Mergeable => {
            Some(AnnotationKind::Reviewable)
        }
        LifecycleStatus::Merged | LifecycleStatus::Cleanable => Some(AnnotationKind::Cleanable),
        LifecycleStatus::Created
        | LifecycleStatus::Provisioning
        | LifecycleStatus::Active
        | LifecycleStatus::Waiting
        | LifecycleStatus::Removing
        | LifecycleStatus::TeardownIncomplete
        | LifecycleStatus::Removed
        | LifecycleStatus::Orphaned
        | LifecycleStatus::Error => None,
    }
}

fn substrate_gap_for_side_flag(flag: SideFlag) -> Option<SubstrateGap> {
    match flag {
        SideFlag::WorktreeMissing => Some(SubstrateGap::WorktreeMissing),
        SideFlag::TmuxMissing => Some(SubstrateGap::TmuxMissing),
        SideFlag::TaskWindowMissing => Some(SubstrateGap::TaskWindowMissing),
        SideFlag::BranchMissing => Some(SubstrateGap::BranchMissing),
        _ => None,
    }
}

fn substrate_gap_for_runtime_health(health: RuntimeHealth) -> Option<SubstrateGap> {
    match health {
        RuntimeHealth::MissingWorktree => Some(SubstrateGap::WorktreeMissing),
        RuntimeHealth::MissingSession => Some(SubstrateGap::TmuxMissing),
        RuntimeHealth::MissingTaskWindow | RuntimeHealth::WrongTaskWindowPath => {
            Some(SubstrateGap::TaskWindowMissing)
        }
        RuntimeHealth::Healthy | RuntimeHealth::Unobservable | RuntimeHealth::CheckoutMismatch => {
            None
        }
    }
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn idle_to_waiting_fires_once() {
        let mut task = waiting_task("notify");

        let transition = super::take_attention_transition(&mut task);

        assert_eq!(
            transition,
            Some(super::AttentionTransition {
                repo: "web".to_string(),
                handle: "notify".to_string(),
                status: TaskStatus::Waiting,
                explanation: Some("Waiting for input".to_string()),
            })
        );
        assert_eq!(super::take_attention_transition(&mut task), None);
    }

    fn at(seconds: u64) -> std::time::SystemTime {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(seconds)
    }

    #[test]
    fn waiting_then_idle_past_episode_clear_then_waiting_fires_again() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition_at(&mut task, at(1_000)).is_some());

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
        assert!(super::take_attention_transition_at(&mut task, at(1_041)).is_some());
    }

    #[test]
    fn waiting_cycle_within_episode_clear_fires_once() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition_at(&mut task, at(1_000)).is_some());

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
        assert!(super::take_attention_transition_at(&mut task, at(1_061)).is_some());
    }

    #[test]
    fn error_within_episode_still_fires() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition_at(&mut task, at(1_000)).is_some());

        task.add_side_flag(SideFlag::Conflicted);
        let transition = super::take_attention_transition_at(&mut task, at(1_030));
        assert_eq!(
            transition.map(|transition| transition.status),
            Some(TaskStatus::Error)
        );
    }

    #[test]
    fn waiting_to_error_fires() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition(&mut task).is_some());

        task.add_side_flag(SideFlag::Conflicted);
        let transition = super::take_attention_transition(&mut task);

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
    fn distinct_error_reason_fires_again() {
        let mut task = active_task("err");
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
            at(1_000),
        );
        assert!(super::take_attention_transition_at(&mut task, at(1_001)).is_some());

        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::MergeConflict, "merge conflict"),
            at(1_010),
        );
        let second = super::take_attention_transition_at(&mut task, at(1_011));
        assert_eq!(
            second
                .as_ref()
                .map(|t| (t.status, t.explanation.as_deref())),
            Some((TaskStatus::Error, Some("Merge conflict")))
        );
    }

    #[test]
    fn acknowledgment_stamp_matches_current_reason() {
        let mut task = active_task("ack-reason");
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"),
            at(1_000),
        );
        assert!(super::take_attention_transition_at(&mut task, at(1_001)).is_some());
        crate::live::acknowledge_attention(&mut task, at(1_010));
        assert_eq!(
            task.metadata
                .get(super::LAST_NOTIFIED_STATUS_KEY)
                .map(String::as_str),
            Some("Error|CI failed")
        );
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_011)),
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
            Some("Waiting|Waiting for input")
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

        let first = super::take_attention_transition(&mut task);
        assert_eq!(first.map(|t| t.status), Some(TaskStatus::Error));
        assert_eq!(super::take_attention_transition(&mut task), None);
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
        let transition = super::take_attention_transition(&mut task);
        assert_eq!(
            transition.map(|transition| (
                transition.status,
                transition.explanation.unwrap_or_default()
            )),
            Some((TaskStatus::Waiting, "Waiting for approval".to_string()))
        );
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

    /// Characterization: notify keys off `status|explanation`. Flipping between
    /// distinct actionable reasons re-fires immediately — no quiet window and
    /// no dwell between attention states. CLI/TUI Full refresh (~1s) plus loose
    /// pane matchers turn this into phone spam.
    #[test]
    fn distinct_attention_reasons_refire_immediately_without_quiet_window() {
        let mut task = active_task("churn");

        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            at(1_000),
        );
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_001))
                .map(|t| t.explanation.unwrap_or_default()),
            Some("Waiting for input".to_string())
        );

        // One second later: different actionable reason (e.g. pane matched "blocked").
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::Blocked, "blocked"),
            at(1_002),
        );
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_002))
                .map(|t| (t.status, t.explanation.unwrap_or_default())),
            Some((TaskStatus::Error, "Agent blocked".to_string()))
        );

        // Back to the original waiting reason — third ping, still no 30s quiet.
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting for input"),
            at(1_003),
        );
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_003))
                .map(|t| t.explanation.unwrap_or_default()),
            Some("Waiting for input".to_string())
        );
    }
}
