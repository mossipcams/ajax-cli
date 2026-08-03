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
/// First actionable sighting after re-arm; delivery waits until this quiet
/// window reaches the confirmation dwell (15s). One shared clock for every
/// actionable Waiting/Error — class changes do not restart the dwell.
pub const NOTIFY_CANDIDATE_SINCE_KEY: &str = "notify_candidate_since";

/// How long Running/Idle must persist after a delivery before the detector
/// re-arms. Brief turn-boundary Running samples stay inside one episode.
// ponytail: 30s constant; gate on tmux client activity if still too chatty.
const EPISODE_CLEAR_DWELL: std::time::Duration = std::time::Duration::from_secs(30);

/// How long any actionable Waiting/Error must persist before the first webhook
/// in an episode. Shared across status classes so flaps do not reset the clock.
pub const NOTIFY_CONFIRMATION_DWELL: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionTransition {
    pub repo: String,
    pub handle: String,
    pub status: TaskStatus,
    pub explanation: Option<String>,
    pub client: String,
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
                clear_notify_candidate(task);
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
                clear_notify_candidate(task);
                return None;
            }
            if !confirm_notify_candidate(task, now) {
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
                client: format!("{:?}", task.selected_agent).to_ascii_lowercase(),
            })
        }
        TaskStatus::Running | TaskStatus::Idle | TaskStatus::Unknown => {
            clear_notify_candidate(task);
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
    clear_notify_candidate(task);
}

fn episode_stamp(status: &crate::ui_state::OperatorStatus) -> String {
    status.status.as_str().to_string()
}

/// Actionable operator-attention is decided structurally by the projector
/// (`OperatorStatus::actionable`), not by matching the explanation string.
fn is_actionable_attention(status: &crate::ui_state::OperatorStatus) -> bool {
    status.actionable
}

fn clear_notify_candidate(task: &mut Task) {
    task.metadata.remove(NOTIFY_CANDIDATE_SINCE_KEY);
}

fn confirm_notify_candidate(task: &mut Task, now: std::time::SystemTime) -> bool {
    let now_secs = unix_seconds(now);
    let candidate_since = task
        .metadata
        .get(NOTIFY_CANDIDATE_SINCE_KEY)
        .and_then(|value| value.parse::<u64>().ok());
    match candidate_since {
        Some(since) if now_secs >= since + NOTIFY_CONFIRMATION_DWELL.as_secs() => {
            clear_notify_candidate(task);
            true
        }
        Some(_) => false,
        None => {
            task.metadata
                .insert(NOTIFY_CANDIDATE_SINCE_KEY.to_string(), now_secs.to_string());
            false
        }
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
mod tests;
