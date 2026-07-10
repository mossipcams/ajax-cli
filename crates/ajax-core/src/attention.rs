use crate::models::{
    AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LifecycleStatus, LiveStatusClass,
    LiveStatusKind, RuntimeHealth, SideFlag, SubstrateGap, Task,
};
use crate::ui_state::{derive_operator_status, TaskStatus};

pub const LAST_NOTIFIED_STATUS_KEY: &str = "last_notified_status";
pub const LAST_NOTIFIED_AT_KEY: &str = "last_notified_at";

/// How long a delivery keeps the detector armed against re-fires. A genuine
/// agent turn cycle (Waiting → Running → Waiting) inside this window is one
/// episode and gets one ping; without it an actively driven session pings on
/// every turn boundary.
/// ponytail: constant; gate on tmux client activity if a ping per window is
/// still too chatty during active use.
const NOTIFY_REARM_COOLDOWN: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionTransition {
    pub repo: String,
    pub handle: String,
    pub status: TaskStatus,
    pub explanation: Option<String>,
}

/// Rising-edge detector for operator attention. Fires once when a task
/// crosses into Waiting or Error, deduplicated by a metadata stamp that
/// persists with the normal registry snapshot save. Returning to Running or
/// Idle re-arms the detector only after `NOTIFY_REARM_COOLDOWN`, so one
/// Waiting episode interrupted by short Running bursts delivers one ping.
/// ponytail: best-effort dedup; a concurrent first observation can produce
/// one duplicate delivery — add per-key CAS only if duplicates ever annoy.
pub fn take_attention_transition(task: &mut Task) -> Option<AttentionTransition> {
    take_attention_transition_at(task, std::time::SystemTime::now())
}

pub fn take_attention_transition_at(
    task: &mut Task,
    now: std::time::SystemTime,
) -> Option<AttentionTransition> {
    let operator_status = derive_operator_status(task);
    match operator_status.status {
        TaskStatus::Waiting | TaskStatus::Error => {
            let stamp = operator_status.status.as_str();
            if task
                .metadata
                .get(LAST_NOTIFIED_STATUS_KEY)
                .is_some_and(|last| last == stamp)
            {
                return None;
            }
            task.metadata
                .insert(LAST_NOTIFIED_STATUS_KEY.to_string(), stamp.to_string());
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
            // Missing or malformed delivery stamp re-arms immediately.
            let cooling_down = task
                .metadata
                .get(LAST_NOTIFIED_AT_KEY)
                .and_then(|value| value.parse::<u64>().ok())
                .is_some_and(|fired_at| {
                    unix_seconds(now).saturating_sub(fired_at) < NOTIFY_REARM_COOLDOWN.as_secs()
                });
            if !cooling_down {
                task.metadata.remove(LAST_NOTIFIED_STATUS_KEY);
                task.metadata.remove(LAST_NOTIFIED_AT_KEY);
            }
            None
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
        if let Some(kind) = annotation_kind_for_live_status(live_status.kind) {
            push_collapsed_annotation(
                &mut annotations,
                Annotation::new(kind, Evidence::LiveStatus(live_status.kind)),
            );
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
            Evidence::Substrate(_) | Evidence::RuntimeObservationFailed => 4,
        },
        AnnotationKind::Broken => match evidence {
            Evidence::LiveStatus(_) => 0,
            Evidence::Substrate(_) => 1,
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
            Evidence::Substrate(_) | Evidence::RuntimeObservationFailed => 4,
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
        RuntimeHealth::Healthy | RuntimeHealth::Unobservable => None,
    }
}

#[cfg(test)]
mod tests {
    use crate::lifecycle::{mark_active, mark_cleanable, mark_merged, mark_reviewable};
    use crate::models::{
        AgentClient, AgentRuntimeStatus, Annotation, AnnotationKind, Evidence, LiveObservation,
        LiveStatusKind, OperatorAction, RuntimeObservationSource, SideFlag, SubstrateGap, Task,
        TaskId,
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
    fn waiting_then_idle_past_cooldown_then_waiting_fires_again() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition_at(&mut task, at(1_000)).is_some());

        task.remove_side_flag(SideFlag::NeedsInput);
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_400)),
            None
        );
        assert!(!task.metadata.contains_key(super::LAST_NOTIFIED_STATUS_KEY));
        assert!(!task.metadata.contains_key(super::LAST_NOTIFIED_AT_KEY));

        task.add_side_flag(SideFlag::NeedsInput);
        assert!(super::take_attention_transition_at(&mut task, at(1_401)).is_some());
    }

    #[test]
    fn waiting_cycle_within_cooldown_fires_once() {
        let mut task = waiting_task("notify");
        assert!(super::take_attention_transition_at(&mut task, at(1_000)).is_some());

        // Agent turn boundary: brief Running, then waiting again 90s later.
        task.remove_side_flag(SideFlag::NeedsInput);
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_060)),
            None
        );
        task.add_side_flag(SideFlag::NeedsInput);
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_090)),
            None
        );

        // A Running sample past the cooldown re-arms; the next wait pings.
        task.remove_side_flag(SideFlag::NeedsInput);
        assert_eq!(
            super::take_attention_transition_at(&mut task, at(1_301)),
            None
        );
        task.add_side_flag(SideFlag::NeedsInput);
        assert!(super::take_attention_transition_at(&mut task, at(1_302)).is_some());
    }

    #[test]
    fn error_within_cooldown_still_fires() {
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
    fn busy_flap_does_not_fire_notification() {
        use std::time::{Duration, UNIX_EPOCH};
        let mut task = task_with_flags("flap", &[]);
        crate::lifecycle::mark_active(&mut task).unwrap();
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::AgentRunning, "working"),
            UNIX_EPOCH + Duration::from_secs(100),
        );

        // One flappy waiting sample while the agent works: no notification.
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "waiting"),
            UNIX_EPOCH + Duration::from_secs(110),
        );
        assert_eq!(super::take_attention_transition(&mut task), None);

        // Dwell-confirmed waiting: fires exactly once.
        crate::live::apply_observation_at(
            &mut task,
            LiveObservation::new(LiveStatusKind::WaitingForInput, "still waiting"),
            UNIX_EPOCH + Duration::from_secs(115),
        );
        let transition = super::take_attention_transition(&mut task);
        assert_eq!(
            transition.map(|transition| transition.status),
            Some(TaskStatus::Waiting)
        );
        assert_eq!(super::take_attention_transition(&mut task), None);
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
}
