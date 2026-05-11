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
    CiFailed,
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
    OpenTask,
    MergeTask,
    CleanTask,
    Status,
}

const TASK_PICKER_MENU: [RecommendedAction; 3] = [
    RecommendedAction::OpenTask,
    RecommendedAction::MergeTask,
    RecommendedAction::CleanTask,
];

impl RecommendedAction {
    pub const fn all() -> &'static [Self] {
        &[
            Self::SelectProject,
            Self::NewTask,
            Self::OpenTask,
            Self::MergeTask,
            Self::CleanTask,
            Self::Status,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SelectProject => "select project",
            Self::NewTask => "new task",
            Self::OpenTask => "open task",
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

    pub const fn task_picker_menu() -> &'static [Self] {
        &TASK_PICKER_MENU
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AgentAttempt, AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, LiveObservation,
        LiveStatusKind, RecommendedAction, Repo, SideFlag, Task, TaskId, TmuxStatus,
        WorktrunkStatus,
    };
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn text_strategy() -> impl Strategy<Value = String> {
        "\\PC{0,64}"
    }

    fn side_flag_strategy() -> impl Strategy<Value = SideFlag> {
        prop::sample::select(
            [
                SideFlag::Dirty,
                SideFlag::AgentRunning,
                SideFlag::AgentDead,
                SideFlag::NeedsInput,
                SideFlag::TestsFailed,
                SideFlag::TmuxMissing,
                SideFlag::WorktreeMissing,
                SideFlag::WorktrunkMissing,
                SideFlag::BranchMissing,
                SideFlag::Stale,
                SideFlag::Conflicted,
                SideFlag::Unpushed,
            ]
            .to_vec(),
        )
    }

    fn live_status_kind_strategy() -> impl Strategy<Value = LiveStatusKind> {
        prop::sample::select(
            [
                LiveStatusKind::WorktreeMissing,
                LiveStatusKind::TmuxMissing,
                LiveStatusKind::WorktrunkMissing,
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
            ]
            .to_vec(),
        )
    }

    fn sample_task() -> Task {
        Task::new(
            TaskId::new("task-generated"),
            "web",
            "generated",
            "Generated task",
            "ajax/generated",
            "main",
            "/tmp/worktrees/generated",
            "ajax-web-generated",
            "worktrunk",
            AgentClient::Codex,
        )
    }

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

    proptest! {
        #[test]
        fn task_identity_and_handle_preserve_generated_inputs(
            id in text_strategy(),
            repo in text_strategy(),
            handle in text_strategy(),
            title in text_strategy(),
            branch in text_strategy(),
            base_branch in text_strategy(),
            worktree_path in text_strategy(),
            tmux_session in text_strategy(),
            worktrunk_window in text_strategy(),
        ) {
            let task = Task::new(
                TaskId::new(&id),
                &repo,
                &handle,
                &title,
                &branch,
                &base_branch,
                &worktree_path,
                &tmux_session,
                &worktrunk_window,
                AgentClient::Codex,
            );

            prop_assert_eq!(task.id.as_str(), id);
            prop_assert_eq!(&task.repo, &repo);
            prop_assert_eq!(&task.handle, &handle);
            prop_assert_eq!(&task.title, &title);
            prop_assert_eq!(&task.branch, &branch);
            prop_assert_eq!(&task.base_branch, &base_branch);
            prop_assert_eq!(&task.worktree_path, std::path::Path::new(&worktree_path));
            prop_assert_eq!(&task.tmux_session, &tmux_session);
            prop_assert_eq!(&task.worktrunk_window, &worktrunk_window);
            prop_assert_eq!(task.qualified_handle(), format!("{repo}/{handle}"));
        }

        #[test]
        fn repo_tmux_and_worktrunk_constructors_preserve_generated_inputs(
            repo_name in text_strategy(),
            repo_path in text_strategy(),
            default_branch in text_strategy(),
            tmux_session in text_strategy(),
            worktrunk_window in text_strategy(),
            worktrunk_path in text_strategy(),
        ) {
            let repo = Repo::new(&repo_name, &repo_path, &default_branch);
            prop_assert_eq!(repo.name, repo_name);
            prop_assert_eq!(repo.path, std::path::Path::new(&repo_path));
            prop_assert_eq!(repo.default_branch, default_branch);

            let tmux = TmuxStatus::present(&tmux_session);
            prop_assert!(tmux.exists);
            prop_assert_eq!(tmux.session_name, tmux_session);

            let worktrunk = WorktrunkStatus::present(&worktrunk_window, &worktrunk_path);
            prop_assert!(worktrunk.exists);
            prop_assert_eq!(worktrunk.window_name, worktrunk_window);
            prop_assert_eq!(worktrunk.current_path, std::path::Path::new(&worktrunk_path));
            prop_assert!(worktrunk.points_at_expected_path);
        }
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

    proptest! {
        #[test]
        fn side_flags_are_unique_sorted_and_removable(
            flags in prop::collection::vec(side_flag_strategy(), 0..32),
            removed in side_flag_strategy(),
        ) {
            let mut task = sample_task();
            let mut expected = BTreeSet::new();

            for flag in flags {
                task.add_side_flag(flag);
                task.add_side_flag(flag);
                expected.insert(flag);
            }

            prop_assert_eq!(task.side_flags().collect::<Vec<_>>(), expected.iter().copied().collect::<Vec<_>>());

            task.remove_side_flag(removed);
            expected.remove(&removed);

            prop_assert_eq!(task.side_flags().collect::<Vec<_>>(), expected.iter().copied().collect::<Vec<_>>());
        }

        #[test]
        fn mark_resource_missing_resets_agent_state_only_for_missing_substrate_flags(
            flag in side_flag_strategy(),
        ) {
            let mut task = sample_task();
            task.agent_status = AgentRuntimeStatus::Running;
            task.add_side_flag(SideFlag::AgentRunning);

            task.mark_resource_missing(flag);

            prop_assert!(task.has_side_flag(flag));
            if flag.is_missing_substrate() {
                prop_assert_eq!(task.agent_status, AgentRuntimeStatus::Unknown);
                prop_assert!(!task.has_side_flag(SideFlag::AgentRunning));
            } else {
                prop_assert_eq!(task.agent_status, AgentRuntimeStatus::Running);
                prop_assert!(task.has_side_flag(SideFlag::AgentRunning));
            }
        }

        #[test]
        fn has_missing_substrate_matches_missing_flags_or_live_status(
            flags in prop::collection::vec(side_flag_strategy(), 0..32),
            live_kind in prop::option::of(live_status_kind_strategy()),
        ) {
            let mut task = sample_task();
            let expected_from_flags = flags.iter().copied().any(SideFlag::is_missing_substrate);
            let expected_from_live = live_kind.is_some_and(LiveStatusKind::is_missing_substrate);

            for flag in flags {
                task.add_side_flag(flag);
            }
            if let Some(kind) = live_kind {
                task.live_status = Some(LiveObservation::new(kind, "generated status"));
            }

            prop_assert_eq!(task.has_missing_substrate(), expected_from_flags || expected_from_live);
        }
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

        assert_eq!(labels.len(), 6);
        assert_eq!(labels[0], "select project");
        assert_eq!(labels[1], "new task");
        assert!(!labels.contains(&"reconcile"));
        for label in labels {
            assert_eq!(
                RecommendedAction::from_label(label).map(|action| action.as_str()),
                Some(label)
            );
        }
    }

    #[test]
    fn recommended_actions_define_task_picker_menus() {
        let labels = RecommendedAction::task_picker_menu()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>();

        assert_eq!(labels, vec!["open task", "merge task", "clean task"]);
    }

    #[test]
    fn recommended_actions_do_not_include_legacy_task_aliases() {
        let labels = RecommendedAction::all()
            .iter()
            .map(|action| action.as_str())
            .collect::<Vec<_>>();

        for legacy_label in [
            "open worktrunk",
            "inspect task",
            "inspect agent",
            "inspect test output",
            "monitor task",
            "check task",
            "diff task",
            "review diff",
            "review branch",
        ] {
            assert!(!labels.contains(&legacy_label), "{legacy_label}");
        }
    }
}
