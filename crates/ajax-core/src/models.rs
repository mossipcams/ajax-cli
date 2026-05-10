use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Deserialize, Serialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AgentClient {
    Claude,
    Codex,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LifecycleStatus {
    Created,
    Provisioning,
    Active,
    Waiting,
    Reviewable,
    Mergeable,
    Merged,
    Cleanable,
    Removed,
    Orphaned,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
pub enum SideFlag {
    Dirty,
    AgentRunning,
    AgentDead,
    NeedsInput,
    TestsFailed,
    TmuxMissing,
    WorktreeMissing,
    WorktrunkMissing,
    BranchMissing,
    Stale,
    Conflicted,
    Unpushed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum LiveStatusKind {
    WorktreeMissing,
    TmuxMissing,
    WorktrunkMissing,
    ShellIdle,
    CommandRunning,
    TestsRunning,
    AgentRunning,
    WaitingForApproval,
    WaitingForInput,
    Blocked,
    RateLimited,
    AuthRequired,
    MergeConflict,
    ContextLimit,
    CommandFailed,
    Done,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct LiveObservation {
    pub kind: LiveStatusKind,
    pub summary: String,
}

impl LiveObservation {
    pub fn new(kind: LiveStatusKind, summary: impl Into<String>) -> Self {
        Self {
            kind,
            summary: summary.into(),
        }
    }
}

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
    pub worktrunk_window: String,
    pub selected_agent: AgentClient,
    pub lifecycle_status: LifecycleStatus,
    pub agent_status: AgentRuntimeStatus,
    pub git_status: Option<GitStatus>,
    pub tmux_status: Option<TmuxStatus>,
    pub worktrunk_status: Option<WorktrunkStatus>,
    #[serde(default)]
    pub live_status: Option<LiveObservation>,
    pub created_at: SystemTime,
    pub last_activity_at: SystemTime,
    pub metadata: HashMap<String, String>,
    pub agent_attempts: Vec<AgentAttempt>,
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
        worktrunk_window: impl Into<String>,
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
            worktrunk_window: worktrunk_window.into(),
            selected_agent,
            lifecycle_status: LifecycleStatus::Created,
            agent_status: AgentRuntimeStatus::NotStarted,
            git_status: None,
            tmux_status: None,
            worktrunk_status: None,
            live_status: None,
            created_at: now,
            last_activity_at: now,
            metadata: HashMap::new(),
            agent_attempts: Vec::new(),
            side_flags: BTreeSet::new(),
        }
    }

    pub fn qualified_handle(&self) -> String {
        format!("{}/{}", self.repo, self.handle)
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
            self.agent_status = AgentRuntimeStatus::Unknown;
            self.remove_side_flag(SideFlag::AgentRunning);
        }
    }

    pub fn has_missing_substrate(&self) -> bool {
        self.side_flags().any(SideFlag::is_missing_substrate)
            || self
                .live_status
                .as_ref()
                .is_some_and(|live_status| live_status.kind.is_missing_substrate())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum AgentRuntimeStatus {
    NotStarted,
    Running,
    Waiting,
    Blocked,
    Dead,
    Done,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct Repo {
    pub name: String,
    pub path: PathBuf,
    pub default_branch: String,
}

impl Repo {
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        default_branch: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            default_branch: default_branch.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AgentAttempt {
    pub agent: AgentClient,
    pub launch_target: String,
    pub started_at: SystemTime,
    pub finished_at: Option<SystemTime>,
    pub status: AgentRuntimeStatus,
}

impl AgentAttempt {
    pub fn new(agent: AgentClient, launch_target: impl Into<String>) -> Self {
        Self {
            agent,
            launch_target: launch_target.into(),
            started_at: SystemTime::now(),
            finished_at: None,
            status: AgentRuntimeStatus::Running,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct GitStatus {
    pub worktree_exists: bool,
    pub branch_exists: bool,
    #[serde(default)]
    pub current_branch: Option<String>,
    pub dirty: bool,
    pub ahead: u32,
    pub behind: u32,
    pub merged: bool,
    pub untracked_files: u32,
    pub unpushed_commits: u32,
    pub conflicted: bool,
    pub last_commit: Option<String>,
}

impl GitStatus {
    pub fn has_unpushed_work(&self) -> bool {
        self.unpushed_commits > 0 || self.ahead > 0
    }
}

impl SideFlag {
    pub fn is_missing_substrate(self) -> bool {
        matches!(
            self,
            SideFlag::WorktrunkMissing
                | SideFlag::TmuxMissing
                | SideFlag::WorktreeMissing
                | SideFlag::BranchMissing
        )
    }
}

impl LiveStatusKind {
    pub fn is_missing_substrate(self) -> bool {
        matches!(
            self,
            LiveStatusKind::WorktreeMissing
                | LiveStatusKind::TmuxMissing
                | LiveStatusKind::WorktrunkMissing
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct TmuxStatus {
    pub exists: bool,
    pub session_name: String,
}

impl TmuxStatus {
    pub fn present(session_name: impl Into<String>) -> Self {
        Self {
            exists: true,
            session_name: session_name.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct WorktrunkStatus {
    pub exists: bool,
    pub window_name: String,
    pub current_path: PathBuf,
    pub points_at_expected_path: bool,
}

impl WorktrunkStatus {
    pub fn present(window_name: impl Into<String>, current_path: impl Into<PathBuf>) -> Self {
        Self {
            exists: true,
            window_name: window_name.into(),
            current_path: current_path.into(),
            points_at_expected_path: true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub enum SafetyClassification {
    Safe,
    NeedsConfirmation,
    Dangerous,
    Blocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct SafetyReport {
    pub classification: SafetyClassification,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
pub struct AttentionItem {
    pub task_id: TaskId,
    pub task_handle: String,
    pub reason: String,
    pub priority: u32,
    pub recommended_action: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum RecommendedAction {
    SelectProject,
    NewTask,
    Reconcile,
    OpenTask,
    OpenWorktrunk,
    InspectTask,
    InspectAgent,
    InspectTestOutput,
    MonitorTask,
    CheckTask,
    DiffTask,
    ReviewDiff,
    ReviewBranch,
    MergeTask,
    CleanTask,
    Status,
}

const TASK_PICKER_MENU: [RecommendedAction; 8] = [
    RecommendedAction::OpenTask,
    RecommendedAction::DiffTask,
    RecommendedAction::CheckTask,
    RecommendedAction::MergeTask,
    RecommendedAction::ReviewBranch,
    RecommendedAction::OpenWorktrunk,
    RecommendedAction::InspectTask,
    RecommendedAction::CleanTask,
];

const REVIEW_TASK_PICKER_MENU: [RecommendedAction; 8] = [
    RecommendedAction::ReviewBranch,
    RecommendedAction::OpenTask,
    RecommendedAction::DiffTask,
    RecommendedAction::CheckTask,
    RecommendedAction::MergeTask,
    RecommendedAction::OpenWorktrunk,
    RecommendedAction::InspectTask,
    RecommendedAction::CleanTask,
];

impl RecommendedAction {
    pub const fn all() -> &'static [Self] {
        &[
            Self::SelectProject,
            Self::NewTask,
            Self::Reconcile,
            Self::OpenTask,
            Self::OpenWorktrunk,
            Self::InspectTask,
            Self::InspectAgent,
            Self::InspectTestOutput,
            Self::MonitorTask,
            Self::CheckTask,
            Self::DiffTask,
            Self::ReviewDiff,
            Self::ReviewBranch,
            Self::MergeTask,
            Self::CleanTask,
            Self::Status,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelectProject => "select project",
            Self::NewTask => "new task",
            Self::Reconcile => "reconcile",
            Self::OpenTask => "open task",
            Self::OpenWorktrunk => "open worktrunk",
            Self::InspectTask => "inspect task",
            Self::InspectAgent => "inspect agent",
            Self::InspectTestOutput => "inspect test output",
            Self::MonitorTask => "monitor task",
            Self::CheckTask => "check task",
            Self::DiffTask => "diff task",
            Self::ReviewDiff => "review diff",
            Self::ReviewBranch => "review branch",
            Self::MergeTask => "merge task",
            Self::CleanTask => "clean task",
            Self::Status => "status",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|action| action.as_str() == label)
    }

    pub const fn task_picker_menu(is_review: bool) -> &'static [Self] {
        if is_review {
            &REVIEW_TASK_PICKER_MENU
        } else {
            &TASK_PICKER_MENU
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentAttempt, AgentClient, GitStatus, LifecycleStatus, RecommendedAction, Repo, SideFlag,
        Task, TaskId, TmuxStatus, WorktrunkStatus,
    };

    #[test]
    fn task_identity_maps_to_repo_handle() {
        let task = Task::new(
            TaskId::new("task-1"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/tmp/worktrees/web-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );

        assert_eq!(task.qualified_handle(), "web/fix-login");
        assert_eq!(task.lifecycle_status, LifecycleStatus::Created);
        assert_eq!(task.agent_attempts.len(), 0);
        assert_eq!(task.selected_agent, AgentClient::Codex);
    }

    #[test]
    fn task_tracks_advisory_side_flags() {
        let mut task = Task::new(
            TaskId::new("task-2"),
            "api",
            "add-cache",
            "Add cache",
            "ajax/add-cache",
            "main",
            "/tmp/worktrees/api-add-cache",
            "ajax-api-add-cache",
            "worktrunk",
            AgentClient::Claude,
        );

        task.add_side_flag(SideFlag::Dirty);
        task.add_side_flag(SideFlag::AgentRunning);

        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::AgentRunning));
        assert!(!task.has_side_flag(SideFlag::Conflicted));
    }

    #[test]
    fn task_marks_and_detects_missing_substrate() {
        let mut task = Task::new(
            TaskId::new("task-3"),
            "web",
            "repair-cockpit",
            "Repair cockpit",
            "ajax/repair-cockpit",
            "main",
            "/tmp/worktrees/repair-cockpit",
            "ajax-web-repair-cockpit",
            "worktrunk",
            AgentClient::Codex,
        );

        task.agent_status = super::AgentRuntimeStatus::Running;
        task.add_side_flag(SideFlag::AgentRunning);
        task.mark_resource_missing(SideFlag::WorktreeMissing);

        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_missing_substrate());
        assert_eq!(task.agent_status, super::AgentRuntimeStatus::Unknown);
        assert!(!task.has_side_flag(SideFlag::AgentRunning));
    }

    #[test]
    fn repo_and_status_models_capture_external_reality() {
        let repo = Repo::new("web", "/Users/matt/projects/web", "main");
        let attempt = AgentAttempt::new(AgentClient::Codex, "tmux:%1");
        let git = GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 1,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 1,
            conflicted: false,
            last_commit: Some("abc123 Fix login".to_string()),
        };
        let tmux = TmuxStatus::present("ajax-web-fix-login");
        let worktrunk = WorktrunkStatus::present("worktrunk", "/Users/matt/projects/web");

        assert_eq!(repo.default_branch, "main");
        assert_eq!(attempt.agent, AgentClient::Codex);
        assert!(git.has_unpushed_work());
        assert!(tmux.exists);
        assert!(worktrunk.points_at_expected_path);
    }

    #[test]
    fn recommended_actions_have_stable_unique_labels() {
        let labels = RecommendedAction::all()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels.len(), 16);
        assert_eq!(labels[0], "select project");
        assert_eq!(labels[1], "new task");
        assert_eq!(labels[2], "reconcile");
        for label in labels {
            assert_eq!(
                RecommendedAction::from_label(label).map(|action| action.as_str()),
                Some(label)
            );
        }
    }

    #[test]
    fn recommended_actions_define_task_picker_menus() {
        let labels = |is_review| {
            RecommendedAction::task_picker_menu(is_review)
                .iter()
                .map(|action| action.as_str())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            labels(false),
            vec![
                "open task",
                "diff task",
                "check task",
                "merge task",
                "review branch",
                "open worktrunk",
                "inspect task",
                "clean task",
            ]
        );
        assert_eq!(
            labels(true),
            vec![
                "review branch",
                "open task",
                "diff task",
                "check task",
                "merge task",
                "open worktrunk",
                "inspect task",
                "clean task",
            ]
        );
    }
}
