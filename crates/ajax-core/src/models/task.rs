use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use super::intent::{AgentClient, LifecycleStatus, TaskId, TaskIntent};
use super::observations::{
    AgentAttempt, AgentRuntimeStatus, GitStatus, LiveObservation, LiveStatusKind, SideFlag,
    TaskWindowStatus, TmuxStatus,
};
use super::projection::{Annotation, RuntimeHealth, RuntimeObservationSource, RuntimeProjection};

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Task {
    pub id: TaskId,
    pub repo: String,
    pub handle: String,
    pub title: String,
    pub branch: String,
    pub base_branch: String,
    pub worktree_path: PathBuf,
    pub tmux_session: String,
    pub task_window: String,
    pub selected_agent: AgentClient,
    pub lifecycle_status: LifecycleStatus,
    pub agent_status: AgentRuntimeStatus,
    pub git_status: Option<GitStatus>,
    pub tmux_status: Option<TmuxStatus>,
    pub task_window_status: Option<TaskWindowStatus>,
    #[serde(default)]
    pub runtime_projection: RuntimeProjection,
    #[serde(default)]
    pub live_status: Option<LiveObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_status_observed_at: Option<SystemTime>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    pub created_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub metadata: HashMap<String, String>,
    pub agent_attempts: Vec<AgentAttempt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attention_acknowledged_at: Option<SystemTime>,
    side_flags: BTreeSet<SideFlag>,
}

impl Task {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: TaskId,
        repo: impl Into<String>,
        handle: impl Into<String>,
        title: impl Into<String>,
        branch: impl Into<String>,
        base_branch: impl Into<String>,
        worktree_path: impl Into<PathBuf>,
        tmux_session: impl Into<String>,
        task_window: impl Into<String>,
        selected_agent: AgentClient,
    ) -> Self {
        let now = SystemTime::now();

        Self {
            id,
            repo: repo.into(),
            handle: handle.into(),
            title: title.into(),
            branch: branch.into(),
            base_branch: base_branch.into(),
            worktree_path: worktree_path.into(),
            tmux_session: tmux_session.into(),
            task_window: task_window.into(),
            selected_agent,
            lifecycle_status: LifecycleStatus::Created,
            agent_status: AgentRuntimeStatus::NotStarted,
            git_status: None,
            tmux_status: None,
            task_window_status: None,
            runtime_projection: RuntimeProjection::default(),
            live_status: None,
            live_status_observed_at: None,
            annotations: Vec::new(),
            created_at: now,
            last_activity_at: now,
            metadata: HashMap::new(),
            agent_attempts: Vec::new(),
            attention_acknowledged_at: None,
            side_flags: BTreeSet::new(),
        }
    }

    /// Record that the operator acknowledged this task's attention at `at`.
    /// Keeps the latest timestamp so an earlier acknowledgment cannot override a
    /// newer one.
    pub fn record_attention_acknowledgment(&mut self, at: SystemTime) {
        let latest = match self.attention_acknowledged_at {
            Some(existing) if existing >= at => existing,
            _ => at,
        };
        self.attention_acknowledged_at = Some(latest);
    }

    pub fn qualified_handle(&self) -> String {
        format!("{}/{}", self.repo, self.handle)
    }

    pub fn intent(&self) -> TaskIntent {
        TaskIntent {
            id: self.id.clone(),
            repo: self.repo.clone(),
            handle: self.handle.clone(),
            title: self.title.clone(),
            branch: self.branch.clone(),
            base_branch: self.base_branch.clone(),
            worktree_path: self.worktree_path.clone(),
            tmux_session: self.tmux_session.clone(),
            task_window: self.task_window.clone(),
            selected_agent: self.selected_agent,
        }
    }

    pub fn add_side_flag(&mut self, flag: SideFlag) {
        self.side_flags.insert(flag);
    }

    pub fn remove_side_flag(&mut self, flag: SideFlag) {
        self.side_flags.remove(&flag);
    }

    pub fn has_side_flag(&self, flag: SideFlag) -> bool {
        self.side_flags.contains(&flag)
    }

    pub fn side_flags(&self) -> impl Iterator<Item = SideFlag> + '_ {
        self.side_flags.iter().copied()
    }

    pub fn mark_resource_missing(&mut self, flag: SideFlag) {
        self.add_side_flag(flag);
        if flag.is_missing_substrate() {
            self.agent_status = AgentRuntimeStatus::Dead;
            self.remove_side_flag(SideFlag::AgentRunning);
        }
    }

    pub fn has_missing_substrate(&self) -> bool {
        self.has_missing_git_substrate()
            || self.side_flags().any(SideFlag::is_missing_substrate)
            || self.runtime_projection.health.is_missing_substrate()
            || self
                .live_status
                .as_ref()
                .is_some_and(|live_status| live_status.kind.is_missing_substrate())
    }

    pub fn has_missing_worktree(&self) -> bool {
        self.has_side_flag(SideFlag::WorktreeMissing)
            || self
                .git_status
                .as_ref()
                .is_some_and(|status| !status.worktree_exists)
            || self.runtime_projection.health == RuntimeHealth::MissingWorktree
            || self
                .live_status
                .as_ref()
                .is_some_and(|live| live.kind == LiveStatusKind::WorktreeMissing)
    }

    pub fn has_missing_branch(&self) -> bool {
        self.has_side_flag(SideFlag::BranchMissing)
            || self
                .git_status
                .as_ref()
                .is_some_and(|status| !status.branch_exists)
    }

    pub fn has_checkout_mismatch(&self) -> bool {
        self.git_status.as_ref().is_some_and(|status| {
            status.worktree_exists && status.current_branch.as_deref() != Some(self.branch.as_str())
        })
    }

    pub fn checkout_mismatch_explanation(&self) -> Option<String> {
        if !self.has_checkout_mismatch()
            && self.runtime_projection.health != RuntimeHealth::CheckoutMismatch
        {
            return None;
        }
        let expected = self.branch.as_str();
        let observed = self.git_status.as_ref().and_then(|status| {
            status
                .current_branch
                .as_deref()
                .map(|branch| format!("Worktree on {branch}; expected {expected}"))
        });
        Some(observed.unwrap_or_else(|| format!("Worktree detached; expected {expected}")))
    }

    pub fn has_missing_git_substrate(&self) -> bool {
        self.has_missing_worktree() || self.has_missing_branch()
    }

    pub fn apply_git_status(&mut self, status: GitStatus) {
        if status.worktree_exists {
            self.remove_side_flag(SideFlag::WorktreeMissing);
        } else {
            self.mark_resource_missing(SideFlag::WorktreeMissing);
        }

        if status.branch_exists {
            self.remove_side_flag(SideFlag::BranchMissing);
        } else {
            self.mark_resource_missing(SideFlag::BranchMissing);
        }

        if status.dirty || status.untracked_files > 0 {
            self.add_side_flag(SideFlag::Dirty);
        } else {
            self.remove_side_flag(SideFlag::Dirty);
        }

        if status.conflicted {
            self.add_side_flag(SideFlag::Conflicted);
        } else {
            self.remove_side_flag(SideFlag::Conflicted);
        }

        if status.has_unpushed_work() {
            self.add_side_flag(SideFlag::Unpushed);
        } else {
            self.remove_side_flag(SideFlag::Unpushed);
        }

        self.git_status = Some(status);
        self.refresh_runtime_projection();
    }

    pub fn apply_tmux_status(&mut self, status: Option<TmuxStatus>) {
        match status.as_ref() {
            Some(status) if status.exists => self.remove_side_flag(SideFlag::TmuxMissing),
            Some(_) | None => self.mark_resource_missing(SideFlag::TmuxMissing),
        }

        self.tmux_status = status;
        self.refresh_runtime_projection();
    }

    pub fn apply_task_window_status(&mut self, status: Option<TaskWindowStatus>) {
        match status.as_ref() {
            Some(status) if status.exists && status.points_at_expected_path => {
                self.remove_side_flag(SideFlag::TaskWindowMissing);
            }
            Some(_) | None => self.mark_resource_missing(SideFlag::TaskWindowMissing),
        }

        self.task_window_status = status;
        self.refresh_runtime_projection();
    }

    pub(crate) fn refresh_runtime_projection(&mut self) {
        self.refresh_runtime_projection_from_source(RuntimeObservationSource::Unknown);
    }

    pub fn refresh_runtime_projection_from_source(&mut self, source: RuntimeObservationSource) {
        self.runtime_projection = crate::runtime::reconcile_runtime(
            &crate::runtime::ObservedTaskRuntime {
                git_status: self.git_status.clone(),
                tmux_status: self.tmux_status.clone(),
                task_window_status: self.task_window_status.clone(),
            },
            &self.branch,
            SystemTime::now(),
            source,
        );
    }

    pub fn record_runtime_probe_failure(
        &mut self,
        source: RuntimeObservationSource,
        error: impl Into<String>,
    ) {
        let previous_health = self.runtime_projection.health;
        self.runtime_projection = RuntimeProjection::with_observation_error(
            previous_health,
            SystemTime::now(),
            source,
            error,
        );
    }
}
