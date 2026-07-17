//! Shared Ajax Dev deployment slot ("Test in Dev").
//!
//! Stable Ajax resolves an ajax-cli task's registered worktree, builds that
//! worktree as-is, and restarts only the existing `ajax-web-dev` instance.
//! Clients never supply a filesystem path.

use ajax_core::{commands::CommandContext, config::ManagedRepo, models::Task, registry::Registry};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, MutexGuard},
    time::{SystemTime, UNIX_EPOCH},
};

pub const AJAX_SELF_REPO: &str = "ajax-cli";
pub const DEV_OPEN_URL: &str = "https://ajaxdev.mossyhome.net:8788";
pub const DEV_PROFILE: &str = "dev";
pub const DEV_PORT: u16 = 8788;

const RESTART_SCRIPT_ENV: &str = "AJAX_WEB_RESTART_SCRIPT";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevDeployPhase {
    ReadyToDeploy,
    Building,
    Restarting,
    DevReady,
    Failed,
}

impl DevDeployPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadyToDeploy => "Ready to deploy",
            Self::Building => "Building",
            Self::Restarting => "Restarting",
            Self::DevReady => "Dev ready",
            Self::Failed => "Failed",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Building | Self::Restarting)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DevSlotOccupant {
    pub task_handle: String,
    pub title: String,
    pub branch: String,
    pub commit_sha: String,
    pub dirty: bool,
    pub deployed_at_unix_secs: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DevDeploySource {
    pub task_handle: String,
    pub title: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub commit_sha: String,
    pub dirty: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DevDeployStatus {
    pub phase: DevDeployPhase,
    pub phase_label: String,
    pub shared_slot: bool,
    pub open_url: String,
    pub active: bool,
    pub error: Option<String>,
    pub occupant: Option<DevSlotOccupant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DevDeployError {
    TaskNotFound(String),
    NotAjaxRepo { repo: String },
    RepoNotConfigured,
    WorktreeMissing(PathBuf),
    WorktreeNotManaged { path: PathBuf, reason: String },
    Busy,
    RestartScriptMissing,
    SpawnFailed(String),
}

impl std::fmt::Display for DevDeployError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TaskNotFound(handle) => write!(f, "task not found: {handle}"),
            Self::NotAjaxRepo { repo } => {
                write!(
                    f,
                    "Test in Dev is only available for {AJAX_SELF_REPO} tasks (got {repo})"
                )
            }
            Self::RepoNotConfigured => {
                write!(f, "{AJAX_SELF_REPO} is not configured in this Ajax runtime")
            }
            Self::WorktreeMissing(path) => {
                write!(f, "worktree path does not exist: {}", path.display())
            }
            Self::WorktreeNotManaged { path, reason } => {
                write!(
                    f,
                    "worktree is not an Ajax-managed {AJAX_SELF_REPO} path ({}): {reason}",
                    path.display()
                )
            }
            Self::Busy => write!(f, "a Test in Dev deployment is already in progress"),
            Self::RestartScriptMissing => write!(
                f,
                "AJAX_WEB_RESTART_SCRIPT is not set; start Ajax via scripts/dev-web-restart.sh"
            ),
            Self::SpawnFailed(message) => write!(f, "failed to start deployment: {message}"),
        }
    }
}

#[derive(Debug, Default)]
pub struct DevDeploySlot {
    phase: DevDeployPhase,
    error: Option<String>,
    occupant: Option<DevSlotOccupant>,
}

impl Default for DevDeployPhase {
    fn default() -> Self {
        Self::ReadyToDeploy
    }
}

impl DevDeploySlot {
    pub fn status(&self) -> DevDeployStatus {
        DevDeployStatus {
            phase: self.phase,
            phase_label: self.phase.label().to_string(),
            shared_slot: true,
            open_url: DEV_OPEN_URL.to_string(),
            active: self.phase.is_active(),
            error: self.error.clone(),
            occupant: self.occupant.clone(),
        }
    }

    pub fn begin(&mut self, source: &DevDeploySource) -> Result<(), DevDeployError> {
        if self.phase.is_active() {
            return Err(DevDeployError::Busy);
        }
        self.phase = DevDeployPhase::Building;
        self.error = None;
        self.occupant = Some(DevSlotOccupant {
            task_handle: source.task_handle.clone(),
            title: source.title.clone(),
            branch: source.branch.clone(),
            commit_sha: source.commit_sha.clone(),
            dirty: source.dirty,
            deployed_at_unix_secs: 0,
        });
        Ok(())
    }

    pub fn set_restarting(&mut self) {
        self.phase = DevDeployPhase::Restarting;
        self.error = None;
    }

    pub fn set_ready(&mut self, source: &DevDeploySource) {
        self.phase = DevDeployPhase::DevReady;
        self.error = None;
        self.occupant = Some(DevSlotOccupant {
            task_handle: source.task_handle.clone(),
            title: source.title.clone(),
            branch: source.branch.clone(),
            commit_sha: source.commit_sha.clone(),
            dirty: source.dirty,
            deployed_at_unix_secs: unix_secs_now(),
        });
    }

    pub fn set_failed(&mut self, message: impl Into<String>) {
        self.phase = DevDeployPhase::Failed;
        self.error = Some(message.into());
    }
}

pub type SharedDevDeploySlot = Mutex<DevDeploySlot>;

pub fn lock_slot(slot: &SharedDevDeploySlot) -> MutexGuard<'_, DevDeploySlot> {
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Resolve a trusted Ajax-managed ajax-cli worktree from registry state.
///
/// The client supplies only `task_handle`. The filesystem path comes from the
/// task record and is validated against the configured ajax-cli repo.
pub fn resolve_ajax_dev_deploy_source<R: Registry>(
    context: &CommandContext<R>,
    task_handle: &str,
) -> Result<DevDeploySource, DevDeployError> {
    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == task_handle)
        .cloned()
        .ok_or_else(|| DevDeployError::TaskNotFound(task_handle.to_string()))?;

    if task.repo != AJAX_SELF_REPO {
        return Err(DevDeployError::NotAjaxRepo {
            repo: task.repo.clone(),
        });
    }

    let managed = configured_ajax_repo(context).ok_or(DevDeployError::RepoNotConfigured)?;
    validate_managed_ajax_worktree(managed, &task)?;

    let (commit_sha, dirty) = source_revision(&task);
    Ok(DevDeploySource {
        task_handle: task.qualified_handle(),
        title: task.title.clone(),
        branch: task.branch.clone(),
        worktree_path: task.worktree_path.clone(),
        commit_sha,
        dirty,
    })
}

fn configured_ajax_repo<R: Registry>(context: &CommandContext<R>) -> Option<&ManagedRepo> {
    context
        .config
        .repos
        .iter()
        .find(|repo| repo.name == AJAX_SELF_REPO)
}

fn validate_managed_ajax_worktree(
    managed: &ManagedRepo,
    task: &Task,
) -> Result<(), DevDeployError> {
    let worktree = &task.worktree_path;
    if !worktree.is_dir() {
        return Err(DevDeployError::WorktreeMissing(worktree.clone()));
    }

    let worktrees_root = legacy_worktrees_root(&managed.path);
    let under_legacy = worktree.starts_with(&worktrees_root);
    if !under_legacy {
        // Still allow when git common-dir matches (covers rooted placements and
        // odd but still Ajax-owned worktrees of the same repo object).
        if !same_git_common_dir(&managed.path, worktree) {
            return Err(DevDeployError::WorktreeNotManaged {
                path: worktree.clone(),
                reason: format!(
                    "path is outside {} and does not share git common-dir with {}",
                    worktrees_root.display(),
                    managed.path.display()
                ),
            });
        }
    } else if !same_git_common_dir(&managed.path, worktree) {
        return Err(DevDeployError::WorktreeNotManaged {
            path: worktree.clone(),
            reason: "git common-dir does not match the configured ajax-cli repository".to_string(),
        });
    }

    Ok(())
}

fn legacy_worktrees_root(repo_path: &Path) -> PathBuf {
    let repo_dir = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    repo_path
        .parent()
        .unwrap_or(repo_path)
        .join(format!("{repo_dir}__worktrees"))
}

fn same_git_common_dir(repo_path: &Path, worktree_path: &Path) -> bool {
    match (git_common_dir(repo_path), git_common_dir(worktree_path)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

fn git_common_dir(path: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args([
            "-C",
            path.to_str()?,
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn source_revision(task: &Task) -> (String, bool) {
    let dirty = task.git_status.as_ref().is_some_and(|git| git.dirty)
        || worktree_is_dirty(&task.worktree_path);
    let sha = task
        .git_status
        .as_ref()
        .and_then(|git| git.last_commit.as_deref())
        .and_then(short_sha_from_last_commit)
        .or_else(|| git_short_sha(&task.worktree_path))
        .unwrap_or_else(|| "unknown".to_string());
    (sha, dirty)
}

fn short_sha_from_last_commit(last_commit: &str) -> Option<String> {
    let token = last_commit.split_whitespace().next()?;
    if token.is_empty() {
        return None;
    }
    Some(token.chars().take(12).collect())
}

fn git_short_sha(worktree: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", worktree.to_str()?, "rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

fn worktree_is_dirty(worktree: &Path) -> bool {
    let output = Command::new("git")
        .args([
            "-C",
            worktree.to_str().unwrap_or(""),
            "status",
            "--porcelain",
        ])
        .output();
    match output {
        Ok(output) if output.status.success() => !output.stdout.is_empty(),
        _ => false,
    }
}

fn unix_secs_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub fn restart_script_from_env() -> Result<PathBuf, DevDeployError> {
    let path = std::env::var_os(RESTART_SCRIPT_ENV)
        .map(PathBuf::from)
        .ok_or(DevDeployError::RestartScriptMissing)?;
    if !path.is_file() {
        return Err(DevDeployError::RestartScriptMissing);
    }
    Ok(path)
}

/// Prefer the selected worktree's restart script so a Test in Dev deploy can
/// carry script changes; fall back to the process AJAX_WEB_RESTART_SCRIPT.
pub fn resolve_restart_script(worktree: &Path) -> Result<PathBuf, DevDeployError> {
    let candidate = worktree.join("scripts/dev-web-restart.sh");
    if candidate.is_file() {
        return Ok(candidate);
    }
    restart_script_from_env()
}

/// Launch the existing restart script for the shared dev slot only.
pub fn spawn_test_in_dev(script: &Path, worktree: &Path) -> Result<(), DevDeployError> {
    Command::new(script)
        .arg("--worktree")
        .arg(worktree)
        .arg("--profile")
        .arg(DEV_PROFILE)
        .arg("--port")
        .arg(DEV_PORT.to_string())
        .spawn()
        .map_err(|error| DevDeployError::SpawnFailed(error.to_string()))?;
    Ok(())
}

/// Build argv for tests and dry-run inspection. Always targets profile=dev.
pub fn test_in_dev_command_args(worktree: &Path) -> Vec<String> {
    vec![
        "--worktree".to_string(),
        worktree.display().to_string(),
        "--profile".to_string(),
        DEV_PROFILE.to_string(),
        "--port".to_string(),
        DEV_PORT.to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::{
        config::{Config, ManagedRepo},
        models::{GitStatus, Task, TaskId},
        registry::InMemoryRegistry,
    };
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    fn ajax_task(worktree: impl Into<PathBuf>) -> Task {
        Task::new(
            TaskId::new("ajax-cli/test-in-dev"),
            "ajax-cli",
            "test-in-dev",
            "Test in Dev",
            "feat/test-in-dev",
            "main",
            worktree,
            "ajax-ajax-cli-test-in-dev",
            "task",
            ajax_core::models::AgentClient::Codex,
        )
    }

    fn other_repo_task() -> Task {
        Task::new(
            TaskId::new("autosnooze/other"),
            "autosnooze",
            "other",
            "Other",
            "feat/other",
            "main",
            "/tmp/not-ajax",
            "ajax-autosnooze-other",
            "task",
            ajax_core::models::AgentClient::Codex,
        )
    }

    fn context_with(tasks: Vec<Task>, repo_path: PathBuf) -> CommandContext<InMemoryRegistry> {
        let mut registry = InMemoryRegistry::default();
        for task in tasks {
            registry.create_task(task).unwrap();
        }
        let config = Config {
            repos: vec![
                ManagedRepo::new("ajax-cli", repo_path, "main"),
                ManagedRepo::new("autosnooze", "/tmp/autosnooze", "main"),
            ],
            ..Config::default()
        };
        CommandContext::new(config, registry)
    }

    #[test]
    fn open_url_is_fixed_ajaxdev_endpoint() {
        assert_eq!(DEV_OPEN_URL, "https://ajaxdev.mossyhome.net:8788");
    }

    #[test]
    fn phase_labels_match_required_ux() {
        assert_eq!(DevDeployPhase::ReadyToDeploy.label(), "Ready to deploy");
        assert_eq!(DevDeployPhase::Building.label(), "Building");
        assert_eq!(DevDeployPhase::Restarting.label(), "Restarting");
        assert_eq!(DevDeployPhase::DevReady.label(), "Dev ready");
        assert_eq!(DevDeployPhase::Failed.label(), "Failed");
    }

    #[test]
    fn slot_rejects_concurrent_begin() {
        let mut slot = DevDeploySlot::default();
        let source = DevDeploySource {
            task_handle: "ajax-cli/one".into(),
            title: "One".into(),
            branch: "feat/one".into(),
            worktree_path: PathBuf::from("/tmp/one"),
            commit_sha: "abc".into(),
            dirty: true,
        };
        slot.begin(&source).unwrap();
        assert_eq!(slot.status().phase, DevDeployPhase::Building);
        assert!(matches!(slot.begin(&source), Err(DevDeployError::Busy)));
    }

    #[test]
    fn slot_state_transitions_and_failure_preserve_prior_occupant_fields() {
        let mut slot = DevDeploySlot::default();
        let source = DevDeploySource {
            task_handle: "ajax-cli/one".into(),
            title: "One".into(),
            branch: "feat/one".into(),
            worktree_path: PathBuf::from("/tmp/one"),
            commit_sha: "deadbeef".into(),
            dirty: true,
        };
        slot.begin(&source).unwrap();
        slot.set_restarting();
        assert_eq!(slot.status().phase, DevDeployPhase::Restarting);
        slot.set_ready(&source);
        let ready = slot.status();
        assert_eq!(ready.phase, DevDeployPhase::DevReady);
        assert_eq!(ready.occupant.as_ref().unwrap().commit_sha, "deadbeef");
        assert!(ready.occupant.as_ref().unwrap().dirty);
        assert!(ready.occupant.as_ref().unwrap().deployed_at_unix_secs > 0);

        slot.set_failed("boom");
        let failed = slot.status();
        assert_eq!(failed.phase, DevDeployPhase::Failed);
        assert_eq!(failed.error.as_deref(), Some("boom"));
        assert_eq!(
            failed.occupant.as_ref().unwrap().task_handle,
            "ajax-cli/one"
        );
    }

    #[test]
    fn global_lock_allows_only_one_active_deployment() {
        let slot = Arc::new(Mutex::new(DevDeploySlot::default()));
        let source = DevDeploySource {
            task_handle: "ajax-cli/one".into(),
            title: "One".into(),
            branch: "feat/one".into(),
            worktree_path: PathBuf::from("/tmp/one"),
            commit_sha: "abc".into(),
            dirty: false,
        };

        {
            let mut guard = lock_slot(&slot);
            guard.begin(&source).unwrap();
        }

        let slot2 = Arc::clone(&slot);
        let source2 = source.clone();
        let handle = thread::spawn(move || {
            let mut guard = lock_slot(&slot2);
            guard.begin(&source2)
        });
        let result = handle.join().unwrap();
        assert!(matches!(result, Err(DevDeployError::Busy)));
    }

    #[test]
    fn resolve_rejects_non_ajax_repo_tasks() {
        let context = context_with(vec![other_repo_task()], PathBuf::from("/tmp/ajax-cli"));
        let err = resolve_ajax_dev_deploy_source(&context, "autosnooze/other").unwrap_err();
        assert!(matches!(err, DevDeployError::NotAjaxRepo { .. }));
    }

    #[test]
    fn resolve_rejects_missing_task() {
        let context = context_with(vec![], PathBuf::from("/tmp/ajax-cli"));
        let err = resolve_ajax_dev_deploy_source(&context, "ajax-cli/missing").unwrap_err();
        assert!(matches!(err, DevDeployError::TaskNotFound(_)));
    }

    #[test]
    fn resolve_rejects_arbitrary_nonexistent_worktree_path() {
        let mut task = ajax_task("/tmp/definitely-not-an-ajax-worktree-path");
        task.git_status = Some(GitStatus {
            worktree_exists: false,
            branch_exists: true,
            current_branch: Some("feat/test-in-dev".into()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abc123 message".into()),
        });
        let context = context_with(vec![task], PathBuf::from("/tmp/ajax-cli"));
        let err = resolve_ajax_dev_deploy_source(&context, "ajax-cli/test-in-dev").unwrap_err();
        assert!(matches!(err, DevDeployError::WorktreeMissing(_)));
    }

    #[test]
    fn resolve_accepts_real_ajax_cli_worktree_when_present() {
        let repo = PathBuf::from("/Users/matt/Desktop/Projects/ajax-cli");
        let worktree =
            PathBuf::from("/Users/matt/Desktop/Projects/ajax-cli__worktrees/feat-test-in-dev");
        if !repo.exists() || !worktree.exists() {
            return;
        }
        let mut task = ajax_task(worktree.clone());
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("feat/test-in-dev".into()),
            dirty: true,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: Some("abcdef012345 message".into()),
        });
        let context = context_with(vec![task], repo);
        let source = resolve_ajax_dev_deploy_source(&context, "ajax-cli/test-in-dev").unwrap();
        assert_eq!(source.worktree_path, worktree);
        assert_eq!(source.commit_sha, "abcdef012345");
        assert!(source.dirty);
    }

    #[test]
    fn resolve_rejects_path_outside_ajax_worktrees_even_if_directory_exists() {
        let scratch =
            std::env::temp_dir().join(format!("ajax-dev-deploy-reject-{}", std::process::id()));
        let _ = fs::remove_dir_all(&scratch);
        fs::create_dir_all(&scratch).unwrap();
        let task = ajax_task(scratch.clone());
        let context = context_with(
            vec![task],
            PathBuf::from("/Users/matt/Desktop/Projects/ajax-cli"),
        );
        let err = resolve_ajax_dev_deploy_source(&context, "ajax-cli/test-in-dev").unwrap_err();
        assert!(
            matches!(err, DevDeployError::WorktreeNotManaged { .. }),
            "got {err:?}"
        );
        let _ = fs::remove_dir_all(&scratch);
    }

    #[test]
    fn test_in_dev_args_never_target_stable() {
        let args = test_in_dev_command_args(Path::new("/tmp/wt"));
        assert!(args.contains(&"--profile".to_string()));
        assert!(args.contains(&"dev".to_string()));
        assert!(args.contains(&"8788".to_string()));
        assert!(!args.iter().any(|arg| arg == "stable"));
        assert!(!args.iter().any(|arg| arg == "8787"));
    }

    #[test]
    fn short_sha_parsing_trims_commit_subject() {
        assert_eq!(
            short_sha_from_last_commit("abc1234 Fix login").as_deref(),
            Some("abc1234")
        );
    }
}
