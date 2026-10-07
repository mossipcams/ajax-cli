use super::lookup::{find_task, task_repo_path, update_task_lifecycle};
use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{CommandRunner, CommandSpec, GitAdapter, TmuxAdapter},
    lifecycle::force_mark_removed,
    models::{LifecycleStatus, SafetyClassification, SideFlag, Task, TaskWindowStatus, TmuxStatus},
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    policy::cleanup_safety,
    registry::{Registry, RegistryError, RegistryEventKind},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub fn mark_task_cleanup_step_completed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    command: &CommandSpec,
) -> Result<bool, CommandError> {
    let task = find_task(context, qualified_handle)?.clone();

    if command.program == "tmux"
        && command
            .args
            .first()
            .is_some_and(|arg| arg == "kill-session")
        && command.args.iter().any(|arg| arg == &task.tmux_session)
    {
        context
            .registry
            .update_tmux_status(
                &task.id,
                Some(TmuxStatus {
                    exists: false,
                    session_name: task.tmux_session.clone(),
                }),
            )
            .map_err(CommandError::Registry)?;
        context
            .registry
            .update_task_window_status(
                &task.id,
                Some(TaskWindowStatus::missing(
                    task.task_window.clone(),
                    task.worktree_path.clone(),
                )),
            )
            .map_err(CommandError::Registry)?;
        return Ok(true);
    }

    if is_fast_worktree_remove_command(command)
        && command
            .args
            .get(4)
            .is_some_and(|arg| arg == &task.worktree_path.display().to_string())
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.worktree_exists = false;
            git_status.dirty = false;
            git_status.untracked_files = 0;
            git_status.conflicted = false;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::WorktreeMissing);
            task.remove_side_flag(SideFlag::Dirty);
            task.remove_side_flag(SideFlag::Conflicted);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "worktree")
        && command.args.iter().any(|arg| arg == "remove")
        && command
            .args
            .iter()
            .any(|arg| arg == &task.worktree_path.display().to_string())
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.worktree_exists = false;
            git_status.dirty = false;
            git_status.untracked_files = 0;
            git_status.conflicted = false;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::WorktreeMissing);
            task.remove_side_flag(SideFlag::Dirty);
            task.remove_side_flag(SideFlag::Conflicted);
        }
        return Ok(true);
    }

    if is_delete_branch_substrate_command(command)
        && command.args.get(4).is_some_and(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "push")
        && command.args.iter().any(|arg| arg == "--delete")
        && command.args.iter().any(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    if command.program == "git"
        && command.args.iter().any(|arg| arg == "branch")
        && (command.args.iter().any(|arg| arg == "-d")
            || command.args.iter().any(|arg| arg == "-D"))
        && command.args.iter().any(|arg| arg == &task.branch)
    {
        if let Some(mut git_status) = task.git_status.clone() {
            git_status.branch_exists = false;
            git_status.current_branch = None;
            git_status.ahead = 0;
            git_status.behind = 0;
            git_status.unpushed_commits = 0;
            context
                .registry
                .update_git_status(&task.id, git_status)
                .map_err(CommandError::Registry)?;
        } else if let Some(task) = context.registry.get_task_mut(&task.id) {
            task.add_side_flag(SideFlag::BranchMissing);
            task.remove_side_flag(SideFlag::Unpushed);
        }
        return Ok(true);
    }

    Ok(false)
}

pub fn is_fast_worktree_remove_command(command: &CommandSpec) -> bool {
    command.program == "sh"
        && command.args.first().is_some_and(|arg| arg == "-c")
        && command
            .args
            .get(2)
            .is_some_and(|arg| arg == "ajax-fast-worktree-remove")
}

pub fn is_delete_branch_substrate_command(command: &CommandSpec) -> bool {
    command.program == "sh"
        && command.args.first().is_some_and(|arg| arg == "-c")
        && command
            .args
            .get(2)
            .is_some_and(|arg| arg == "ajax-delete-branch")
}

pub fn clean_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("clean task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Clean)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    let safety = cleanup_safety(task);

    match safety.classification {
        SafetyClassification::Safe => {
            plan.commands = native_cleanup_commands(context, task)?;
        }
        SafetyClassification::NeedsConfirmation | SafetyClassification::Dangerous => {
            plan.requires_confirmation = true;
            plan.commands = native_cleanup_commands(context, task)?;
        }
        SafetyClassification::Blocked => {
            plan.blocked_reasons = safety.reasons;
        }
    }

    Ok(plan)
}

pub fn remove_task_plan<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<CommandPlan, CommandError> {
    let task = find_task(context, qualified_handle)?;
    let mut plan = CommandPlan::new(format!("remove task: {qualified_handle}"));
    if let OperationEligibility::Blocked(reasons) =
        task_operation_eligibility(task, TaskOperation::Remove)
    {
        plan.blocked_reasons = reasons;
        return Ok(plan);
    }

    plan.requires_confirmation = true;
    plan.commands = native_remove_commands(context, task)?;

    Ok(plan)
}

pub fn ensure_cleanup_git_status<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    runner: &mut impl CommandRunner,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let merged = task.lifecycle_status == LifecycleStatus::Merged
        || task.lifecycle_status == LifecycleStatus::Cleanable
        || task.git_status.as_ref().is_some_and(|status| status.merged);
    super::refresh_git_evidence(context, qualified_handle, runner, merged)
}

pub fn mark_task_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Removed)
}

pub fn mark_task_removing<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    update_task_lifecycle(context, qualified_handle, LifecycleStatus::Removing)
}

pub fn mark_task_force_removed<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    let Some(task) = context.registry.get_task_mut(&task_id) else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    force_mark_removed(task).map_err(|error| {
        CommandError::Registry(RegistryError::InvalidLifecycleTransition(error))
    })?;
    task.last_activity_at = SystemTime::now();
    task.remove_side_flag(SideFlag::Stale);
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::LifecycleChanged,
            "lifecycle changed to Removed",
        )
        .map_err(CommandError::Registry)
}

pub fn sweep_cleanup_plan<R: Registry>(context: &CommandContext<R>) -> CommandPlan {
    let mut plan = CommandPlan::new("sweep cleanup");

    plan.commands = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| super::projection::is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .filter_map(|task| native_cleanup_commands(context, task).ok())
        .flatten()
        .collect();
    plan.commands.extend(sweep_trash_commands(context));

    plan
}

pub fn sweep_cleanup_candidates<R: Registry>(context: &CommandContext<R>) -> Vec<String> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| super::projection::is_visible_task(task))
        .filter(|task| cleanup_safety(task).classification == SafetyClassification::Safe)
        .map(Task::qualified_handle)
        .collect()
}

pub fn sweep_trash_commands<R: Registry>(context: &CommandContext<R>) -> Vec<CommandSpec> {
    worktree_roots(context)
        .into_iter()
        .map(|worktree_root| sweep_trash_command(&worktree_root))
        .collect()
}

fn native_cleanup_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, false)
}

fn native_remove_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
) -> Result<Vec<CommandSpec>, CommandError> {
    native_teardown_commands(context, task, true)
}

fn worktree_roots<R: Registry>(context: &CommandContext<R>) -> Vec<PathBuf> {
    context
        .registry
        .list_tasks()
        .into_iter()
        .filter_map(|task| task.worktree_path.parent().map(Path::to_path_buf))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sweep_trash_command(worktree_root: &Path) -> CommandSpec {
    let trash_dir = worktree_root.join(".ajax-trash").display().to_string();
    CommandSpec::new(
        "sh",
        [
            "-c",
            "if [ -d \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi",
            "ajax-trash-sweep",
            &trash_dir,
        ],
    )
}
fn native_teardown_commands<R: Registry>(
    context: &CommandContext<R>,
    task: &Task,
    force: bool,
) -> Result<Vec<CommandSpec>, CommandError> {
    let repo_path = task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let mut commands = Vec::new();
    let task_only = context.task_only_drop_verification(task)?;
    let preserve_checkout = task_only.is_some();
    commands.extend(task_only);

    if !preserve_checkout
        && task
            .git_status
            .as_ref()
            .is_none_or(|status| status.worktree_exists)
    {
        context.ensure_task_worktree_removable(task)?;
        let worktree_path = task.worktree_path.display().to_string();
        let needs_force = force
            || task.git_status.as_ref().is_some_and(|status| {
                status.dirty
                    || status.untracked_files > 0
                    || status.conflicted
                    || task.has_side_flag(SideFlag::Dirty)
                    || task.has_side_flag(SideFlag::Conflicted)
            });
        let command = if needs_force {
            git.force_remove_worktree(&repo_path, &worktree_path)
        } else {
            git.remove_worktree(&repo_path, &worktree_path)
        };
        commands.push(command);
    }
    if !preserve_checkout
        && task
            .git_status
            .as_ref()
            .is_none_or(|status| status.branch_exists)
    {
        let needs_force = force
            || task
                .git_status
                .as_ref()
                .is_some_and(|status| !status.merged);
        let command = git.delete_branch_substrate(&repo_path, &task.branch, needs_force);
        commands.push(command);
    }
    if task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists)
    {
        commands.push(tmux.kill_session(&task.tmux_session));
    }

    Ok(commands)
}

impl<R: Registry> CommandContext<R> {
    /// Issue #1216: the task's own main checkout (or a recorded path that is an
    /// ancestor of it) has no linked git worktree to remove. Dropping such a
    /// task removes only registry state and the tmux session, and
    /// `task_only_drop_verification` must prove first that the task's work
    /// landed on origin.
    pub(crate) fn task_worktree_is_preserved_checkout(&self, task: &Task) -> bool {
        let Some(repo_path_str) = task_repo_path(self, task) else {
            return false;
        };
        let repo_path = Path::new(&repo_path_str);
        if !repo_path.join(".git").is_dir() {
            return false;
        }

        let canonical_repo = match std::fs::canonicalize(repo_path) {
            Ok(path) => path,
            Err(_) => return false,
        };
        let recorded = Path::new(&task.worktree_path);
        let normalized_recorded =
            std::fs::canonicalize(recorded).unwrap_or_else(|_| recorded.to_path_buf());

        canonical_repo == normalized_recorded || canonical_repo.starts_with(&normalized_recorded)
    }

    /// #1216: a task attached to the main checkout owns no disposable Git
    /// resources. Preserve its checkout and branches, including checked-out
    /// main; return the verification command that proves the task's commits
    /// landed on origin's default branch before the drop may proceed.
    pub(crate) fn task_only_drop_verification(
        &self,
        task: &Task,
    ) -> Result<Option<CommandSpec>, CommandError> {
        let repo_path = task_repo_path(self, task)
            .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
        if !self.task_worktree_is_preserved_checkout(task) {
            return Ok(None);
        }

        // Query origin itself: local main and cached origin/HEAD are insufficient.
        // Fetch only the advertised commit and compare exact commit ancestry.
        // Positional arguments keep repository paths and branch names out of shell code.
        let script = r#"
fail() {
    printf 'Cannot verify that branch %s has landed on origin default branch. Push the task branch, merge its commits into origin main (or the origin default branch), then retry ajax drop. Check origin connectivity if already merged. No task resources were removed.\n' "$2" >&2
    exit 1
}
remote=$(git -C "$1" ls-remote --symref origin HEAD) || fail "$@"
default=$(printf '%s\n' "$remote" | awk '$1 == "ref:" && $3 == "HEAD" { print $2 }')
tip=$(printf '%s\n' "$remote" | awk '$2 == "HEAD" && $1 != "ref:" { print $1 }')
case "$default" in refs/heads/*) ;; *) fail "$@" ;; esac
[ -n "$tip" ] || fail "$@"
git -C "$1" fetch --no-tags origin "$tip" || fail "$@"
git -C "$1" merge-base --is-ancestor "refs/heads/$2" "$tip" || fail "$@"
"#;
        Ok(Some(
            CommandSpec::new(
                "sh",
                [
                    "-c",
                    script,
                    "ajax-verify-task-only-drop",
                    &repo_path,
                    &task.branch,
                ],
            )
            .with_timeout(std::time::Duration::from_secs(60)),
        ))
    }

    /// Refuse teardown operations that would move a path which is not a real
    /// linked worktree (the repo root, an ancestor of it, or a full repository
    /// with a `.git` directory) into the trash directory.
    pub(crate) fn ensure_task_worktree_removable(&self, task: &Task) -> Result<(), CommandError> {
        let repo_path = PathBuf::from(
            task_repo_path(self, task)
                .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?,
        );
        let worktree_path = task.worktree_path.clone();

        if !worktree_path.exists() {
            return Ok(());
        }

        let canonical_worktree =
            std::fs::canonicalize(&worktree_path).unwrap_or_else(|_| worktree_path.clone());
        let canonical_repo =
            std::fs::canonicalize(&repo_path).unwrap_or_else(|_| repo_path.clone());

        if canonical_worktree == canonical_repo || canonical_repo.starts_with(&canonical_worktree) {
            // Issue #1216: the task's own main checkout is not a linked
            // worktree and has nothing to move. Dropping it proceeds as a
            // task-only drop gated by origin landing verification instead of
            // being permanently blocked.
            if self.task_worktree_is_preserved_checkout(task) {
                return Ok(());
            }
            return Err(CommandError::PlanBlocked(vec![format!(
                "worktree path {} is the repo root or an ancestor of it; refusing to move it to trash",
                worktree_path.display()
            )]));
        }

        if worktree_path.join(".git").is_dir() {
            return Err(CommandError::PlanBlocked(vec![format!(
                "worktree path {} contains a .git directory (full repository, not a linked worktree); refusing to move it to trash",
                worktree_path.display()
            )]));
        }

        Ok(())
    }
}

mod drop_observation;

pub use drop_observation::{
    drop_op_label, format_drop_remaining_resources_detail, format_drop_teardown_incomplete_message,
    mark_drop_agent_stopped, mark_task_teardown_incomplete, observe_drop_resources,
    observe_drop_resources_with_cache, plan_drop_from_observation,
    plan_drop_from_observation_for_task, DropObservation, DropOp, RepoDropObservationCache,
    ResourceState, DROP_TEARDOWN_ORDER,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod worktree_guard_tests {
    use super::*;
    use crate::config::{Config, ManagedRepo};
    use crate::models::{AgentClient, TaskId};
    use crate::registry::InMemoryRegistry;
    use std::time::UNIX_EPOCH;

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ajax-teardown-guard-{}-{}-{nanos}",
            std::process::id(),
            label
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn context_with_worktree(
        repo_path: &Path,
        worktree_path: &Path,
    ) -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new(
                    "web",
                    repo_path.display().to_string(),
                    "main",
                )],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            worktree_path.display().to_string(),
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        );
        context.registry.create_task(task).unwrap();
        context
    }

    fn first_task(context: &CommandContext<InMemoryRegistry>) -> Task {
        context
            .registry
            .list_tasks()
            .into_iter()
            .next()
            .expect("task exists")
            .clone()
    }

    #[test]
    fn repo_root_path_is_blocked() {
        let root = make_temp_dir("repo-root");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let context = context_with_worktree(&repo, &repo);
        let task = first_task(&context);

        let error = context.ensure_task_worktree_removable(&task).unwrap_err();
        assert!(
            matches!(error, CommandError::PlanBlocked(_)),
            "got {error:?}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ancestor_of_repo_is_blocked() {
        let root = make_temp_dir("ancestor");
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let context = context_with_worktree(&repo, &root);
        let task = first_task(&context);

        let error = context.ensure_task_worktree_removable(&task).unwrap_err();
        assert!(
            matches!(error, CommandError::PlanBlocked(_)),
            "got {error:?}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn dir_with_git_directory_is_blocked() {
        let root = make_temp_dir("git-dir");
        let repo = root.join("repo");
        let worktree = root.join("worktree");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(worktree.join(".git")).unwrap();
        let context = context_with_worktree(&repo, &worktree);
        let task = first_task(&context);

        let error = context.ensure_task_worktree_removable(&task).unwrap_err();
        assert!(
            matches!(error, CommandError::PlanBlocked(_)),
            "got {error:?}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn linked_worktree_with_git_file_is_allowed() {
        let root = make_temp_dir("git-file");
        let repo = root.join("repo");
        let worktree = root.join("worktree");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::write(worktree.join(".git"), "gitdir: /repo/.git/worktrees/wt\n").unwrap();
        let context = context_with_worktree(&repo, &worktree);
        let task = first_task(&context);

        context.ensure_task_worktree_removable(&task).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_worktree_path_is_allowed() {
        let root = make_temp_dir("missing");
        let repo = root.join("repo");
        let worktree = root.join("does-not-exist");
        std::fs::create_dir_all(&repo).unwrap();
        let context = context_with_worktree(&repo, &worktree);
        let task = first_task(&context);

        context.ensure_task_worktree_removable(&task).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    // #1216 regression: a task whose worktree path is the main checkout
    // (a real git directory, not just any path) must be droppable as a
    // task-only drop — the old guard moved the user's entire repo to trash.
    #[test]
    fn repo_root_checkout_with_git_dir_is_allowed() {
        let root = make_temp_dir("preserved-root");
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let context = context_with_worktree(&repo, &repo);
        let task = first_task(&context);

        context.ensure_task_worktree_removable(&task).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }

    // #1216 regression: an ancestor path that contains the repo's main
    // checkout (e.g. /Users/matt/Desktop/Projects containing SaySo) is a
    // preserved checkout, not a disposable worktree.
    #[test]
    fn ancestor_of_repo_with_git_dir_is_allowed() {
        let root = make_temp_dir("preserved-ancestor");
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let context = context_with_worktree(&repo, &root);
        let task = first_task(&context);

        context.ensure_task_worktree_removable(&task).unwrap();

        std::fs::remove_dir_all(&root).unwrap();
    }
}
