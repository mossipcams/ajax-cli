mod check;
mod diff;
mod doctor;
mod lookup;
mod merge;
mod new_task;
mod open;
mod projection;
mod teardown;
mod trunk;

pub use crate::adapters::DoctorEnvironment;
pub use crate::use_cases::{CommandContext, CommandError, CommandPlan, OpenMode};
pub use check::{
    check_task_plan, mark_task_check_failed, mark_task_check_started, mark_task_check_succeeded,
};
pub use diff::diff_task_plan;
pub use doctor::{doctor, doctor_with_environment};
pub use merge::{mark_task_merge_failed, mark_task_merged, merge_task_plan};
pub use new_task::{
    is_agent_send_keys_command, is_git_worktree_add_command, is_new_task_husky_hook_command,
    is_worktrunk_new_session_command, mark_new_task_provisioning_failed,
    mark_new_task_provisioning_step_completed, mark_new_task_step_completed, new_task_plan,
    record_new_task, start_provisioning_step_for_command, task_from_new_request, NewTaskRequest,
    StartProvisioningStep,
};
pub use open::{mark_task_opened, open_task_plan};
pub use teardown::{
    clean_task_plan, drop_op_label, ensure_cleanup_git_status,
    format_drop_remaining_resources_detail, format_drop_teardown_incomplete_message,
    mark_drop_agent_stopped, mark_task_cleanup_step_completed, mark_task_force_removed,
    mark_task_removed, mark_task_removing, mark_task_teardown_incomplete, observe_drop_resources,
    observe_drop_resources_with_cache, plan_drop_from_observation,
    plan_drop_from_observation_for_task, remove_task_plan, sweep_cleanup_candidates,
    sweep_cleanup_plan, DropObservation, DropOp, RepoDropObservationCache, ResourceState,
};
pub use trunk::{mark_task_trunk_repaired, trunk_task_plan, trunk_task_plan_with_open_mode};

use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter},
    analysis::git_evidence::interpret_git_status,
    config::Config,
    models::{Annotation, AnnotationKind, GitStatus, LifecycleStatus, SideFlag, Task},
    output::{
        AnnotationItem, CockpitProjection, CockpitResponse, CockpitView, InboxResponse,
        InspectResponse, NextResponse, RepoSummary, ReposResponse, TasksResponse,
    },
    recommended::{evidence_label, operator_action},
    registry::Registry,
};
use lookup::find_task;
use projection::{
    annotations_for_task, cockpit_projection as build_cockpit_projection, cockpit_summary,
    count_active_tasks, count_attention_items, count_lifecycle, is_cockpit_menu_task,
    is_visible_task, task_summary,
};
use std::{collections::BTreeSet, time::Duration, time::SystemTime};

const STALE_AFTER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

pub fn list_repos<R: Registry>(context: &CommandContext<R>) -> ReposResponse {
    let all_tasks = context.registry.list_tasks();
    list_repos_from_tasks(&context.config, all_tasks.as_slice())
}

fn list_repos_from_tasks(config: &Config, all_tasks: &[&Task]) -> ReposResponse {
    let repos = config
        .repos
        .iter()
        .map(|repo| {
            let repo_tasks: Vec<&Task> = all_tasks
                .iter()
                .copied()
                .filter(|task| task.repo == repo.name && is_visible_task(task))
                .collect();

            RepoSummary {
                name: repo.name.clone(),
                path: repo.path.display().to_string(),
                active_tasks: count_active_tasks(&repo_tasks),
                attention_items: count_attention_items(&repo_tasks),
                reviewable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Reviewable),
                cleanable_tasks: count_lifecycle(&repo_tasks, LifecycleStatus::Cleanable),
            }
        })
        .collect();

    ReposResponse { repos }
}

pub fn list_tasks<R: Registry>(context: &CommandContext<R>, repo: Option<&str>) -> TasksResponse {
    let all_tasks = context.registry.list_tasks();
    list_tasks_from_tasks(all_tasks.as_slice(), repo)
}

fn list_tasks_from_tasks(tasks: &[&Task], repo: Option<&str>) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| repo.is_none_or(|repo_name| task.repo == repo_name))
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

pub fn review_queue<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    crate::slices::review::review_queue(context)
}

fn review_queue_from_tasks(tasks: &[&Task]) -> TasksResponse {
    let tasks = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .filter(|task| {
            matches!(
                task.lifecycle_status,
                LifecycleStatus::Reviewable | LifecycleStatus::Mergeable
            )
        })
        .map(task_summary)
        .collect();

    TasksResponse { tasks }
}

pub fn inspect_task<R: Registry>(
    context: &CommandContext<R>,
    qualified_handle: &str,
) -> Result<InspectResponse, CommandError> {
    let Some(task) = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == qualified_handle)
    else {
        return Err(CommandError::TaskNotFound(qualified_handle.to_string()));
    };

    Ok(InspectResponse {
        task: task_summary(task),
        branch: task.branch.clone(),
        worktree_path: task.worktree_path.display().to_string(),
        tmux_session: task.tmux_session.clone(),
        flags: task
            .side_flags()
            .map(|flag| format!("{flag:?}"))
            .collect::<Vec<_>>(),
    })
}

pub fn inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    inbox_from_tasks(tasks.as_slice())
}

pub fn cockpit_inbox<R: Registry>(context: &CommandContext<R>) -> InboxResponse {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    cockpit_inbox_from_tasks(tasks.as_slice())
}

fn inbox_from_tasks(tasks: &[&Task]) -> InboxResponse {
    let visible = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    InboxResponse {
        items: annotation_items_matching(visible.as_slice(), |_| true),
    }
}

fn cockpit_inbox_from_tasks(tasks: &[&Task]) -> InboxResponse {
    let visible = tasks
        .iter()
        .copied()
        .filter(|task| is_visible_task(task))
        .collect::<Vec<_>>();
    InboxResponse {
        items: annotation_items_matching(visible.as_slice(), is_cockpit_inbox_annotation),
    }
}

pub fn next<R: Registry>(context: &CommandContext<R>) -> NextResponse {
    NextResponse {
        item: inbox(context).items.into_iter().next(),
    }
}

fn annotation_items_matching(
    tasks: &[&Task],
    include: impl Fn(&Annotation) -> bool,
) -> Vec<AnnotationItem> {
    let mut items = tasks
        .iter()
        .copied()
        .filter_map(|task| {
            annotations_for_task(task)
                .into_iter()
                .filter(|annotation| include(annotation))
                .min_by_key(|annotation| annotation.severity)
                .map(|annotation| annotation_item(task, annotation))
        })
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.severity
            .cmp(&right.severity)
            .then_with(|| left.task_handle.cmp(&right.task_handle))
            .then_with(|| left.reason.cmp(&right.reason))
    });
    items
}

fn is_cockpit_inbox_annotation(annotation: &Annotation) -> bool {
    matches!(
        annotation.kind,
        AnnotationKind::NeedsMe | AnnotationKind::Broken
    )
}

fn annotation_item(task: &Task, annotation: Annotation) -> AnnotationItem {
    AnnotationItem {
        task_id: task.id.clone(),
        task_handle: task.qualified_handle(),
        reason: evidence_label(&annotation.evidence).to_string(),
        severity: annotation.severity,
        action: operator_action(task).action,
    }
}

pub fn status<R: Registry>(context: &CommandContext<R>) -> TasksResponse {
    list_tasks(context, None)
}

pub fn cockpit<R: Registry>(context: &CommandContext<R>) -> CockpitResponse {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let tasks = list_tasks_from_tasks(all_tasks.as_slice(), None);
    let review = review_queue_from_tasks(all_tasks.as_slice());
    let inbox = inbox_from_tasks(all_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks, &review, &inbox);
    let next = NextResponse {
        item: inbox.items.first().cloned(),
    };

    CockpitResponse {
        summary,
        repos,
        tasks,
        review,
        inbox,
        next,
    }
}

pub fn cockpit_projection<R: Registry>(context: &CommandContext<R>) -> CockpitProjection {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let inbox = inbox_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review, &inbox);
    build_cockpit_projection(all_tasks.as_slice(), summary)
}

pub fn cockpit_view<R: Registry>(context: &CommandContext<R>) -> CockpitView {
    let all_tasks = context.registry.list_tasks();
    let repos = list_repos_from_tasks(&context.config, all_tasks.as_slice());
    let cockpit_tasks = all_tasks
        .iter()
        .copied()
        .filter(|task| is_cockpit_menu_task(task))
        .collect::<Vec<_>>();
    let tasks_list = list_tasks_from_tasks(cockpit_tasks.as_slice(), None);
    let review = review_queue_from_tasks(cockpit_tasks.as_slice());
    let inbox = cockpit_inbox_from_tasks(cockpit_tasks.as_slice());
    let summary = cockpit_summary(&repos, &tasks_list, &review, &inbox);
    let projection = build_cockpit_projection(all_tasks.as_slice(), summary);

    CockpitView {
        repos,
        cards: projection.cards,
        inbox,
    }
}

pub fn mark_stale_tasks<R: Registry>(context: &mut CommandContext<R>, now: SystemTime) -> u32 {
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| is_visible_task(task))
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let mut tasks_changed = 0;

    for task_id in &task_ids {
        if let Some(task) = context.registry.get_task_mut(task_id) {
            let Ok(inactive_for) = now.duration_since(task.last_activity_at) else {
                continue;
            };

            if inactive_for >= STALE_AFTER && !task.has_side_flag(SideFlag::Stale) {
                task.add_side_flag(SideFlag::Stale);
                tasks_changed += 1;
            }
        }
    }

    tasks_changed
}

pub fn refresh_git_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    runner: &mut impl CommandRunner,
    merged: bool,
) -> Result<(), CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let git = GitAdapter::new("git");
    let output = runner
        .run(&git.status(&task.worktree_path.display().to_string()))
        .map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: "git".to_string(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: None,
        }));
    }

    let Some(git_status) = interpret_git_status(&output.stdout, task.git_status.as_ref(), merged)
    else {
        return Ok(());
    };
    context
        .registry
        .update_git_status(&task.id, git_status)
        .map_err(CommandError::Registry)?;

    Ok(())
}

pub fn refresh_git_substrate_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    runner: &mut impl CommandRunner,
) -> Result<bool, CommandError> {
    let tasks = context
        .registry
        .list_tasks()
        .into_iter()
        .filter(|task| task.lifecycle_status != LifecycleStatus::Removed)
        .filter(|task| {
            task.git_status.is_some()
                || task.has_side_flag(crate::models::SideFlag::WorktreeMissing)
                || task.has_side_flag(crate::models::SideFlag::BranchMissing)
        })
        .cloned()
        .collect::<Vec<_>>();
    if tasks.is_empty() {
        return Ok(false);
    }

    let git = GitAdapter::new("git");
    let mut updates = Vec::new();

    for repo in &context.config.repos {
        let repo_tasks = tasks
            .iter()
            .filter(|task| task.repo == repo.name)
            .collect::<Vec<_>>();
        if repo_tasks.is_empty() {
            continue;
        }

        let repo_path = repo.path.display().to_string();
        let worktrees_output = run_successful_command(runner, &git.list_worktrees(&repo_path))?;
        if worktrees_output.trim().is_empty() {
            continue;
        }
        let branches_output = run_successful_command(runner, &git.list_branches(&repo_path))?;
        let worktrees = GitAdapter::parse_worktrees(&worktrees_output);
        let branches = GitAdapter::parse_branches(&branches_output)
            .into_iter()
            .collect::<BTreeSet<_>>();

        for task in repo_tasks {
            let expected_worktree = task.worktree_path.display().to_string();
            let observed_worktree = worktrees
                .iter()
                .find(|worktree| worktree.path == expected_worktree);
            let worktree_exists = observed_worktree.is_some();
            let branch_exists = branches.contains(&task.branch)
                || observed_worktree
                    .and_then(|worktree| worktree.branch.as_ref())
                    .is_some_and(|branch| branch == &task.branch);
            let current_branch = observed_worktree.and_then(|worktree| worktree.branch.clone());
            let git_status = substrate_git_status(
                task.git_status.as_ref(),
                worktree_exists,
                branch_exists,
                current_branch,
            );

            if task.git_status.as_ref() != Some(&git_status) {
                updates.push((task.id.clone(), git_status));
            }
        }
    }

    let changed = !updates.is_empty();
    for (task_id, git_status) in updates {
        context
            .registry
            .update_git_status(&task_id, git_status)
            .map_err(CommandError::Registry)?;
    }

    Ok(changed)
}

pub fn mark_task_git_substrate_missing<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<bool, CommandError> {
    let task = find_task(context, qualified_handle)?.clone();
    let git_status = substrate_git_status(task.git_status.as_ref(), false, false, None);
    if task.git_status.as_ref() == Some(&git_status) {
        return Ok(false);
    }

    context
        .registry
        .update_git_status(&task.id, git_status)
        .map_err(CommandError::Registry)?;

    Ok(true)
}

fn run_successful_command(
    runner: &mut impl CommandRunner,
    command: &CommandSpec,
) -> Result<String, CommandError> {
    let output = runner.run(command).map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: command.program.clone(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: command.cwd.clone(),
        }));
    }

    Ok(output.stdout)
}

fn substrate_git_status(
    previous: Option<&GitStatus>,
    worktree_exists: bool,
    branch_exists: bool,
    current_branch: Option<String>,
) -> GitStatus {
    let mut status = previous.cloned().unwrap_or(GitStatus {
        worktree_exists,
        branch_exists,
        current_branch: current_branch.clone(),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    status.worktree_exists = worktree_exists;
    status.branch_exists = branch_exists;
    status.current_branch = current_branch;

    if !worktree_exists {
        status.dirty = false;
        status.ahead = 0;
        status.behind = 0;
        status.untracked_files = 0;
        status.unpushed_commits = 0;
        status.conflicted = false;
        status.last_commit = None;
    }

    status
}

pub fn execute_plan(
    plan: &CommandPlan,
    confirmed: bool,
    runner: &mut impl CommandRunner,
) -> Result<Vec<CommandOutput>, CommandError> {
    if !plan.blocked_reasons.is_empty() {
        return Err(CommandError::PlanBlocked(plan.blocked_reasons.clone()));
    }

    if plan.requires_confirmation && !confirmed {
        return Err(CommandError::ConfirmationRequired);
    }

    let mut outputs = Vec::new();

    for command in &plan.commands {
        let output = runner.run(command).map_err(CommandError::CommandRun)?;
        if output.status_code != 0 {
            return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: command.program.clone(),
                status_code: output.status_code,
                stderr: output.stderr,
                cwd: command.cwd.clone(),
            }));
        }
        outputs.push(output);
    }

    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::{
        check_task_plan, clean_task_plan, cockpit, cockpit_inbox, diff_task_plan,
        doctor_with_environment, inbox, inspect_task, list_repos, list_tasks, mark_stale_tasks,
        merge_task_plan, new_task_plan, next, observe_drop_resources, open_task_plan,
        plan_drop_from_observation, plan_drop_from_observation_for_task,
        refresh_git_substrate_evidence, remove_task_plan, review_queue, status, sweep_cleanup_plan,
        task_from_new_request, trunk_task_plan, CommandContext, CommandError, DoctorEnvironment,
        DropObservation, DropOp, NewTaskRequest, OpenMode, ResourceState, StartProvisioningStep,
    };
    use crate::{
        adapters::{
            CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, GitAdapter,
            RecordingCommandRunner,
        },
        config::{Config, ManagedRepo, TestCommand},
        live::LiveStatusKind,
        models::{
            AgentClient, Annotation, AnnotationKind, Evidence, GitStatus, LifecycleStatus,
            LiveObservation, OperatorAction, RuntimeHealth, RuntimeObservationSource,
            RuntimeProjection, SideFlag, StepReceipt, Task, TaskId, TmuxStatus, WorktrunkStatus,
        },
        output::CockpitSummary,
        registry::{InMemoryRegistry, Registry, RegistryError, RegistryEvent, RegistryEventKind},
    };
    use proptest::prelude::*;
    use rstest::rstest;
    use std::cell::Cell;

    fn context_with_tasks() -> CommandContext<InMemoryRegistry> {
        let config = Config {
            repos: vec![
                ManagedRepo::new("web", "/Users/matt/projects/web", "main"),
                ManagedRepo::new("api", "/Users/matt/projects/api", "main"),
            ],
            ..Config::default()
        };
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
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
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.add_side_flag(SideFlag::NeedsInput);
        registry.create_task(task).unwrap();

        CommandContext::new(config, registry)
    }

    #[derive(Default)]
    struct CountingRegistry {
        inner: InMemoryRegistry,
        list_tasks_calls: Cell<u32>,
    }

    impl CountingRegistry {
        fn from_registry(inner: InMemoryRegistry) -> Self {
            Self {
                inner,
                list_tasks_calls: Cell::new(0),
            }
        }

        fn list_tasks_calls(&self) -> u32 {
            self.list_tasks_calls.get()
        }
    }

    impl Registry for CountingRegistry {
        fn create_task(&mut self, task: Task) -> Result<(), RegistryError> {
            self.inner.create_task(task)
        }

        fn delete_task(&mut self, task_id: &TaskId) -> Result<(), RegistryError> {
            self.inner.delete_task(task_id)
        }

        fn get_task(&self, task_id: &TaskId) -> Option<&Task> {
            self.inner.get_task(task_id)
        }

        fn get_task_mut(&mut self, task_id: &TaskId) -> Option<&mut Task> {
            self.inner.get_task_mut(task_id)
        }

        fn list_tasks(&self) -> Vec<&Task> {
            self.list_tasks_calls.set(self.list_tasks_calls.get() + 1);
            self.inner.list_tasks()
        }

        fn update_lifecycle(
            &mut self,
            task_id: &TaskId,
            status: LifecycleStatus,
        ) -> Result<(), RegistryError> {
            self.inner.update_lifecycle(task_id, status)
        }

        fn record_event(
            &mut self,
            task_id: TaskId,
            kind: RegistryEventKind,
            message: impl Into<String>,
        ) -> Result<(), RegistryError> {
            self.inner.record_event(task_id, kind, message)
        }

        fn update_git_status(
            &mut self,
            task_id: &TaskId,
            status: GitStatus,
        ) -> Result<(), RegistryError> {
            self.inner.update_git_status(task_id, status)
        }

        fn update_tmux_status(
            &mut self,
            task_id: &TaskId,
            status: Option<TmuxStatus>,
        ) -> Result<(), RegistryError> {
            self.inner.update_tmux_status(task_id, status)
        }

        fn update_worktrunk_status(
            &mut self,
            task_id: &TaskId,
            status: Option<WorktrunkStatus>,
        ) -> Result<(), RegistryError> {
            self.inner.update_worktrunk_status(task_id, status)
        }

        fn apply_live_observation(
            &mut self,
            task_id: &TaskId,
            observation: LiveObservation,
        ) -> Result<(), RegistryError> {
            self.inner.apply_live_observation(task_id, observation)
        }

        fn list_events(&self) -> Vec<&RegistryEvent> {
            self.inner.list_events()
        }

        fn events_for_task(&self, task_id: &TaskId) -> Vec<&RegistryEvent> {
            self.inner.events_for_task(task_id)
        }

        fn record_step_receipt(&mut self, receipt: StepReceipt) -> Result<(), RegistryError> {
            self.inner.record_step_receipt(receipt)
        }

        fn step_receipts_for_task(&self, task_id: &TaskId) -> Vec<&StepReceipt> {
            self.inner.step_receipts_for_task(task_id)
        }
    }

    fn counting_context_with_tasks() -> CommandContext<CountingRegistry> {
        let context = context_with_tasks();
        CommandContext::new(
            context.config,
            CountingRegistry::from_registry(context.registry),
        )
    }

    fn context_with_cleanable_task() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_tasks();
        let task_id = TaskId::new("task-1");
        let task = context.registry.get_task(&task_id).cloned().unwrap();
        let mut cleanable = task;
        cleanable.lifecycle_status = LifecycleStatus::Cleanable;
        cleanable.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        cleanable.tmux_status = Some(crate::models::TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        context.registry = InMemoryRegistry::default();
        context.registry.create_task(cleanable).unwrap();
        context
    }

    fn context_with_test_command() -> CommandContext<InMemoryRegistry> {
        let mut context = context_with_tasks();
        context.config.test_commands = vec![TestCommand::new("web", "cargo test")];
        context
    }

    #[derive(Default)]
    struct QueuedRunner {
        outputs: std::collections::VecDeque<CommandOutput>,
        commands: Vec<CommandSpec>,
    }

    impl QueuedRunner {
        fn new(outputs: Vec<CommandOutput>) -> Self {
            Self {
                outputs: outputs.into(),
                commands: Vec::new(),
            }
        }
    }

    impl CommandRunner for QueuedRunner {
        fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
            self.commands.push(command.clone());
            self.outputs
                .pop_front()
                .ok_or_else(|| CommandRunError::SpawnFailed("missing queued output".to_string()))
        }
    }

    fn output(status_code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            status_code,
            stdout: stdout.to_string(),
            stderr: String::new(),
        }
    }

    fn shell_words(command: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut current = String::new();
        let mut chars = command.chars().peekable();
        let mut in_single_quotes = false;
        let mut word_started = false;

        while let Some(character) = chars.next() {
            match character {
                '\'' => {
                    word_started = true;
                    in_single_quotes = !in_single_quotes;
                }
                '\\' if !in_single_quotes => {
                    word_started = true;
                    if let Some(escaped) = chars.next() {
                        current.push(escaped);
                    } else {
                        current.push(character);
                    }
                }
                ' ' if !in_single_quotes => {
                    if word_started {
                        words.push(std::mem::take(&mut current));
                        word_started = false;
                    }
                }
                _ => {
                    word_started = true;
                    current.push(character);
                }
            }
        }

        if word_started {
            words.push(current);
        }

        words
    }

    proptest! {
        #[test]
        fn native_new_task_agent_command_does_not_send_generated_title(
            title in "[^\\x00]{0,80}"
        ) {
            let context = CommandContext::new(
                Config {
                    repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                    ..Config::default()
                },
                InMemoryRegistry::default(),
            );
            let plan = new_task_plan(
                &context,
                NewTaskRequest {
                    repo: "web".to_string(),
                    title: title.clone(),
                    agent: "codex".to_string(),
                },
            )
            .unwrap();

            let worktree_command = plan
                .commands
                .iter()
                .find(|command| super::is_git_worktree_add_command(command))
                .expect("worktree add command");
            let send_keys = plan
                .commands
                .iter()
                .find(|command| super::is_agent_send_keys_command(command))
                .expect("agent send-keys command");
            let worktree_path = worktree_command.args[6].clone();

            prop_assert_eq!(send_keys.program.as_str(), "tmux");
            prop_assert_eq!(send_keys.args[0].as_str(), "send-keys");
            prop_assert_eq!(
                shell_words(&send_keys.args[3]),
                vec![
                    "codex".to_string(),
                    "--cd".to_string(),
                    worktree_path,
                ]
            );
        }

        #[test]
        fn native_cleanup_commands_reflect_generated_resource_and_risk_status(
            tmux_exists in any::<bool>(),
            dirty in any::<bool>(),
            conflicted in any::<bool>(),
            side_dirty in any::<bool>(),
            side_conflicted in any::<bool>(),
            untracked_files in 0u32..4,
            merged in any::<bool>()
        ) {
            let mut context = context_with_cleanable_task();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            let git_status = task.git_status.as_mut().unwrap();
            git_status.dirty = dirty;
            git_status.conflicted = conflicted;
            git_status.untracked_files = untracked_files;
            git_status.merged = merged;
            task.tmux_status = Some(TmuxStatus {
                exists: tmux_exists,
                session_name: task.tmux_session.clone(),
            });
            if side_dirty {
                task.add_side_flag(SideFlag::Dirty);
            }
            if side_conflicted {
                task.add_side_flag(SideFlag::Conflicted);
            }

            let plan = clean_task_plan(&context, "web/fix-login").unwrap();
            let expected_force_worktree =
                dirty || conflicted || side_dirty || side_conflicted || untracked_files > 0;
            let expected_worktree_args: Vec<String> = if expected_force_worktree {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "--force",
                    "/tmp/worktrees/web-fix-login",
                ]
            } else {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/web-fix-login",
                ]
            }
            .into_iter()
            .map(str::to_string)
            .collect();
            let expected_branch_args: Vec<String> = if merged {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login",
                ]
            } else {
                vec![
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-D",
                    "ajax/fix-login",
                ]
            }
            .into_iter()
            .map(str::to_string)
            .collect();
            let has_expected_worktree_command = plan
                .commands
                .iter()
                .any(|command| command.program == "git" && command.args == expected_worktree_args);
            let has_expected_branch_command = plan
                .commands
                .iter()
                .any(|command| command.program == "git" && command.args == expected_branch_args);

            prop_assert!(plan.blocked_reasons.is_empty());
            prop_assert_eq!(
                plan.commands
                    .iter()
                    .any(|command| command.args == vec!["kill-session", "-t", "ajax-web-fix-login"]),
                tmux_exists
            );
            prop_assert!(has_expected_worktree_command);
            prop_assert!(has_expected_branch_command);
        }

        #[test]
        fn trunk_plan_repairs_generated_tmux_and_worktrunk_states(
            worktree_exists in any::<bool>(),
            tmux_exists in any::<bool>(),
            worktrunk_exists in any::<bool>(),
            points_at_expected_path in any::<bool>()
        ) {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.git_status = Some(GitStatus {
                worktree_exists,
                branch_exists: true,
                current_branch: Some("ajax/fix-login".to_string()),
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: None,
            });
            task.tmux_status = Some(TmuxStatus {
                exists: tmux_exists,
                session_name: task.tmux_session.clone(),
            });
            task.worktrunk_status = Some(WorktrunkStatus {
                exists: worktrunk_exists,
                window_name: task.worktrunk_window.clone(),
                current_path: if points_at_expected_path {
                    task.worktree_path.clone()
                } else {
                    "/tmp/other-worktree".into()
                },
                points_at_expected_path,
            });

            let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

            if !worktree_exists {
                prop_assert!(plan.commands.is_empty());
                prop_assert_eq!(
                    plan.blocked_reasons,
                    vec!["task worktree is missing: /tmp/worktrees/web-fix-login"]
                );
                return Ok(());
            }

            prop_assert!(plan.blocked_reasons.is_empty());
            prop_assert_eq!(
                &plan.commands[plan.commands.len() - 2..],
                &[
                    CommandSpec::new(
                        "tmux",
                        ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                    ),
                    CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                        .with_mode(CommandMode::InheritStdio),
                ]
            );

            let repair_commands = &plan.commands[..plan.commands.len() - 2];
            if !tmux_exists {
                prop_assert_eq!(
                    repair_commands,
                    &[CommandSpec::new(
                        "tmux",
                        [
                            "new-session",
                            "-d",
                            "-s",
                            "ajax-web-fix-login",
                            "-n",
                            "worktrunk",
                            "-c",
                            "/tmp/worktrees/web-fix-login",
                        ],
                    )]
                );
            } else if worktrunk_exists && !points_at_expected_path {
                prop_assert_eq!(
                    repair_commands,
                    &[
                        CommandSpec::new(
                            "tmux",
                            ["kill-window", "-t", "ajax-web-fix-login:worktrunk"]
                        ),
                        CommandSpec::new(
                            "tmux",
                            [
                                "new-window",
                                "-t",
                                "ajax-web-fix-login",
                                "-n",
                                "worktrunk",
                                "-c",
                                "/tmp/worktrees/web-fix-login",
                            ],
                        ),
                    ]
                );
            } else if !worktrunk_exists {
                prop_assert_eq!(
                    repair_commands,
                    &[CommandSpec::new(
                        "tmux",
                        [
                            "new-window",
                            "-t",
                            "ajax-web-fix-login",
                            "-n",
                            "worktrunk",
                            "-c",
                            "/tmp/worktrees/web-fix-login",
                        ],
                    )]
                );
            } else {
                prop_assert!(repair_commands.is_empty());
            }
        }

        #[test]
        fn stale_task_marking_uses_seven_day_boundary(
            seconds_before_boundary in 0u64..(7 * 24 * 60 * 60)
        ) {
            let last_activity = std::time::SystemTime::UNIX_EPOCH;
            let stale_after = std::time::Duration::from_secs(7 * 24 * 60 * 60);
            let mut before_context = context_with_tasks();
            before_context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .last_activity_at = last_activity;
            let before_changed = mark_stale_tasks(
                &mut before_context,
                last_activity + std::time::Duration::from_secs(seconds_before_boundary),
            );

            prop_assert_eq!(before_changed, 0);
            prop_assert!(!before_context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .has_side_flag(SideFlag::Stale));

            let mut boundary_context = context_with_tasks();
            boundary_context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .last_activity_at = last_activity;
            let boundary_changed =
                mark_stale_tasks(&mut boundary_context, last_activity + stale_after);

            prop_assert_eq!(boundary_changed, 1);
            prop_assert!(boundary_context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .has_side_flag(SideFlag::Stale));
        }
    }

    #[test]
    fn repos_include_task_counts_by_repo() {
        let context = context_with_tasks();

        let response = list_repos(&context);

        assert_eq!(response.repos.len(), 2);
        assert_eq!(response.repos[0].name, "web");
        assert_eq!(response.repos[0].reviewable_tasks, 1);
        assert_eq!(response.repos[1].name, "api");
        assert_eq!(response.repos[1].active_tasks, 0);
    }

    #[test]
    fn missing_substrate_tasks_are_not_counted_as_active() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::WorktreeMissing);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].active_tasks, 0);
    }

    #[test]
    fn repo_attention_count_includes_broken_visible_tasks() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::Conflicted);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].attention_items, 1);
    }

    #[test]
    fn repo_attention_count_includes_visible_missing_substrate_tasks() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::TmuxMissing);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].attention_items, 1);
    }

    #[test]
    fn cockpit_summary_attention_includes_visible_missing_substrate_tasks() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;
        task.add_side_flag(SideFlag::TmuxMissing);

        let response = cockpit(&context);

        assert_eq!(response.summary.attention_items, 1);
    }

    #[test]
    fn repo_counts_include_active_and_attention_work() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::Stale);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].active_tasks, 1);
        assert_eq!(response.repos[0].attention_items, 0);
    }

    #[test]
    fn repo_attention_count_counts_tasks_once() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::Conflicted);

        let response = list_repos(&context);

        assert_eq!(response.repos[0].attention_items, 1);
    }

    #[test]
    fn tasks_can_be_filtered_by_repo() {
        let context = context_with_tasks();

        let all_tasks = list_tasks(&context, None);
        let web_tasks = list_tasks(&context, Some("web"));
        let api_tasks = list_tasks(&context, Some("api"));

        assert_eq!(all_tasks.tasks.len(), 1);
        assert_eq!(web_tasks.tasks.len(), 1);
        assert_eq!(api_tasks.tasks.len(), 0);
    }

    #[test]
    fn missing_substrate_tasks_remain_visible_in_task_lists() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let response = list_tasks(&context, None);

        assert_eq!(response.tasks.len(), 1);
        assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
    }

    #[test]
    fn list_repos_scans_registry_once() {
        let context = counting_context_with_tasks();

        let response = list_repos(&context);

        assert_eq!(response.repos.len(), 2);
        assert_eq!(context.registry.list_tasks_calls(), 1);
    }

    #[test]
    fn task_summary_marks_live_attention_without_side_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(crate::models::LiveObservation::new(
            LiveStatusKind::WaitingForApproval,
            "waiting for approval",
        ));

        let response = list_tasks(&context, None);

        assert!(response.tasks[0].needs_attention);
    }

    #[test]
    fn task_summary_and_inbox_ignore_stale_cached_annotations() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.annotations = vec![Annotation::new(
            AnnotationKind::NeedsMe,
            Evidence::SideFlag(SideFlag::NeedsInput),
        )];

        let tasks = list_tasks(&context, None);
        let inbox = inbox(&context);

        assert!(!tasks.tasks[0].needs_attention);
        assert!(inbox.items.is_empty());
    }

    #[test]
    fn cockpit_inbox_does_not_list_reviewable_tasks_without_input_or_blocker() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.remove_side_flag(SideFlag::NeedsInput);

        assert_eq!(review_queue(&context).tasks.len(), 1);
        assert!(cockpit_inbox(&context).items.is_empty());
    }

    #[rstest]
    #[case(LiveStatusKind::WaitingForInput, "waiting_for_input")]
    #[case(LiveStatusKind::WaitingForApproval, "waiting_for_approval")]
    #[case(LiveStatusKind::CommandFailed, "command_failed")]
    #[case(LiveStatusKind::Blocked, "blocked")]
    #[case(LiveStatusKind::MergeConflict, "merge_conflict")]
    #[case(LiveStatusKind::CiFailed, "ci_failed")]
    fn cockpit_inbox_lists_waiting_and_blocker_live_statuses(
        #[case] live_status: LiveStatusKind,
        #[case] expected_reason: &str,
    ) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.live_status = Some(LiveObservation::new(live_status, expected_reason));

        let response = cockpit_inbox(&context);

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].task_handle, "web/fix-login");
        assert_eq!(response.items[0].reason, expected_reason);
    }

    #[test]
    fn task_summaries_expose_lifecycle_aware_actions() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;

        let active = list_tasks(&context, None);
        assert_eq!(
            active.tasks[0].actions,
            vec![
                OperatorAction::Resume.as_str().to_string(),
                OperatorAction::Drop.as_str().to_string(),
            ]
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Reviewable;
        let reviewable = list_tasks(&context, None);
        assert_eq!(
            reviewable.tasks[0].actions,
            vec![
                OperatorAction::Resume.as_str().to_string(),
                OperatorAction::Ship.as_str().to_string(),
                OperatorAction::Drop.as_str().to_string(),
            ]
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Cleanable;
        let cleanable = list_tasks(&context, None);
        assert_eq!(
            cleanable.tasks[0].actions,
            vec![
                OperatorAction::Resume.as_str().to_string(),
                OperatorAction::Drop.as_str().to_string(),
            ]
        );
    }

    #[test]
    fn task_summaries_expose_drop_for_invalid_task_evidence() {
        for flag in [SideFlag::TmuxMissing, SideFlag::WorktrunkMissing] {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.remove_side_flag(SideFlag::NeedsInput);
            task.add_side_flag(flag);

            let response = list_tasks(&context, None);

            assert_eq!(
                response.tasks[0].actions,
                vec![OperatorAction::Drop.as_str().to_string()],
                "{flag:?}"
            );
            assert_eq!(
                inbox(&context).items[0].action,
                OperatorAction::Drop,
                "{flag:?}"
            );
        }
    }

    #[test]
    fn removed_tasks_are_hidden_from_operational_summaries() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Removed;
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        task.live_status = Some(crate::models::LiveObservation::new(
            LiveStatusKind::WorktreeMissing,
            "worktree missing",
        ));

        assert!(list_tasks(&context, None).tasks.is_empty());
        assert!(inbox(&context).items.is_empty());
    }

    #[test]
    fn missing_substrate_tasks_are_visible_but_not_actionable() {
        for flag in [
            SideFlag::WorktreeMissing,
            SideFlag::BranchMissing,
            SideFlag::TmuxMissing,
            SideFlag::WorktrunkMissing,
        ] {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.remove_side_flag(SideFlag::NeedsInput);
            task.add_side_flag(flag);

            assert_eq!(list_tasks(&context, None).tasks.len(), 1, "{flag:?}");
            assert_eq!(review_queue(&context).tasks.len(), 1, "{flag:?}");
            assert_eq!(inbox(&context).items.len(), 1, "{flag:?}");
            assert_eq!(cockpit(&context).tasks.tasks.len(), 1, "{flag:?}");
            assert_eq!(list_repos(&context).repos[0].active_tasks, 0, "{flag:?}");
            assert_eq!(
                list_repos(&context).repos[0].reviewable_tasks,
                1,
                "{flag:?}"
            );
        }
    }

    #[test]
    fn review_queue_lists_reviewable_and_mergeable_tasks() {
        let mut context = context_with_tasks();
        let mut mergeable = Task::new(
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
        mergeable.lifecycle_status = LifecycleStatus::Mergeable;
        context.registry.create_task(mergeable).unwrap();

        let response = review_queue(&context);

        assert_eq!(response.tasks.len(), 2);
        assert_eq!(response.tasks[0].qualified_handle, "web/fix-login");
        assert_eq!(response.tasks[1].qualified_handle, "api/add-cache");
    }

    #[test]
    fn review_slice_facade_lists_reviewable_and_mergeable_tasks() {
        let mut context = context_with_tasks();
        let mut mergeable = Task::new(
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
        mergeable.lifecycle_status = LifecycleStatus::Mergeable;
        context.registry.create_task(mergeable).unwrap();

        assert_eq!(
            crate::slices::review::review_queue(&context),
            review_queue(&context)
        );
    }

    #[test]
    fn cockpit_includes_review_queue() {
        let mut context = context_with_tasks();
        let mut mergeable = Task::new(
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
        mergeable.lifecycle_status = LifecycleStatus::Mergeable;
        context.registry.create_task(mergeable).unwrap();

        let response = cockpit(&context);

        assert_eq!(response.review.tasks.len(), 2);
        assert_eq!(response.review.tasks[0].qualified_handle, "web/fix-login");
        assert_eq!(response.review.tasks[1].qualified_handle, "api/add-cache");
    }

    #[test]
    fn cockpit_inbox_excludes_reviewable_tasks_without_input_or_blocker() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.remove_side_flag(SideFlag::NeedsInput);

        let view = super::cockpit_view(&context);

        assert!(view.inbox.items.is_empty());
        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.cards[0].qualified_handle, "web/fix-login");
    }

    #[test]
    fn cockpit_view_includes_missing_substrate_tasks_as_drop_only_cards() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TmuxMissing);

        let view = super::cockpit_view(&context);

        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.cards[0].qualified_handle, "web/fix-login");
        assert_eq!(view.cards[0].primary_action, OperatorAction::Drop);
        assert_eq!(view.cards[0].available_actions, vec![OperatorAction::Drop]);
    }

    #[test]
    fn cockpit_scans_registry_once() {
        let context = counting_context_with_tasks();

        let response = cockpit(&context);

        assert_eq!(response.summary.tasks, 1);
        assert_eq!(response.inbox.items.len(), 1);
        assert_eq!(context.registry.list_tasks_calls(), 1);
    }

    #[test]
    fn cockpit_projection_scans_registry_once() {
        let context = counting_context_with_tasks();

        let response = super::cockpit_projection(&context);

        assert_eq!(response.counts.tasks, 1);
        assert_eq!(response.cards.len(), 1);
        assert_eq!(context.registry.list_tasks_calls(), 1);
    }

    #[test]
    fn cockpit_view_scans_registry_once_for_repos_cards_and_inbox() {
        let context = counting_context_with_tasks();

        let view = super::cockpit_view(&context);

        assert_eq!(view.repos.repos.len(), 2);
        assert_eq!(view.cards.len(), 1);
        assert_eq!(view.inbox.items.len(), 1);
        assert_eq!(context.registry.list_tasks_calls(), 1);
    }

    #[test]
    fn cockpit_summary_counts_operator_work() {
        let mut context = context_with_tasks();
        let mut cleanable = Task::new(
            TaskId::new("task-2"),
            "api",
            "remove-cache",
            "Remove cache",
            "ajax/remove-cache",
            "main",
            "/tmp/worktrees/api-remove-cache",
            "ajax-api-remove-cache",
            "worktrunk",
            AgentClient::Claude,
        );
        cleanable.lifecycle_status = LifecycleStatus::Cleanable;
        context.registry.create_task(cleanable).unwrap();

        let response = cockpit(&context);

        assert_eq!(
            response.summary,
            CockpitSummary {
                repos: 2,
                tasks: 2,
                active_tasks: 0,
                attention_items: 2,
                reviewable_tasks: 1,
                cleanable_tasks: 1,
            }
        );
    }

    #[test]
    fn cockpit_next_matches_next_command() {
        let context = context_with_tasks();

        let response = cockpit(&context);

        assert_eq!(response.next, next(&context));
    }

    #[test]
    fn inspect_returns_task_details_by_qualified_handle() {
        let context = context_with_tasks();

        let response = inspect_task(&context, "web/fix-login").unwrap();

        assert_eq!(response.task.qualified_handle, "web/fix-login");
        assert_eq!(response.branch, "ajax/fix-login");
        assert_eq!(response.tmux_session, "ajax-web-fix-login");
        assert_eq!(response.flags, vec!["NeedsInput"]);
    }

    #[test]
    fn inspect_reports_missing_tasks() {
        let context = context_with_tasks();

        let error = inspect_task(&context, "web/missing").unwrap_err();

        assert_eq!(error, CommandError::TaskNotFound("web/missing".to_string()));
    }

    #[test]
    fn inbox_returns_annotation_items_from_task_annotations() {
        let context = context_with_tasks();

        let response = inbox(&context);
        let source = std::fs::read_to_string("src/commands.rs").unwrap();

        assert_eq!(response.items.len(), 1);
        assert_eq!(response.items[0].task_handle, "web/fix-login");
        assert_eq!(response.items[0].reason, "needs_input");
        assert_eq!(response.items[0].severity, 1);
        assert_eq!(response.items[0].action, OperatorAction::Resume);
        assert!(!source.contains(&["fn ", "annotation_items("].concat()));
        assert!(!source.contains(&["fn ", "cockpit_annotation_items("].concat()));
    }

    #[test]
    fn next_returns_first_annotation_item() {
        let context = context_with_tasks();

        let response = next(&context);

        let item = response.item.unwrap();
        assert_eq!(item.task_handle, "web/fix-login");
        assert_eq!(item.reason, "needs_input");
    }

    #[test]
    fn doctor_and_status_return_basic_health() {
        let mut context = context_with_tasks();
        context.config.test_commands = vec![
            TestCommand::new("web", "cargo test"),
            TestCommand::new("api", "cargo test"),
        ];
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
            .with_existing_paths(["/Users/matt/projects/web", "/Users/matt/projects/api"])
            .with_graphify_out_gitignored(std::iter::empty::<std::path::PathBuf>());

        let doctor = doctor_with_environment(&context, &environment);
        let status = status(&context);

        assert!(doctor.checks.iter().all(|check| check.ok));
        assert_eq!(status.tasks.len(), 1);
    }

    #[test]
    fn doctor_reports_required_tool_availability() {
        let context = context_with_tasks();
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux"]);

        let doctor = doctor_with_environment(&context, &environment);

        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "tool:git")
                .map(|check| (check.ok, check.message.as_str())),
            Some((true, "available"))
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "tool:codex")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "not found on PATH"))
        );
    }

    #[test]
    fn doctor_reports_repo_config_problems() {
        let config = Config {
            repos: vec![
                ManagedRepo::new("web", "/repos/web", "main"),
                ManagedRepo::new("web", "/missing/web-copy", "main"),
                ManagedRepo::new("api", "/missing/api", "main"),
            ],
            test_commands: vec![TestCommand::new("web", "cargo test")],
        };
        let context = CommandContext::new(config, InMemoryRegistry::default());
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
            .with_existing_paths(["/repos/web"])
            .with_graphify_out_gitignored(std::iter::empty::<std::path::PathBuf>());

        let doctor = doctor_with_environment(&context, &environment);

        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "config:repo-names")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "duplicate repo name: web"))
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "repo:api:path")
                .map(|check| check.ok),
            Some(false)
        );
        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "repo:api:test-command")
                .map(|check| (check.ok, check.message.as_str())),
            Some((false, "no test command configured"))
        );
    }

    #[test]
    fn doctor_warns_when_graphify_out_is_gitignored() {
        let config = Config {
            repos: vec![ManagedRepo::new("web", "/repos/web", "main")],
            ..Config::default()
        };
        let context = CommandContext::new(config, InMemoryRegistry::default());
        let environment = DoctorEnvironment::from_available_tools(["git", "tmux", "codex"])
            .with_existing_paths(["/repos/web"])
            .with_graphify_out_gitignored(["/repos/web"]);

        let doctor = doctor_with_environment(&context, &environment);

        assert_eq!(
            doctor
                .checks
                .iter()
                .find(|check| check.name == "repo:web:graphify-out")
                .map(|check| (check.ok, check.message.contains("gitignored"))),
            Some((false, true))
        );
    }

    #[test]
    fn stale_task_marking_marks_inactive_old_tasks() {
        let mut context = context_with_tasks();
        let old_activity = std::time::SystemTime::UNIX_EPOCH;
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .last_activity_at = old_activity;

        let changed = mark_stale_tasks(
            &mut context,
            old_activity + std::time::Duration::from_secs(8 * 24 * 60 * 60),
        );

        assert_eq!(changed, 1);
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::Stale));
    }

    #[test]
    fn refresh_git_substrate_evidence_updates_stale_missing_worktree_and_branch() {
        let mut context = context_with_tasks();
        let task_id = TaskId::new("task-1");
        context
            .registry
            .update_git_status(
                &task_id,
                GitStatus {
                    worktree_exists: true,
                    branch_exists: true,
                    current_branch: Some("ajax/fix-login".to_string()),
                    dirty: true,
                    ahead: 2,
                    behind: 0,
                    merged: false,
                    untracked_files: 1,
                    unpushed_commits: 2,
                    conflicted: true,
                    last_commit: Some("abc123".to_string()),
                },
            )
            .unwrap();
        let mut runner = QueuedRunner::new(vec![
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/other\n"),
        ]);

        let changed = refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

        assert!(changed);
        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "list",
                        "--porcelain"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "--format=%(refname:short)"
                    ]
                )
            ]
        );
        let task = context.registry.get_task(&task_id).unwrap();
        let git_status = task.git_status.as_ref().unwrap();
        assert!(!git_status.worktree_exists);
        assert!(!git_status.branch_exists);
        assert_eq!(git_status.current_branch, None);
        assert!(!git_status.dirty);
        assert_eq!(git_status.untracked_files, 0);
        assert_eq!(git_status.unpushed_commits, 0);
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn refresh_git_substrate_evidence_ignores_empty_worktree_listing() {
        let mut context = context_with_tasks();
        let task_id = TaskId::new("task-1");
        let cached_status = GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 2,
            behind: 0,
            merged: false,
            untracked_files: 1,
            unpushed_commits: 2,
            conflicted: true,
            last_commit: Some("abc123".to_string()),
        };
        context
            .registry
            .update_git_status(&task_id, cached_status.clone())
            .unwrap();
        let mut runner = QueuedRunner::new(vec![output(0, ""), output(0, "main\n")]);

        let changed = refresh_git_substrate_evidence(&mut context, &mut runner).unwrap();

        assert!(!changed);
        assert_eq!(
            context.registry.get_task(&task_id).unwrap().git_status,
            Some(cached_status)
        );
    }

    #[test]
    fn new_task_plan_validates_repo_and_builds_native_lifecycle() {
        let context = context_with_tasks();

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "fix logout".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert!(!plan.requires_confirmation);
        let git = GitAdapter::new("git");
        assert_eq!(
            plan.commands,
            vec![
                git.fetch_origin_branch("/Users/matt/projects/web", "main"),
                git.sync_default_branch_from_origin("/Users/matt/projects/web", "main"),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "add",
                        "-b",
                        "ajax/fix-logout",
                        "/Users/matt/projects/web__worktrees/ajax-fix-logout",
                        "main"
                    ]
                ),
                CommandSpec::new(
                    "/bin/sh",
                    [
                        "-lc",
                        "cd \"$1\" 2>/dev/null || exit 0; if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi",
                        "sh",
                        "/Users/matt/projects/web__worktrees/ajax-fix-logout"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-logout",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/Users/matt/projects/web__worktrees/ajax-fix-logout"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "send-keys",
                        "-t",
                        "ajax-web-fix-logout:worktrunk",
                        "codex --cd /Users/matt/projects/web__worktrees/ajax-fix-logout",
                        "Enter"
                    ]
                )
            ]
        );
    }

    #[test]
    fn new_task_plan_preserves_paths_with_spaces_as_command_arguments() {
        let context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new(
                    "web",
                    "/Users/matt/projects/web app",
                    "main",
                )],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        assert_eq!(plan.commands[0].args[1], "/Users/matt/projects/web app");
        assert_eq!(plan.commands[1].args[1], "/Users/matt/projects/web app");
        assert_eq!(
            plan.commands[2].args[6],
            "/Users/matt/projects/web app__worktrees/ajax-fix-login"
        );
        assert_eq!(
            plan.commands[3].args[3],
            "/Users/matt/projects/web app__worktrees/ajax-fix-login"
        );
        assert_eq!(
            plan.commands[4].args[7],
            "/Users/matt/projects/web app__worktrees/ajax-fix-login"
        );
        assert_eq!(
            shell_words(&plan.commands[5].args[3]),
            vec![
                "codex",
                "--cd",
                "/Users/matt/projects/web app__worktrees/ajax-fix-login",
            ]
        );
    }

    #[test]
    fn new_task_plan_rejects_unknown_repo() {
        let context = context_with_tasks();

        let error = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "missing".to_string(),
                title: "fix login".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(error, CommandError::RepoNotFound("missing".to_string()));
    }

    #[test]
    fn new_task_contract_preserves_generated_names_and_duplicate_handles() {
        let mut context = context_with_tasks();

        let missing_repo = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "missing".to_string(),
                title: "Ship oauth v2!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(
            missing_repo,
            CommandError::RepoNotFound("missing".to_string())
        );

        let plan = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "api".to_string(),
                title: "Ship oauth v2!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();
        assert_eq!(plan.title, "create task: Ship oauth v2!");
        let worktree_command = plan
            .commands
            .iter()
            .find(|command| super::is_git_worktree_add_command(command))
            .expect("worktree add command");
        let send_keys = plan
            .commands
            .iter()
            .find(|command| super::is_agent_send_keys_command(command))
            .expect("agent send-keys command");
        assert_eq!(worktree_command.args[5], "ajax/ship-oauth-v2");
        assert_eq!(
            worktree_command.args[6],
            "/Users/matt/projects/api__worktrees/ajax-ship-oauth-v2"
        );
        assert_eq!(
            plan.commands
                .iter()
                .find(|command| super::is_new_task_husky_hook_command(command))
                .expect("husky command")
                .args[3],
            "/Users/matt/projects/api__worktrees/ajax-ship-oauth-v2"
        );
        assert_eq!(
            plan.commands
                .iter()
                .find(|command| super::is_worktrunk_new_session_command(command))
                .expect("tmux session command")
                .args[3],
            "ajax-api-ship-oauth-v2"
        );
        assert_eq!(send_keys.args[2], "ajax-api-ship-oauth-v2:worktrunk");
        assert_eq!(
            send_keys.args[3],
            "codex --cd /Users/matt/projects/api__worktrees/ajax-ship-oauth-v2"
        );

        let active_duplicate = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap_err();
        assert_eq!(
            active_duplicate,
            CommandError::PlanBlocked(vec!["task already exists: web/fix-login".to_string()])
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Removed;
        let removed_duplicate = new_task_plan(
            &context,
            NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();

        let removed_worktree = removed_duplicate
            .commands
            .iter()
            .find(|command| super::is_git_worktree_add_command(command))
            .expect("worktree add command");
        let removed_session = removed_duplicate
            .commands
            .iter()
            .find(|command| super::is_worktrunk_new_session_command(command))
            .expect("tmux session command");
        assert_eq!(removed_worktree.args[5], "ajax/fix-login");
        assert_eq!(removed_session.args[3], "ajax-web-fix-login");
    }

    #[test]
    fn new_task_request_creates_provisional_task_record() {
        let context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
        };

        let task = task_from_new_request(&context, &request).unwrap();

        assert_eq!(task.id.as_str(), "web/fix-login");
        assert_eq!(task.handle, "fix-login");
        assert_eq!(task.branch, "ajax/fix-login");
        assert_eq!(task.tmux_session, "ajax-web-fix-login");
        assert_eq!(
            task.worktree_path.to_string_lossy(),
            "/Users/matt/projects/web__worktrees/ajax-fix-login"
        );
        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
        assert_eq!(task.selected_agent, AgentClient::Codex);
    }

    #[test]
    fn new_task_request_slugifies_blank_titles_to_task() {
        let context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "!!!".to_string(),
            agent: "claude".to_string(),
        };

        let task = task_from_new_request(&context, &request).unwrap();

        assert_eq!(task.handle, "task");
        assert_eq!(task.selected_agent, AgentClient::Claude);
    }

    #[test]
    fn record_new_task_adds_provisional_task_to_registry() {
        let mut context = context_with_tasks();
        let request = NewTaskRequest {
            repo: "api".to_string(),
            title: "Add cache".to_string(),
            agent: "codex".to_string(),
        };

        let task = super::record_new_task(&mut context, &request).unwrap();

        assert_eq!(task.qualified_handle(), "api/add-cache");
        assert!(context
            .registry
            .list_tasks()
            .iter()
            .any(|task| task.qualified_handle() == "api/add-cache"));
    }

    #[test]
    fn record_new_task_reuses_removed_task_handle() {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut removed = task_from_new_request(
            &context,
            &NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login!".to_string(),
                agent: "codex".to_string(),
            },
        )
        .unwrap();
        removed.lifecycle_status = LifecycleStatus::Removed;
        context.registry.create_task(removed).unwrap();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login!".to_string(),
            agent: "codex".to_string(),
        };

        let task = super::record_new_task(&mut context, &request).unwrap();

        assert_eq!(task.qualified_handle(), "web/fix-login");
        assert_eq!(context.registry.list_tasks().len(), 1);
        assert_eq!(
            context.registry.list_tasks()[0].lifecycle_status,
            LifecycleStatus::Provisioning
        );
    }

    #[test]
    fn new_task_provisioning_state_updates_live_in_core() {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
        };
        let task = super::record_new_task(&mut context, &request).unwrap();
        let task_id = task.id.clone();

        super::mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::WorktreeCreated,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| status.worktree_exists && status.branch_exists));
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));

        super::mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::TaskSessionCreated,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(
            task.tmux_status,
            Some(TmuxStatus::present("ajax-web-fix-login"))
        );
        assert_eq!(
            task.worktrunk_status,
            Some(WorktrunkStatus::present(
                "worktrunk",
                "/Users/matt/projects/web__worktrees/ajax-fix-login"
            ))
        );
        assert!(!task.has_side_flag(SideFlag::TmuxMissing));
        assert!(!task.has_side_flag(SideFlag::WorktrunkMissing));

        super::mark_new_task_provisioning_step_completed(
            &mut context,
            &task_id,
            StartProvisioningStep::AgentCommandSent,
        )
        .unwrap();
        let task = context.registry.get_task(&task_id).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
        assert_eq!(task.agent_attempts.len(), 1);
        assert_eq!(task.agent_attempts[0].agent, AgentClient::Codex);
        assert_eq!(
            task.agent_attempts[0].launch_target,
            "/Users/matt/projects/web__worktrees/ajax-fix-login"
        );
        assert!(task.has_side_flag(SideFlag::AgentRunning));

        let mut failing_context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/Users/matt/projects/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let failing_task = super::record_new_task(&mut failing_context, &request).unwrap();
        super::mark_new_task_provisioning_failed(&mut failing_context, &failing_task.id).unwrap();
        let failing_task = failing_context.registry.get_task(&failing_task.id).unwrap();
        assert_eq!(failing_task.lifecycle_status, LifecycleStatus::Error);
        assert!(failing_task.has_side_flag(SideFlag::NeedsInput));
    }

    #[test]
    fn open_task_plan_targets_worktrunk_directly() {
        let context = context_with_tasks();

        let outside_tmux = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
        let inside_tmux =
            open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

        assert_eq!(
            outside_tmux.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
        assert_eq!(
            inside_tmux.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn open_use_case_module_targets_worktrunk_directly() {
        let context = context_with_tasks();

        let plan =
            super::open::open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert_eq!(plan.title, "open task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn open_task_plan_emits_no_commands_for_no_attach_mode() {
        let context = context_with_tasks();

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::NoAttach).unwrap();

        assert!(plan.blocked_reasons.is_empty(), "{plan:?}");
        assert!(plan.commands.is_empty(), "{plan:?}");
    }

    #[test]
    fn open_task_plan_blocks_removed_tasks() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Removed;

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task is removed"]);
    }

    #[test]
    fn direct_task_plans_block_removed_tasks() {
        let mut context = context_with_test_command();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Removed;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });

        let plans = [
            open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap(),
            merge_task_plan(&context, "web/fix-login").unwrap(),
            clean_task_plan(&context, "web/fix-login").unwrap(),
            check_task_plan(&context, "web/fix-login").unwrap(),
            diff_task_plan(&context, "web/fix-login").unwrap(),
        ];

        for plan in plans {
            assert!(plan.commands.is_empty(), "{}", plan.title);
            assert!(
                plan.blocked_reasons
                    .iter()
                    .any(|reason| reason == "task is removed"),
                "{}: {:?}",
                plan.title,
                plan.blocked_reasons
            );
        }
    }

    #[test]
    fn check_task_plan_runs_configured_command_in_task_worktree() {
        let context = context_with_test_command();

        let plan = check_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "check task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("sh", ["-lc", "cargo test"])
                .with_cwd("/tmp/worktrees/web-fix-login")]
        );
    }

    #[test]
    fn check_use_case_module_plans_configured_command_in_task_worktree() {
        let context = context_with_test_command();

        let plan = super::check::check_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "check task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![CommandSpec::new("sh", ["-lc", "cargo test"])
                .with_cwd("/tmp/worktrees/web-fix-login")]
        );
    }

    #[test]
    fn check_task_plan_blocks_missing_worktree() {
        let mut context = context_with_test_command();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let plan = check_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
    }

    #[test]
    fn diff_task_plan_summarizes_branch_diff_in_task_worktree() {
        let context = context_with_tasks();

        let plan = diff_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "diff task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("git", ["diff", "--stat", "main...ajax/fix-login"])
                    .with_cwd("/tmp/worktrees/web-fix-login")
            ]
        );
    }

    #[test]
    fn diff_use_case_module_summarizes_branch_diff_in_task_worktree() {
        let context = context_with_tasks();

        let plan = super::diff::diff_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan.title, "diff task: web/fix-login");
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("git", ["diff", "--stat", "main...ajax/fix-login"])
                    .with_cwd("/tmp/worktrees/web-fix-login")
            ]
        );
    }

    #[test]
    fn review_slice_facade_summarizes_branch_diff_in_task_worktree() {
        let context = context_with_tasks();

        let plan = crate::slices::review::review_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(plan, diff_task_plan(&context, "web/fix-login").unwrap());
    }

    #[test]
    fn diff_task_plan_blocks_missing_worktree() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);

        let plan = diff_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task worktree is missing"]);
    }

    #[test]
    fn trunk_task_plan_still_repairs_missing_tmux_flag() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::TmuxMissing);

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn trunk_use_case_module_repairs_missing_tmux_flag() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::TmuxMissing);

        let plan = super::trunk::trunk_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn open_task_plan_blocks_missing_tmux_instead_of_repairing_trunk() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::TmuxMissing);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = None;

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert_eq!(plan.title, "open task: web/fix-login");
        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[test]
    fn open_task_plan_blocks_missing_tmux_as_not_openable() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::TmuxMissing);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[test]
    fn open_task_plan_blocks_missing_tmux_inside_tmux() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::TmuxMissing);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = None;

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

        assert_eq!(plan.title, "open task: web/fix-login");
        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[test]
    fn open_task_plan_blocks_unobservable_runtime_projection_until_refresh() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Unobservable,
            std::time::SystemTime::UNIX_EPOCH,
            RuntimeObservationSource::TmuxProbe,
        );

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.blocked_reasons,
            vec!["runtime state is unobservable; refresh before resume"]
        );
    }

    #[test]
    fn open_task_plan_allows_old_healthy_complete_runtime_projection() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            std::time::SystemTime::UNIX_EPOCH,
            RuntimeObservationSource::TmuxProbe,
        );

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn open_task_plan_allows_stale_runtime_when_live_status_requests_resume() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus::present("ajax-web-fix-login"));
        task.worktrunk_status = Some(WorktrunkStatus::present(
            "worktrunk",
            "/tmp/worktrees/web-fix-login",
        ));
        task.live_status = Some(LiveObservation::new(LiveStatusKind::Blocked, "blocked"));
        task.runtime_projection = RuntimeProjection::new(
            RuntimeHealth::Healthy,
            std::time::SystemTime::UNIX_EPOCH,
            RuntimeObservationSource::TmuxProbe,
        );

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn lifecycle_transitions_update_registry_status() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Created;

        super::mark_task_opened(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Created
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Mergeable;
        super::mark_task_merged(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Merged
        );

        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Cleanable;
        super::mark_task_removed(&mut context, "web/fix-login").unwrap();
        assert_eq!(
            context
                .registry
                .get_task(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status,
            LifecycleStatus::Removed
        );
    }

    #[test]
    fn mark_task_opened_preserves_existing_lifecycle() {
        for status in [
            LifecycleStatus::Reviewable,
            LifecycleStatus::Merged,
            LifecycleStatus::Cleanable,
        ] {
            let mut context = context_with_tasks();
            context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap()
                .lifecycle_status = status;

            super::mark_task_opened(&mut context, "web/fix-login").unwrap();

            assert_eq!(
                context
                    .registry
                    .get_task(&TaskId::new("task-1"))
                    .unwrap()
                    .lifecycle_status,
                status
            );
        }
    }

    #[test]
    fn merge_plan_requires_confirmation_when_task_needs_attention() {
        let context = context_with_tasks();

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"]),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "merge",
                        "--ff-only",
                        "ajax/fix-login"
                    ]
                )
            ]
        );
    }

    #[test]
    fn merge_task_plan_blocks_non_review_states() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.blocked_reasons,
            vec!["merge requires reviewable or mergeable lifecycle"]
        );
    }

    #[test]
    fn merge_task_plan_allows_mergeable_tasks() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Mergeable;

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn merge_result_updates_replace_failed_merge_attention() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Mergeable;
        task.add_side_flag(SideFlag::Conflicted);
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "merge failed",
        ));

        super::mark_task_merged(&mut context, "web/fix-login").unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Merged);
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert!(task.live_status.is_none());
    }

    #[test]
    fn merge_result_preserves_unrelated_command_failure_attention() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Mergeable;
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "check failed",
        ));

        super::mark_task_merged(&mut context, "web/fix-login").unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.live_status.as_ref().is_some_and(|status| {
            status.kind == LiveStatusKind::CommandFailed && status.summary == "check failed"
        }));
    }

    #[rstest]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: true,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        }),
        None,
        "merge requires clean worktree evidence"
    )]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: true,
            last_commit: None,
        }),
        None,
        "merge requires clean worktree evidence"
    )]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 1,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        }),
        None,
        "merge requires clean worktree evidence"
    )]
    #[case(
        Some(GitStatus {
            worktree_exists: true,
            branch_exists: false,
            current_branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        }),
        None,
        "task branch is missing"
    )]
    #[case(None, Some(SideFlag::Dirty), "merge requires clean worktree evidence")]
    #[case(
        None,
        Some(SideFlag::Conflicted),
        "merge requires clean worktree evidence"
    )]
    #[case(None, Some(SideFlag::BranchMissing), "task branch is missing")]
    fn merge_task_plan_blocks_risky_or_missing_branch_evidence(
        #[case] git_status: Option<GitStatus>,
        #[case] side_flag: Option<SideFlag>,
        #[case] expected_reason: &str,
    ) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.git_status = git_status;
        if let Some(side_flag) = side_flag {
            task.add_side_flag(side_flag);
        }

        let plan = merge_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec![expected_reason]);
    }

    #[test]
    fn clean_plan_uses_policy_and_native_cleanup() {
        let context = context_with_cleanable_task();

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(!plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-d",
                        "ajax/fix-login"
                    ]
                ),
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
            ]
        );
    }

    #[rstest]
    #[case(SideFlag::Dirty)]
    #[case(SideFlag::Conflicted)]
    fn clean_plan_requires_confirmation_for_risky_cleanup(#[case] side_flag: SideFlag) {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(side_flag);
        if let Some(git_status) = task.git_status.as_mut() {
            match side_flag {
                SideFlag::Dirty => {
                    git_status.dirty = true;
                }
                SideFlag::Conflicted => {
                    git_status.conflicted = true;
                }
                _ => {}
            }
        }

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert!(!plan.commands.is_empty());
        assert!(plan.blocked_reasons.is_empty());
    }

    #[test]
    fn clean_task_plan_blocks_non_cleanup_lifecycle() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .lifecycle_status = LifecycleStatus::Active;

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(
            plan.blocked_reasons,
            vec!["clean requires merged or cleanable lifecycle"]
        );
    }

    #[test]
    fn remove_task_plan_force_removes_active_task_resources() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: true,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: task.tmux_session.clone(),
        });

        let plan = remove_task_plan(&context, "web/fix-login").unwrap();

        assert!(plan.requires_confirmation);
        assert!(plan.blocked_reasons.is_empty());
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "--force",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-D",
                        "ajax/fix-login"
                    ]
                ),
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
            ]
        );
    }

    #[test]
    fn remove_task_plan_keeps_removing_remaining_resources_for_invalid_tasks() {
        for arrange in [
            |task: &mut Task| {
                task.tmux_status = Some(TmuxStatus {
                    exists: false,
                    session_name: task.tmux_session.clone(),
                });
            },
            |task: &mut Task| {
                task.git_status.as_mut().unwrap().worktree_exists = false;
            },
            |task: &mut Task| {
                task.git_status.as_mut().unwrap().branch_exists = false;
            },
            |task: &mut Task| {
                task.worktrunk_status = Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                });
            },
        ] {
            let mut context = context_with_tasks();
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.remove_side_flag(SideFlag::NeedsInput);
            task.lifecycle_status = LifecycleStatus::Active;
            task.git_status = Some(GitStatus {
                worktree_exists: true,
                branch_exists: true,
                current_branch: Some("ajax/fix-login".to_string()),
                dirty: false,
                ahead: 0,
                behind: 0,
                merged: false,
                untracked_files: 0,
                unpushed_commits: 0,
                conflicted: false,
                last_commit: None,
            });
            task.tmux_status = Some(TmuxStatus {
                exists: true,
                session_name: task.tmux_session.clone(),
            });
            task.worktrunk_status = Some(WorktrunkStatus::present(
                task.worktrunk_window.clone(),
                task.worktree_path.clone(),
            ));
            arrange(task);

            let plan = remove_task_plan(&context, "web/fix-login").unwrap();

            assert!(plan.requires_confirmation);
            assert!(plan.blocked_reasons.is_empty());
            assert!(
                !plan.commands.is_empty(),
                "invalid task should still remove remaining resources"
            );
            assert!(
                plan.commands
                    .iter()
                    .all(|command| { command.program == "tmux" || command.program == "git" }),
                "unexpected teardown commands: {:?}",
                plan.commands
            );
        }
    }

    #[test]
    fn drop_plan_from_observation_resumes_from_live_resource_state() {
        let observation = DropObservation {
            agent: ResourceState::Present,
            tmux_session: ResourceState::Absent,
            worktree: ResourceState::Unknown,
            branch: ResourceState::Present,
        };

        let ops = plan_drop_from_observation(&observation);

        assert_eq!(
            ops,
            vec![
                DropOp::EnsureAgentStopped,
                DropOp::EnsureWorktreeAbsent,
                DropOp::EnsureBranchAbsent,
            ]
        );
    }

    #[test]
    fn drop_plan_from_observation_tears_down_git_before_tmux() {
        let observation = DropObservation {
            agent: ResourceState::Present,
            tmux_session: ResourceState::Present,
            worktree: ResourceState::Present,
            branch: ResourceState::Present,
        };

        let ops = plan_drop_from_observation(&observation);

        assert_eq!(
            ops,
            vec![
                DropOp::EnsureAgentStopped,
                DropOp::EnsureWorktreeAbsent,
                DropOp::EnsureBranchAbsent,
                DropOp::EnsureTmuxSessionAbsent,
            ]
        );
    }

    #[test]
    fn drop_plan_from_observation_for_task_skips_receipted_steps() {
        use crate::models::{StepReceipt, TaskId, TaskOperationKind};

        let observation = DropObservation {
            agent: ResourceState::Absent,
            tmux_session: ResourceState::Present,
            worktree: ResourceState::Present,
            branch: ResourceState::Present,
        };
        let receipts = vec![
            StepReceipt::succeeded(
                TaskId::new("web/fix-login"),
                TaskOperationKind::Drop,
                "worktree_absent",
                "/repo/web__worktrees/ajax-fix-login",
                "{}",
            ),
            StepReceipt::succeeded(
                TaskId::new("web/fix-login"),
                TaskOperationKind::Drop,
                "branch_absent",
                "ajax/fix-login",
                "{}",
            ),
        ];

        let ops = plan_drop_from_observation_for_task(&observation, &receipts);

        assert_eq!(ops, vec![DropOp::EnsureTmuxSessionAbsent]);
    }

    #[test]
    fn observe_drop_resources_prefers_live_tmux_and_git_state_over_registry_cache() {
        let mut context = context_with_cleanable_task();
        let task_id = TaskId::new("task-1");
        context
            .registry
            .update_tmux_status(
                &task_id,
                Some(TmuxStatus {
                    exists: false,
                    session_name: "ajax-web-fix-login".to_string(),
                }),
            )
            .unwrap();
        let task = context.registry.get_task(&task_id).unwrap().clone();
        let mut runner = QueuedRunner::new(vec![
            output(0, "ajax-web-fix-login\n"),
            output(
                0,
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\n",
            ),
            output(0, "main\najax/fix-login\n"),
        ]);

        let observation = observe_drop_resources(&mut context, &task, &mut runner).unwrap();

        assert_eq!(observation.tmux_session, ResourceState::Present);
        assert_eq!(observation.worktree, ResourceState::Absent);
        assert_eq!(observation.branch, ResourceState::Present);
        assert_eq!(
            runner.commands,
            vec![
                CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                    .with_timeout(std::time::Duration::from_secs(8)),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "list",
                        "--porcelain"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "--format=%(refname:short)"
                    ]
                ),
            ]
        );
        let task = context.registry.get_task(&task_id).unwrap();
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists));
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists && status.branch_exists));
    }

    #[test]
    fn cleanup_and_remove_plans_are_distinct() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });

        let cleanup = clean_task_plan(&context, "web/fix-login").unwrap();
        let remove = remove_task_plan(&context, "web/fix-login").unwrap();

        assert!(!cleanup.requires_confirmation);
        assert!(remove.requires_confirmation);
        assert_ne!(cleanup.commands, remove.commands);
        assert!(remove.commands.iter().any(|command| {
            command.program == "git"
                && command.args.iter().any(|arg| arg == "--force")
                && command.args.iter().any(|arg| arg == "worktree")
        }));
        assert!(remove.commands.iter().any(|command| {
            command.program == "git" && command.args.iter().any(|arg| arg == "-D")
        }));
    }

    #[test]
    fn teardown_commands_use_force_flag_without_mode_enum() {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/teardown.rs"),
        )
        .unwrap();

        assert!(!source.contains("TeardownMode"));
    }

    #[test]
    fn teardown_step_result_ignores_unrelated_resource_commands() {
        let unrelated_commands = [
            CommandSpec::new("tmux", ["kill-session", "-t", "other-session"]),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/tmp/worktrees/other-task",
                ],
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/other-task",
                ],
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "remove",
                    "/tmp/worktrees/web-fix-login",
                ],
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "prune",
                    "/tmp/worktrees/web-fix-login",
                ],
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "-d",
                    "ajax/fix-login",
                ],
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "--list",
                    "ajax/fix-login",
                ],
            ),
        ];

        for command in unrelated_commands {
            let mut context = context_with_cleanable_task();
            let changed =
                super::mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command)
                    .unwrap();

            let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
            assert!(!changed);
            assert!(task
                .tmux_status
                .as_ref()
                .is_some_and(|status| status.exists));
            assert!(task
                .git_status
                .as_ref()
                .is_some_and(|status| status.worktree_exists && status.branch_exists));
            assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
            assert!(!task.has_side_flag(SideFlag::BranchMissing));
        }
    }

    #[test]
    fn teardown_step_result_records_matching_tmux_cleanup() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::TmuxMissing);
        task.add_side_flag(SideFlag::WorktrunkMissing);
        let command = CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"]);

        let changed =
            super::mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command)
                .unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(changed);
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| !status.exists && status.session_name == "ajax-web-fix-login"));
        assert!(task.worktrunk_status.as_ref().is_some_and(|status| {
            !status.exists
                && status.window_name == "worktrunk"
                && !status.points_at_expected_path
                && status.current_path == task.worktree_path
        }));
        assert!(
            task.has_side_flag(SideFlag::TmuxMissing),
            "missing-substrate flags stay until drop completes so retries stay visible"
        );
        assert!(
            task.has_side_flag(SideFlag::WorktrunkMissing),
            "missing-substrate flags stay until drop completes so retries stay visible"
        );
    }

    #[test]
    fn teardown_step_result_records_matching_worktree_cleanup() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        let git_status = task.git_status.as_mut().unwrap();
        git_status.dirty = true;
        git_status.conflicted = true;
        git_status.untracked_files = 2;
        task.add_side_flag(SideFlag::Dirty);
        task.add_side_flag(SideFlag::Conflicted);
        let command = CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "worktree",
                "remove",
                "/tmp/worktrees/web-fix-login",
            ],
        );

        let changed =
            super::mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command)
                .unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        let git_status = task.git_status.as_ref().unwrap();
        assert!(changed);
        assert!(!git_status.worktree_exists);
        assert!(!git_status.dirty);
        assert!(!git_status.conflicted);
        assert_eq!(git_status.untracked_files, 0);
        assert!(!task.has_side_flag(SideFlag::Dirty));
        assert!(!task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn teardown_step_result_records_matching_branch_cleanup() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        let git_status = task.git_status.as_mut().unwrap();
        git_status.ahead = 2;
        git_status.behind = 1;
        git_status.unpushed_commits = 2;
        task.add_side_flag(SideFlag::Unpushed);
        let command = CommandSpec::new(
            "git",
            [
                "-C",
                "/Users/matt/projects/web",
                "branch",
                "-d",
                "ajax/fix-login",
            ],
        );

        let changed =
            super::mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command)
                .unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        let git_status = task.git_status.as_ref().unwrap();
        assert!(changed);
        assert!(!git_status.branch_exists);
        assert!(git_status.current_branch.is_none());
        assert_eq!(git_status.ahead, 0);
        assert_eq!(git_status.behind, 0);
        assert_eq!(git_status.unpushed_commits, 0);
        assert!(!task.has_side_flag(SideFlag::Unpushed));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn cleanup_git_status_bookkeeping_updates_only_cleanup_evidence() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Merged;
        task.remove_side_flag(SideFlag::NeedsInput);
        task.git_status = None;
        task.tmux_status = None;
        task.worktrunk_status = None;
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktrees/web-fix-login",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )]
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Merged);
        assert!(task.git_status.as_ref().is_some_and(|status| {
            status.worktree_exists
                && status.branch_exists
                && status.merged
                && !status.dirty
                && status.untracked_files == 0
        }));
        assert!(task.tmux_status.is_none());
        assert!(task.worktrunk_status.is_none());
        assert!(task.live_status.is_none());
    }

    #[test]
    fn cleanup_git_status_refreshes_even_when_cached_status_exists() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status.as_mut().unwrap().dirty = true;
        task.add_side_flag(SideFlag::Dirty);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        assert_eq!(
            runner.commands,
            vec![CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktrees/web-fix-login",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )]
        );
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.git_status.as_ref().unwrap().dirty);
        assert!(!task.has_side_flag(SideFlag::Dirty));
    }

    #[test]
    fn cleanup_git_status_keeps_active_unmerged_evidence_unmerged() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.lifecycle_status = LifecycleStatus::Active;
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.git_status.as_ref().unwrap().merged);
    }

    #[test]
    fn cleanup_git_status_treats_cleanable_task_as_merged_even_without_cached_merge() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .git_status
            .as_mut()
            .unwrap()
            .merged = false;
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::ensure_cleanup_git_status(&mut context, "web/fix-login", &mut runner).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.git_status.as_ref().unwrap().merged);
    }

    #[test]
    fn git_evidence_refresh_parses_status_and_side_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status = None;
        task.remove_side_flag(SideFlag::NeedsInput);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login [ahead 2]\nUU src/lib.rs\n?? notes.md\n",
        )]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        let git_status = task.git_status.as_ref().unwrap();
        assert!(git_status.worktree_exists);
        assert!(git_status.branch_exists);
        assert_eq!(git_status.current_branch.as_deref(), Some("ajax/fix-login"));
        assert!(git_status.dirty);
        assert!(git_status.conflicted);
        assert_eq!(git_status.untracked_files, 1);
        assert_eq!(git_status.unpushed_commits, 2);
        assert!(task.has_side_flag(SideFlag::Dirty));
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert!(task.has_side_flag(SideFlag::Unpushed));
    }

    #[test]
    fn git_evidence_refresh_clears_recovered_missing_worktree_and_branch_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::WorktreeMissing);
        task.add_side_flag(SideFlag::BranchMissing);
        let mut runner = QueuedRunner::new(vec![output(
            0,
            "## ajax/fix-login...origin/ajax/fix-login\n",
        )]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(!task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn git_evidence_refresh_preserves_unresolved_missing_flags() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.add_side_flag(SideFlag::BranchMissing);
        let mut runner = QueuedRunner::new(vec![output(0, "## HEAD (no branch)\n")]);

        super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false).unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
        assert!(task.has_side_flag(SideFlag::BranchMissing));
    }

    #[test]
    fn failed_git_evidence_refresh_preserves_existing_missing_flags() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::WorktreeMissing);
        let mut runner = QueuedRunner::new(vec![CommandOutput {
            status_code: 128,
            stdout: String::new(),
            stderr: "not a git repository".to_string(),
        }]);

        let result = super::refresh_git_evidence(&mut context, "web/fix-login", &mut runner, false);

        assert!(result.is_err());
        assert!(context
            .registry
            .get_task(&TaskId::new("task-1"))
            .unwrap()
            .has_side_flag(SideFlag::WorktreeMissing));
    }

    #[test]
    fn confirmed_cleanup_deletes_existing_unmerged_branch() {
        let mut context = context_with_cleanable_task();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.git_status.as_mut().unwrap().merged = false;
        task.add_side_flag(SideFlag::NeedsInput);

        let plan = clean_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-D",
                        "ajax/fix-login"
                    ]
                ),
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
            ]
        );
    }

    #[test]
    fn sweep_cleanup_plans_only_safe_candidates() {
        let context = context_with_cleanable_task();

        let plan = sweep_cleanup_plan(&context);
        let candidates = super::sweep_cleanup_candidates(&context);

        assert_eq!(candidates, vec!["web/fix-login"]);
        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "worktree",
                        "remove",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        "/Users/matt/projects/web",
                        "branch",
                        "-d",
                        "ajax/fix-login"
                    ]
                ),
                CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
            ]
        );
    }

    #[test]
    fn sweep_cleanup_ignores_removed_tasks() {
        let mut context = context_with_cleanable_task();
        context
            .registry
            .update_lifecycle(&TaskId::new("task-1"), LifecycleStatus::Removed)
            .unwrap();

        let plan = sweep_cleanup_plan(&context);
        let candidates = super::sweep_cleanup_candidates(&context);

        assert!(plan.commands.is_empty());
        assert!(candidates.is_empty());
    }

    #[test]
    fn open_task_plan_blocks_missing_trunk_substrate() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TmuxMissing);
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::SwitchClient).unwrap();

        assert_eq!(plan.title, "open task: web/fix-login");
        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[rstest]
    #[case::worktrunk_side_flag(|task: &mut Task| task.add_side_flag(SideFlag::WorktrunkMissing))]
    #[case::tmux_status_missing(|task: &mut Task| {
        task.tmux_status = Some(TmuxStatus {
            exists: false,
            session_name: "ajax-web-fix-login".to_string(),
        });
    })]
    #[case::worktrunk_status_missing(|task: &mut Task| {
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: false,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/worktrees/web-fix-login".into(),
            points_at_expected_path: true,
        });
    })]
    #[case::worktrunk_wrong_path(|task: &mut Task| {
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: true,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/other".into(),
            points_at_expected_path: false,
        });
    })]
    fn open_task_plan_blocks_each_trunk_substrate_signal(#[case] arrange_task: fn(&mut Task)) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        arrange_task(task);

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert_eq!(plan.title, "open task: web/fix-login");
        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[test]
    fn open_task_plan_blocks_missing_git_substrate_instead_of_repairing_trunk() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TmuxMissing);
        task.add_side_flag(SideFlag::WorktreeMissing);

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[rstest]
    #[case::missing_worktree(|status: &mut GitStatus| status.worktree_exists = false)]
    #[case::missing_branch(|status: &mut GitStatus| status.branch_exists = false)]
    fn open_task_plan_blocks_missing_git_status_instead_of_repairing_trunk(
        #[case] arrange_git_status: fn(&mut GitStatus),
    ) {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.remove_side_flag(SideFlag::NeedsInput);
        task.add_side_flag(SideFlag::TmuxMissing);
        task.git_status = Some(GitStatus {
            worktree_exists: true,
            branch_exists: true,
            current_branch: Some("ajax/fix-login".to_string()),
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: false,
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        arrange_git_status(task.git_status.as_mut().unwrap());

        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();

        assert!(plan.commands.is_empty());
        assert_eq!(plan.blocked_reasons, vec!["task has missing substrate"]);
    }

    #[test]
    fn mark_task_opened_reports_missing_task() {
        let mut context = context_with_tasks();

        let result = super::mark_task_opened(&mut context, "web/missing");

        assert!(
            matches!(result, Err(CommandError::TaskNotFound(handle)) if handle == "web/missing")
        );
    }

    #[test]
    fn command_result_markers_update_visible_task_state() {
        let mut context = context_with_test_command();
        {
            let task = context
                .registry
                .get_task_mut(&TaskId::new("task-1"))
                .unwrap();
            task.lifecycle_status = LifecycleStatus::Active;
            task.add_side_flag(SideFlag::TestsFailed);
        }

        super::mark_task_check_started(&mut context, "web/fix-login").unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(!task.has_side_flag(SideFlag::TestsFailed));
        assert!(task
            .live_status
            .as_ref()
            .is_some_and(|status| status.kind == LiveStatusKind::TestsRunning));

        super::mark_task_check_succeeded(&mut context, "web/fix-login").unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Reviewable);
        assert!(task.live_status.is_none());

        super::mark_task_check_failed(&mut context, "web/fix-login").unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::TestsFailed));
        assert!(task.live_status.as_ref().is_some_and(|status| {
            status.kind == LiveStatusKind::CommandFailed && status.summary == "check failed"
        }));
    }

    #[test]
    fn check_success_preserves_unrelated_live_status() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CommandFailed,
            "agent failed",
        ));

        super::mark_task_check_succeeded(&mut context, "web/fix-login").unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.live_status.as_ref().is_some_and(|status| {
            status.kind == LiveStatusKind::CommandFailed && status.summary == "agent failed"
        }));
    }

    #[test]
    fn merge_and_trunk_result_markers_update_recovery_state() {
        let mut context = context_with_tasks();

        super::mark_task_merge_failed(&mut context, "web/fix-login", true).unwrap();
        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task.has_side_flag(SideFlag::Conflicted));
        assert!(task.live_status.as_ref().is_some_and(|status| {
            status.kind == LiveStatusKind::CommandFailed && status.summary == "merge failed"
        }));

        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::TmuxMissing,
            "tmux session missing",
        ));

        super::mark_task_trunk_repaired(&mut context, "web/fix-login").unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert!(task
            .tmux_status
            .as_ref()
            .is_some_and(|status| status.exists && status.session_name == "ajax-web-fix-login"));
        assert!(task.worktrunk_status.as_ref().is_some_and(|status| {
            status.exists
                && status.window_name == "worktrunk"
                && status.points_at_expected_path
                && status.current_path == task.worktree_path
        }));
        assert!(task.live_status.is_none());
    }

    #[test]
    fn force_remove_marks_task_removed_and_records_recovery_event() {
        let mut context = context_with_tasks();
        context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap()
            .add_side_flag(SideFlag::Stale);

        super::mark_task_force_removed(&mut context, "web/fix-login").unwrap();

        let task = context.registry.get_task(&TaskId::new("task-1")).unwrap();
        assert_eq!(task.lifecycle_status, LifecycleStatus::Removed);
        assert!(!task.has_side_flag(SideFlag::Stale));
        assert!(context
            .registry
            .events_for_task(&TaskId::new("task-1"))
            .iter()
            .any(|event| event.message == "lifecycle changed to Removed"));
    }

    #[test]
    fn trunk_task_plan_recreates_missing_tmux_session_with_worktrunk() {
        let context = context_with_tasks();

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        "ajax-web-fix-login",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn trunk_task_plan_switches_client_when_inside_tmux() {
        let context = context_with_tasks();

        let plan = super::trunk_task_plan_with_open_mode(
            &context,
            "web/fix-login",
            OpenMode::SwitchClient,
        )
        .unwrap();

        assert_eq!(
            plan.commands.last(),
            Some(
                &CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            )
        );
    }

    #[test]
    fn trunk_task_plan_repairs_worktrunk_when_tmux_session_exists() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: true,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/other".into(),
            points_at_expected_path: false,
        });

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    ["kill-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-window",
                        "-t",
                        "ajax-web-fix-login",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn trunk_task_plan_creates_missing_worktrunk_when_tmux_session_exists() {
        let mut context = context_with_tasks();
        let task = context
            .registry
            .get_task_mut(&TaskId::new("task-1"))
            .unwrap();
        task.tmux_status = Some(TmuxStatus {
            exists: true,
            session_name: "ajax-web-fix-login".to_string(),
        });
        task.worktrunk_status = Some(WorktrunkStatus {
            exists: false,
            window_name: "worktrunk".to_string(),
            current_path: "/tmp/worktrees/web-fix-login".into(),
            points_at_expected_path: false,
        });

        let plan = trunk_task_plan(&context, "web/fix-login").unwrap();

        assert_eq!(
            plan.commands,
            vec![
                CommandSpec::new(
                    "tmux",
                    [
                        "new-window",
                        "-t",
                        "ajax-web-fix-login",
                        "-n",
                        "worktrunk",
                        "-c",
                        "/tmp/worktrees/web-fix-login"
                    ]
                ),
                CommandSpec::new(
                    "tmux",
                    ["select-window", "-t", "ajax-web-fix-login:worktrunk"]
                ),
                CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                    .with_mode(CommandMode::InheritStdio)
            ]
        );
    }

    #[test]
    fn execute_plan_runs_safe_commands() {
        let context = context_with_tasks();
        let plan = open_task_plan(&context, "web/fix-login", OpenMode::Attach).unwrap();
        let mut runner = RecordingCommandRunner::default();

        let outputs = super::execute_plan(&plan, false, &mut runner).unwrap();

        assert_eq!(outputs.len(), 2);
        assert_eq!(runner.commands(), plan.commands.as_slice());
    }

    #[test]
    fn execute_plan_requires_confirmation_for_risky_commands() {
        let context = context_with_tasks();
        let plan = merge_task_plan(&context, "web/fix-login").unwrap();
        let mut runner = RecordingCommandRunner::default();

        let error = super::execute_plan(&plan, false, &mut runner).unwrap_err();

        assert_eq!(error, CommandError::ConfirmationRequired);
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn execute_plan_refuses_blocked_commands() {
        let mut runner = RecordingCommandRunner::default();
        let mut plan = super::CommandPlan::new("blocked");
        plan.blocked_reasons.push("worktree is missing".to_string());
        plan.commands.push(CommandSpec::new("git", ["status"]));

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::PlanBlocked(vec!["worktree is missing".to_string()])
        );
        assert!(runner.commands().is_empty());
    }

    #[test]
    fn execute_plan_rejects_nonzero_command_outputs() {
        let mut runner = QueuedRunner::new(vec![output(2, "nope")]);
        let mut plan = super::CommandPlan::new("failing");
        plan.commands
            .push(CommandSpec::new("git", ["merge", "ajax/fix-login"]));

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 2,
                stderr: String::new(),
                cwd: None,
            })
        );
    }

    #[test]
    fn execute_plan_reports_nonzero_command_cwd() {
        let mut runner = QueuedRunner::new(vec![output(1, "Error: Not in a git repository\n")]);
        let mut plan = super::CommandPlan::new("failing");
        plan.commands.push(
            CommandSpec::new("git", ["status"]).with_cwd("/Users/matt/Desktop/Projects/autodoctor"),
        );

        let error = super::execute_plan(&plan, true, &mut runner).unwrap_err();

        assert_eq!(
            error,
            CommandError::CommandRun(CommandRunError::NonZeroExit {
                program: "git".to_string(),
                status_code: 1,
                stderr: String::new(),
                cwd: Some("/Users/matt/Desktop/Projects/autodoctor".into()),
            })
        );
    }
}
