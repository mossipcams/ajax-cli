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
fn dev_deploy_status_has_no_open_url_field() {
    let status = DevDeploySlot::default().status();
    let value = serde_json::to_value(&status).expect("status serializes");
    assert!(value.get("open_url").is_none());
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
