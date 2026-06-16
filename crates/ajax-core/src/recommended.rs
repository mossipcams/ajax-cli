use crate::{
    models::{
        AgentRuntimeStatus, Annotation, Evidence, LifecycleStatus, LiveStatusKind, OperatorAction,
        SideFlag, Task,
    },
    policy::merge_safety,
    ui_state::derive_operator_status,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RemediationId {
    FixCi,
    ResolveMergeConflicts,
}

impl RemediationId {
    pub const fn compatibility_label(self) -> &'static str {
        match self {
            Self::FixCi => crate::remediation::FIX_CI,
            Self::ResolveMergeConflicts => crate::remediation::RESOLVE_MERGE_CONFLICTS,
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            crate::remediation::FIX_CI => Some(Self::FixCi),
            crate::remediation::RESOLVE_MERGE_CONFLICTS => Some(Self::ResolveMergeConflicts),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TaskActionId {
    BuiltIn(OperatorAction),
    Remediation(RemediationId),
}

impl TaskActionId {
    pub const fn compatibility_label(self) -> &'static str {
        match self {
            Self::BuiltIn(action) => action.as_str(),
            Self::Remediation(remediation) => remediation.compatibility_label(),
        }
    }

    pub fn from_compatibility_label(label: &str) -> Option<Self> {
        OperatorAction::from_label(label)
            .map(Self::BuiltIn)
            .or_else(|| RemediationId::from_label(label).map(Self::Remediation))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionAvailability {
    Available,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskActionDecision {
    pub id: TaskActionId,
    pub availability: ActionAvailability,
    pub reason: String,
    pub requires_confirmation: bool,
}

impl TaskActionDecision {
    pub fn is_available(&self) -> bool {
        self.availability == ActionAvailability::Available
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperatorActionPlan {
    pub action: OperatorAction,
    pub reason: String,
    pub available_actions: Vec<OperatorAction>,
}

pub fn task_action_decisions(task: &Task) -> Vec<TaskActionDecision> {
    let built_ins = [
        crate::slices::resume::decision(task),
        crate::slices::review::decision(task),
        crate::slices::ship::decision(task),
        crate::slices::drop::decision(task),
        crate::slices::repair::decision(task),
    ];
    let mut decisions = built_ins.into_iter().collect::<Vec<_>>();
    decisions.extend(crate::slices::remediate::decisions(task));
    decisions
}

pub(crate) fn available_built_in_decision(
    action: OperatorAction,
    reason: impl Into<String>,
    requires_confirmation: bool,
) -> TaskActionDecision {
    TaskActionDecision {
        id: TaskActionId::BuiltIn(action),
        availability: ActionAvailability::Available,
        reason: reason.into(),
        requires_confirmation,
    }
}

pub(crate) fn blocked_built_in_decision(
    action: OperatorAction,
    reason: impl Into<String>,
    requires_confirmation: bool,
) -> TaskActionDecision {
    TaskActionDecision {
        id: TaskActionId::BuiltIn(action),
        availability: ActionAvailability::Blocked,
        reason: reason.into(),
        requires_confirmation,
    }
}

pub fn operator_action(task: &Task) -> OperatorActionPlan {
    if task_is_known_invalid(task) {
        return OperatorActionPlan {
            action: OperatorAction::Drop,
            reason: "invalid_task".to_string(),
            available_actions: available_operator_actions(task),
        };
    }

    let derived_annotations = crate::attention::annotate(task);
    let annotations = derived_annotations.as_slice();
    let primary_annotation = annotations
        .iter()
        .min_by_key(|annotation| annotation.severity);
    let available_actions = available_operator_actions(task);
    let action = primary_annotation
        .map(|annotation| annotation.suggests)
        .filter(|action| available_actions.contains(action))
        .unwrap_or_else(|| fallback_operator_action(task));
    let reason = primary_annotation
        .map(annotation_reason)
        .unwrap_or_else(|| fallback_operator_reason(task).to_string());

    OperatorActionPlan {
        action,
        reason,
        available_actions,
    }
}

fn annotation_reason(annotation: &Annotation) -> String {
    evidence_label(&annotation.evidence).to_string()
}

pub(crate) fn evidence_label(evidence: &Evidence) -> &'static str {
    match evidence {
        Evidence::LiveStatus(status) => match status {
            LiveStatusKind::WaitingForApproval => "waiting_for_approval",
            LiveStatusKind::WaitingForInput => "waiting_for_input",
            LiveStatusKind::AuthRequired => "auth_required",
            LiveStatusKind::RateLimited => "rate_limited",
            LiveStatusKind::ContextLimit => "context_limit",
            LiveStatusKind::CommandFailed => "command_failed",
            LiveStatusKind::Blocked => "blocked",
            LiveStatusKind::WorktreeMissing => "worktree_missing",
            LiveStatusKind::TmuxMissing => "tmux_missing",
            LiveStatusKind::WorktrunkMissing => "worktrunk_missing",
            LiveStatusKind::MergeConflict => "merge_conflict",
            LiveStatusKind::CiFailed => "ci_failed",
            LiveStatusKind::Done => "done",
            LiveStatusKind::ShellIdle
            | LiveStatusKind::CommandRunning
            | LiveStatusKind::TestsRunning
            | LiveStatusKind::AgentRunning
            | LiveStatusKind::Unknown => "live_status",
        },
        Evidence::AgentStatus(status) => match status {
            AgentRuntimeStatus::NotStarted => "agent_not_started",
            AgentRuntimeStatus::Running => "agent_running",
            AgentRuntimeStatus::Waiting => "agent_waiting",
            AgentRuntimeStatus::Blocked => "agent_blocked",
            AgentRuntimeStatus::Done => "agent_done",
            AgentRuntimeStatus::Dead => "agent_dead",
            AgentRuntimeStatus::Unknown => "agent_status_not_observed",
        },
        Evidence::SideFlag(flag) => match flag {
            SideFlag::Dirty => "dirty",
            SideFlag::AgentRunning => "agent_running",
            SideFlag::AgentDead => "agent_dead",
            SideFlag::NeedsInput => "needs_input",
            SideFlag::TestsFailed => "tests_failed",
            SideFlag::TmuxMissing => "tmux_missing",
            SideFlag::WorktreeMissing => "worktree_missing",
            SideFlag::WorktrunkMissing => "worktrunk_missing",
            SideFlag::BranchMissing => "branch_missing",
            SideFlag::Stale => "stale",
            SideFlag::Conflicted => "conflicted",
            SideFlag::Unpushed => "unpushed",
        },
        Evidence::Lifecycle(status) => match status {
            LifecycleStatus::Created => "created",
            LifecycleStatus::Provisioning => "provisioning",
            LifecycleStatus::Active => "active",
            LifecycleStatus::Waiting => "waiting",
            LifecycleStatus::Reviewable => "reviewable",
            LifecycleStatus::Mergeable => "mergeable",
            LifecycleStatus::Merged => "merged",
            LifecycleStatus::Cleanable => "cleanable",
            LifecycleStatus::Removing => "removing",
            LifecycleStatus::TeardownIncomplete => "teardown incomplete",
            LifecycleStatus::Removed => "removed",
            LifecycleStatus::Orphaned => "orphaned",
            LifecycleStatus::Error => "error",
        },
        Evidence::Substrate(gap) => match gap {
            crate::models::SubstrateGap::WorktreeMissing => "worktree_missing",
            crate::models::SubstrateGap::TmuxMissing => "tmux_missing",
            crate::models::SubstrateGap::WorktrunkMissing => "worktrunk_missing",
            crate::models::SubstrateGap::BranchMissing => "branch_missing",
        },
        Evidence::RuntimeObservationFailed => "runtime_observation_failed",
    }
}

fn fallback_operator_action(task: &Task) -> OperatorAction {
    if task.runtime_projection.observation_error.is_some() {
        return OperatorAction::Repair;
    }
    match task.lifecycle_status {
        LifecycleStatus::Mergeable => OperatorAction::Ship,
        LifecycleStatus::Reviewable
            if merge_safety(task).classification == crate::models::SafetyClassification::Safe =>
        {
            OperatorAction::Ship
        }
        LifecycleStatus::Reviewable => OperatorAction::Review,
        LifecycleStatus::Cleanable | LifecycleStatus::Removing | LifecycleStatus::Removed => {
            OperatorAction::Drop
        }
        LifecycleStatus::Merged if crate::slices::drop::decision(task).is_available() => {
            OperatorAction::Drop
        }
        _ => OperatorAction::Resume,
    }
}

fn fallback_operator_reason(task: &Task) -> &'static str {
    if task.runtime_projection.observation_error.is_some() {
        return "repair";
    }
    match fallback_operator_action(task) {
        OperatorAction::Ship => "ship",
        OperatorAction::Review => "review",
        OperatorAction::Drop => "drop",
        OperatorAction::Repair => primary_blocker_reason(task).unwrap_or("repair"),
        OperatorAction::Resume | OperatorAction::Start => {
            match derive_operator_status(task).status {
                crate::ui_state::TaskStatus::Running => "monitor",
                crate::ui_state::TaskStatus::Waiting => "needs_input",
                crate::ui_state::TaskStatus::Error => {
                    primary_blocker_reason(task).unwrap_or("resolve_blocker")
                }
                crate::ui_state::TaskStatus::Idle => "resume",
            }
        }
    }
}

pub fn available_operator_actions(task: &Task) -> Vec<OperatorAction> {
    if task_is_known_invalid(task) {
        return vec![OperatorAction::Drop];
    }

    if task.has_missing_substrate() && !has_only_shell_substrate_gap(task) {
        return vec![OperatorAction::Repair];
    }

    let decisions = task_action_decisions(task);
    [
        OperatorAction::Repair,
        OperatorAction::Resume,
        OperatorAction::Ship,
        OperatorAction::Drop,
    ]
    .into_iter()
    .filter(|action| {
        if *action == OperatorAction::Repair
            && !task.has_missing_substrate()
            && task.runtime_projection.observation_error.is_none()
        {
            return false;
        }
        decisions.iter().any(|decision| {
            decision.id == TaskActionId::BuiltIn(*action) && decision.is_available()
        })
    })
    .collect()
}

fn has_only_shell_substrate_gap(task: &Task) -> bool {
    let facts = task.facts();
    !facts.worktree_missing
        && !facts.branch_missing
        && !task.runtime_projection.health.is_git_substrate_gap()
        && !task
            .live_status
            .as_ref()
            .is_some_and(|live| live.kind == LiveStatusKind::WorktreeMissing)
}

fn task_is_known_invalid(task: &Task) -> bool {
    crate::slices::drop::invalid_task_requires_drop(task)
}

pub fn primary_blocker_reason(task: &Task) -> Option<&'static str> {
    let facts = task.facts();
    if let Some(live) = task.live_status.as_ref() {
        if let Some(reason) = blocker_reason_for_live(live.kind) {
            return Some(reason);
        }
    }
    if facts.needs_input {
        return Some("agent needs input");
    }
    if facts.conflicted {
        return Some("git conflicts detected");
    }
    if facts.tests_failed {
        return Some("tests failed");
    }
    if facts.agent_dead {
        return Some("agent appears dead");
    }
    if facts.worktrunk_missing {
        return Some("worktrunk missing");
    }
    if facts.tmux_missing {
        return Some("tmux session missing");
    }
    if facts.worktree_missing {
        return Some("worktree missing");
    }
    if facts.branch_missing {
        return Some("branch missing");
    }
    match task.agent_status {
        AgentRuntimeStatus::Waiting => Some("agent is waiting"),
        AgentRuntimeStatus::Blocked => Some("agent is blocked"),
        AgentRuntimeStatus::Dead => Some("agent appears dead"),
        _ => None,
    }
}

fn blocker_reason_for_live(kind: LiveStatusKind) -> Option<&'static str> {
    match kind {
        LiveStatusKind::WaitingForApproval => Some("waiting for approval"),
        LiveStatusKind::WaitingForInput => Some("waiting for input"),
        LiveStatusKind::AuthRequired => Some("authentication required"),
        LiveStatusKind::RateLimited => Some("rate limited"),
        LiveStatusKind::ContextLimit => Some("context limit reached"),
        LiveStatusKind::MergeConflict => Some("merge conflict needs attention"),
        LiveStatusKind::CommandFailed => Some("command failed"),
        LiveStatusKind::CiFailed => Some("ci failed"),
        LiveStatusKind::Blocked => Some("agent is blocked"),
        LiveStatusKind::WorktreeMissing => Some("worktree missing"),
        LiveStatusKind::TmuxMissing => Some("tmux session missing"),
        LiveStatusKind::WorktrunkMissing => Some("worktrunk missing"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        operator_action, task_action_decisions, ActionAvailability, RemediationId, TaskActionId,
    };
    use crate::{
        lifecycle::{mark_active, mark_reviewable},
        models::{
            AgentClient, GitStatus, LifecycleStatus, OperatorAction, RuntimeHealth,
            RuntimeObservationSource, SideFlag, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
    };

    fn task(handle: &str) -> Task {
        Task::new(
            TaskId::new(format!("task-{handle}")),
            "web",
            handle,
            format!("Task {handle}"),
            format!("ajax/{handle}"),
            "main",
            format!("/tmp/worktrees/{handle}"),
            format!("ajax-web-{handle}"),
            "worktrunk",
            AgentClient::Codex,
        )
    }

    fn clean_reviewable_task(handle: &str) -> Task {
        let mut t = task(handle);
        mark_active(&mut t).unwrap();
        mark_reviewable(&mut t).unwrap();
        t.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some(format!("ajax/{handle}")),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123 Fix task".to_string()),
        });
        t
    }

    #[test]
    fn operator_actions_prefer_drop_when_shell_substrate_is_missing() {
        for flag in [SideFlag::TmuxMissing, SideFlag::WorktrunkMissing] {
            let mut t = clean_reviewable_task("reviewable");
            t.add_side_flag(flag);

            let plan = operator_action(&t);

            assert_eq!(plan.action, OperatorAction::Drop);
            assert_eq!(plan.available_actions, vec![OperatorAction::Drop]);
        }
    }

    #[test]
    fn operator_actions_hide_ship_when_git_substrate_is_missing() {
        for mark_missing_git_substrate in [
            |task: &mut Task| task.git_status.as_mut().unwrap().worktree_exists = false,
            |task: &mut Task| task.git_status.as_mut().unwrap().branch_exists = false,
        ] {
            let mut t = clean_reviewable_task("reviewable");
            mark_missing_git_substrate(&mut t);

            let plan = operator_action(&t);

            assert!(
                !plan.available_actions.contains(&OperatorAction::Ship),
                "missing git substrate should hide ship: {:?}",
                plan.available_actions
            );
        }
    }

    #[test]
    fn runtime_probe_failure_recommends_repair_instead_of_drop() {
        let mut t = task("probe-failed");
        mark_active(&mut t).unwrap();
        t.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Repair);
        assert_eq!(plan.reason, "runtime_observation_failed");
        assert!(plan.available_actions.contains(&OperatorAction::Repair));
        assert_ne!(plan.action, OperatorAction::Drop);
    }

    #[test]
    fn invalid_tasks_prefer_drop_for_removal() {
        for make_invalid in [
            |task: &mut Task| task.add_side_flag(SideFlag::TmuxMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::WorktrunkMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::WorktreeMissing),
            |task: &mut Task| task.add_side_flag(SideFlag::BranchMissing),
            |task: &mut Task| {
                task.tmux_status = Some(TmuxStatus {
                    exists: false,
                    session_name: task.tmux_session.clone(),
                });
            },
            |task: &mut Task| {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                });
            },
            |task: &mut Task| {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: true,
                    window_name: task.worktrunk_window.clone(),
                    current_path: "/tmp/wrong".into(),
                    points_at_expected_path: false,
                });
            },
            |task: &mut Task| task.git_status.as_mut().unwrap().worktree_exists = false,
            |task: &mut Task| task.git_status.as_mut().unwrap().branch_exists = false,
        ] {
            let mut t = clean_reviewable_task("reviewable");
            make_invalid(&mut t);

            let plan = operator_action(&t);

            assert_eq!(plan.action, OperatorAction::Drop);
            assert!(
                plan.available_actions.contains(&OperatorAction::Drop),
                "invalid task should stay removable: {:?}",
                plan.available_actions
            );
            assert!(
                !plan.available_actions.contains(&OperatorAction::Ship),
                "invalid task should not offer ship: {:?}",
                plan.available_actions
            );
        }
    }

    #[test]
    fn operator_action_prefers_runtime_health_for_shell_repair_without_hiding_ship() {
        let mut t = clean_reviewable_task("reviewable");
        t.runtime_projection.health = RuntimeHealth::MissingSession;

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Repair);
        assert_eq!(plan.reason, "tmux_missing");
        assert!(
            plan.available_actions.contains(&OperatorAction::Repair),
            "runtime health should offer repair: {:?}",
            plan.available_actions
        );
        assert!(
            plan.available_actions.contains(&OperatorAction::Ship),
            "shell runtime health should not hide ship: {:?}",
            plan.available_actions
        );
    }

    #[test]
    fn operator_action_uses_lowest_severity_annotation() {
        let mut t = task("annotated");
        t.lifecycle_status = LifecycleStatus::Reviewable;
        t.add_side_flag(SideFlag::NeedsInput);

        let plan = operator_action(&t);

        assert_eq!(plan.action, OperatorAction::Resume);
        assert_eq!(plan.reason, "needs_input");
    }

    #[test]
    fn task_action_decisions_include_builtin_and_remediation_actions() {
        let mut task = task("ci");
        task.live_status = Some(crate::models::LiveObservation::new(
            crate::models::LiveStatusKind::CiFailed,
            "ci failed",
        ));

        let decisions = task_action_decisions(&task);

        assert!(decisions
            .iter()
            .any(|decision| { decision.id == TaskActionId::BuiltIn(OperatorAction::Resume) }));
        assert!(decisions
            .iter()
            .any(|decision| { decision.id == TaskActionId::Remediation(RemediationId::FixCi) }));
    }

    #[test]
    fn start_is_project_scoped_not_task_scoped() {
        let decisions = task_action_decisions(&task("active"));

        assert!(!decisions
            .iter()
            .any(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Start)));
    }

    #[test]
    fn task_action_decisions_keep_eligibility_separate_from_surface_capability() {
        let decisions = task_action_decisions(&task("active"));
        let resume = decisions
            .iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Resume))
            .unwrap();

        assert_eq!(resume.availability, ActionAvailability::Available);
        assert_eq!(resume.id.compatibility_label(), "resume");
    }

    #[test]
    fn built_in_recommendation_is_not_displaced_by_remediation_decisions() {
        let mut task = clean_reviewable_task("ci");
        task.live_status = Some(crate::models::LiveObservation::new(
            crate::models::LiveStatusKind::CiFailed,
            "ci failed",
        ));

        let plan = operator_action(&task);

        assert_eq!(plan.action, OperatorAction::Ship);
    }

    #[test]
    fn task_action_ids_are_typed_namespaced_and_keep_compatibility_labels() {
        assert_eq!(
            TaskActionId::BuiltIn(OperatorAction::Ship).compatibility_label(),
            "ship"
        );
        assert_eq!(
            TaskActionId::Remediation(RemediationId::ResolveMergeConflicts).compatibility_label(),
            "resolve-merge-conflicts"
        );
    }

    #[test]
    fn task_action_decision_order_is_stable() {
        let mut task = clean_reviewable_task("blocked");
        task.live_status = Some(crate::models::LiveObservation::new(
            crate::models::LiveStatusKind::MergeConflict,
            "merge conflict",
        ));
        task.add_side_flag(SideFlag::TestsFailed);

        let ids = task_action_decisions(&task)
            .into_iter()
            .map(|decision| decision.id)
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                TaskActionId::BuiltIn(OperatorAction::Resume),
                TaskActionId::BuiltIn(OperatorAction::Review),
                TaskActionId::BuiltIn(OperatorAction::Ship),
                TaskActionId::BuiltIn(OperatorAction::Drop),
                TaskActionId::BuiltIn(OperatorAction::Repair),
                TaskActionId::Remediation(RemediationId::FixCi),
                TaskActionId::Remediation(RemediationId::ResolveMergeConflicts),
            ]
        );
    }

    #[test]
    fn drop_decision_carries_confirmation_requirement() {
        let decision = task_action_decisions(&task("active"))
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Drop))
            .unwrap();

        assert!(decision.requires_confirmation);
    }

    #[test]
    fn removed_task_action_decisions_preserve_existing_blocks() {
        let mut task = task("removed");
        task.lifecycle_status = LifecycleStatus::Removed;

        let decisions = task_action_decisions(&task);

        assert!(decisions
            .iter()
            .filter(|decision| matches!(decision.id, TaskActionId::BuiltIn(_)))
            .all(|decision| !decision.is_available()));
    }

    #[test]
    fn ship_decision_requires_reviewable_or_mergeable_lifecycle() {
        let decision = task_action_decisions(&task("active"))
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Ship))
            .unwrap();

        assert!(!decision.is_available());
        assert_eq!(
            decision.reason,
            "merge requires reviewable or mergeable lifecycle"
        );
    }

    #[test]
    fn drop_decision_preserves_clean_and_remove_eligibility_union() {
        let decision = task_action_decisions(&task("active"))
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Drop))
            .unwrap();

        assert!(decision.is_available());
    }

    #[test]
    fn review_decision_blocks_missing_worktree() {
        let mut task = task("review");
        task.add_side_flag(SideFlag::WorktreeMissing);

        let decision = task_action_decisions(&task)
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Review))
            .unwrap();

        assert!(!decision.is_available());
        assert!(decision.reason.contains("missing"));
    }

    #[test]
    fn resume_decision_blocks_missing_required_substrate() {
        let mut task = task("resume");
        task.add_side_flag(SideFlag::TmuxMissing);

        let decision = task_action_decisions(&task)
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Resume))
            .unwrap();

        assert!(!decision.is_available());
        assert!(decision.reason.contains("missing"));
    }

    #[test]
    fn repair_decision_remains_available_for_probe_failure() {
        let mut task = task("repair");
        task.record_runtime_probe_failure(
            RuntimeObservationSource::TmuxProbe,
            "tmux server unavailable",
        );

        let decision = task_action_decisions(&task)
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Repair))
            .unwrap();

        assert!(decision.is_available());
    }

    #[test]
    fn repair_decision_allows_recoverable_tmux_and_task_window_loss() {
        let mut task = task("repair");
        task.add_side_flag(SideFlag::TmuxMissing);
        task.add_side_flag(SideFlag::WorktrunkMissing);

        let decision = task_action_decisions(&task)
            .into_iter()
            .find(|decision| decision.id == TaskActionId::BuiltIn(OperatorAction::Repair))
            .unwrap();

        assert!(decision.is_available());
    }
}
