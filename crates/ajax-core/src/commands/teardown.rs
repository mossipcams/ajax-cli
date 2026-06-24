use super::{CommandContext, CommandError, CommandPlan};
use crate::{
    adapters::{CommandRunner, CommandSpec, GitAdapter, TmuxAdapter},
    lifecycle::force_mark_removed,
    models::{
        AgentRuntimeStatus, LifecycleStatus, SafetyClassification, SideFlag, StepReceipt,
        StepReceiptStatus, Task, TaskOperationKind, TmuxStatus, WorktrunkStatus,
    },
    operation::{task_operation_eligibility, OperationEligibility, TaskOperation},
    policy::cleanup_safety,
    registry::{Registry, RegistryError, RegistryEventKind},
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::lookup::{find_task, task_repo_path, update_task_lifecycle};

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
            .update_worktrunk_status(
                &task.id,
                Some(WorktrunkStatus {
                    exists: false,
                    window_name: task.worktrunk_window.clone(),
                    current_path: task.worktree_path.clone(),
                    points_at_expected_path: false,
                }),
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

pub fn drop_op_label(op: DropOp) -> &'static str {
    match op {
        DropOp::EnsureAgentStopped => "stop agent",
        DropOp::EnsureWorktreeAbsent => "remove worktree",
        DropOp::EnsureBranchAbsent => "delete branch",
        DropOp::EnsureTmuxSessionAbsent => "kill tmux session",
    }
}

pub fn format_drop_remaining_resources_detail(observation: &DropObservation) -> String {
    let mut remaining = Vec::new();
    if observation.agent == ResourceState::Present {
        remaining.push("agent still running");
    }
    if observation.tmux_session == ResourceState::Present {
        remaining.push("tmux session still present");
    }
    if observation.worktree == ResourceState::Present {
        remaining.push("worktree still present");
    }
    if observation.branch == ResourceState::Present {
        remaining.push("branch still present");
    }
    if remaining.is_empty() {
        "external resources still present after teardown attempt".to_string()
    } else {
        remaining.join(", ")
    }
}

pub fn format_drop_teardown_incomplete_message(
    task_handle: &str,
    failed_step: DropOp,
    detail: &str,
) -> String {
    let step = drop_op_label(failed_step);
    let detail = detail.trim();
    let core = if detail.is_empty() {
        format!("drop incomplete for {task_handle} at {step}")
    } else {
        format!("drop incomplete for {task_handle} at {step}: {detail}")
    };
    format!("{core}; retry with `ajax drop {task_handle} --execute`")
}

pub fn mark_task_teardown_incomplete<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
    failed_step: DropOp,
    observation: &DropObservation,
    failure_detail: Option<&str>,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    context
        .registry
        .update_lifecycle(&task_id, LifecycleStatus::TeardownIncomplete)
        .map_err(CommandError::Registry)?;
    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    task.metadata.insert(
        "drop_failed_step".to_string(),
        drop_op_label(failed_step).to_string(),
    );
    task.metadata.insert(
        "drop_failed_step_key".to_string(),
        drop_op_step_key(failed_step).to_string(),
    );
    if let Some(detail) = failure_detail
        .map(str::trim)
        .filter(|detail| !detail.is_empty())
    {
        task.metadata
            .insert("drop_failed_detail".to_string(), detail.to_string());
    }
    task.metadata.insert(
        "drop_latest_observation".to_string(),
        format!(
            "agent={:?};tmux={:?};worktree={:?};branch={:?}",
            observation.agent, observation.tmux_session, observation.worktree, observation.branch
        ),
    );
    let event_detail = failure_detail
        .map(str::trim)
        .filter(|detail| !detail.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format_drop_remaining_resources_detail(observation));
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::LifecycleChanged,
            format!(
                "drop teardown incomplete at {}: {event_detail}",
                drop_op_label(failed_step)
            ),
        )
        .map_err(CommandError::Registry)
}

pub fn mark_drop_agent_stopped<R: Registry>(
    context: &mut CommandContext<R>,
    qualified_handle: &str,
) -> Result<(), CommandError> {
    let task_id = find_task(context, qualified_handle)?.id.clone();
    let task = context
        .registry
        .get_task_mut(&task_id)
        .ok_or_else(|| CommandError::TaskNotFound(qualified_handle.to_string()))?;
    task.agent_status = AgentRuntimeStatus::Dead;
    task.remove_side_flag(SideFlag::AgentRunning);
    for attempt in &mut task.agent_attempts {
        if attempt.status == AgentRuntimeStatus::Running {
            attempt.status = AgentRuntimeStatus::Dead;
        }
    }
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::SubstrateChanged,
            "agent stopped",
        )
        .map_err(CommandError::Registry)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceState {
    Present,
    Absent,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropObservation {
    pub agent: ResourceState,
    pub tmux_session: ResourceState,
    pub worktree: ResourceState,
    pub branch: ResourceState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropOp {
    EnsureAgentStopped,
    EnsureTmuxSessionAbsent,
    EnsureWorktreeAbsent,
    EnsureBranchAbsent,
}

pub const DROP_TEARDOWN_ORDER: [DropOp; 4] = [
    DropOp::EnsureAgentStopped,
    DropOp::EnsureWorktreeAbsent,
    DropOp::EnsureBranchAbsent,
    DropOp::EnsureTmuxSessionAbsent,
];

impl DropOp {
    pub fn observed_state(self, observation: &DropObservation) -> ResourceState {
        match self {
            DropOp::EnsureAgentStopped => observation.agent,
            DropOp::EnsureWorktreeAbsent => observation.worktree,
            DropOp::EnsureBranchAbsent => observation.branch,
            DropOp::EnsureTmuxSessionAbsent => observation.tmux_session,
        }
    }

    pub fn step_key(self) -> &'static str {
        match self {
            DropOp::EnsureAgentStopped => "agent_stopped",
            DropOp::EnsureTmuxSessionAbsent => "tmux_session_absent",
            DropOp::EnsureWorktreeAbsent => "worktree_absent",
            DropOp::EnsureBranchAbsent => "branch_absent",
        }
    }

    pub fn records_observed_absent_receipt(self) -> bool {
        matches!(
            self,
            DropOp::EnsureTmuxSessionAbsent
                | DropOp::EnsureWorktreeAbsent
                | DropOp::EnsureBranchAbsent
        )
    }

    pub fn receipt_target(self, task: &Task) -> String {
        match self {
            DropOp::EnsureAgentStopped | DropOp::EnsureTmuxSessionAbsent => {
                task.tmux_session.clone()
            }
            DropOp::EnsureWorktreeAbsent => task.worktree_path.display().to_string(),
            DropOp::EnsureBranchAbsent => task.branch.clone(),
        }
    }
}

pub fn drop_op_step_key(op: DropOp) -> &'static str {
    op.step_key()
}

/// Tear down git resources before killing tmux so a failed drop can be retried while the
/// session is still attachable.
pub fn plan_drop_from_observation(observation: &DropObservation) -> Vec<DropOp> {
    DROP_TEARDOWN_ORDER
        .into_iter()
        .filter(|op| op.observed_state(observation) != ResourceState::Absent)
        .collect()
}

pub fn plan_drop_from_observation_for_task(
    observation: &DropObservation,
    receipts: &[StepReceipt],
) -> Vec<DropOp> {
    let completed = receipts
        .iter()
        .filter(|receipt| receipt.operation == TaskOperationKind::Drop)
        .filter(|receipt| {
            matches!(
                receipt.status,
                StepReceiptStatus::Succeeded | StepReceiptStatus::SkippedObserved
            )
        })
        .map(|receipt| receipt.step_key.as_str())
        .collect::<BTreeSet<_>>();

    plan_drop_from_observation(observation)
        .into_iter()
        .filter(|op| !completed.contains(op.step_key()))
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct RepoDropObservationCache {
    pub worktrees_output: Option<String>,
    pub branches_output: Option<String>,
}

pub fn observe_drop_resources<R: Registry>(
    context: &mut CommandContext<R>,
    task: &Task,
    runner: &mut impl CommandRunner,
) -> Result<DropObservation, CommandError> {
    observe_drop_resources_with_cache(
        context,
        task,
        runner,
        None,
        &mut RepoDropObservationCache::default(),
    )
}

pub fn observe_drop_resources_with_cache<R: Registry>(
    context: &mut CommandContext<R>,
    task: &Task,
    runner: &mut impl CommandRunner,
    shared_sessions_output: Option<&str>,
    repo_cache: &mut RepoDropObservationCache,
) -> Result<DropObservation, CommandError> {
    let repo_path = task_repo_path(context, task)
        .ok_or_else(|| CommandError::RepoNotFound(task.repo.clone()))?;
    let git = GitAdapter::new("git");
    let tmux = TmuxAdapter::new("tmux");
    let tmux_output = match shared_sessions_output {
        Some(output) => ObservationOutput::Output(output.to_string()),
        None => run_observation_command(runner, &tmux.list_sessions())?,
    };
    if repo_cache.worktrees_output.is_none() {
        repo_cache.worktrees_output =
            match run_observation_command(runner, &git.list_worktrees(&repo_path))? {
                ObservationOutput::Output(output) => Some(output),
                ObservationOutput::Unsupported | ObservationOutput::Unknown => None,
            };
    }
    if repo_cache.branches_output.is_none() {
        repo_cache.branches_output =
            match run_observation_command(runner, &git.list_branches(&repo_path))? {
                ObservationOutput::Output(output) => Some(output),
                ObservationOutput::Unsupported | ObservationOutput::Unknown => None,
            };
    }
    let worktrees_output = repo_cache
        .worktrees_output
        .as_ref()
        .map(|output| ObservationOutput::Output(output.clone()))
        .unwrap_or(ObservationOutput::Unknown);
    let branches_output = repo_cache
        .branches_output
        .as_ref()
        .map(|output| ObservationOutput::Output(output.clone()))
        .unwrap_or(ObservationOutput::Unknown);

    let tmux_session = match tmux_output {
        ObservationOutput::Output(ref output) => {
            if TmuxAdapter::parse_session_status(&task.tmux_session, output).exists {
                ResourceState::Present
            } else {
                ResourceState::Absent
            }
        }
        ObservationOutput::Unsupported | ObservationOutput::Unknown => ResourceState::Unknown,
    };

    let expected_worktree = task.worktree_path.display().to_string();
    let parsed_worktrees = match &worktrees_output {
        ObservationOutput::Output(output) => GitAdapter::parse_worktrees(output),
        ObservationOutput::Unsupported | ObservationOutput::Unknown => Vec::new(),
    };
    let observed_worktree = parsed_worktrees
        .iter()
        .find(|worktree| worktree.path == expected_worktree);
    let worktree = match worktrees_output {
        ObservationOutput::Output(_) => state_from_bool(observed_worktree.is_some()),
        ObservationOutput::Unsupported => task
            .git_status
            .as_ref()
            .map(|status| state_from_bool(status.worktree_exists))
            .unwrap_or(ResourceState::Unknown),
        ObservationOutput::Unknown => ResourceState::Unknown,
    };

    let parsed_branches = match &branches_output {
        ObservationOutput::Output(output) => GitAdapter::parse_branches(output),
        ObservationOutput::Unsupported | ObservationOutput::Unknown => Vec::new(),
    }
    .into_iter()
    .collect::<BTreeSet<_>>();
    let branch_seen_in_worktree = observed_worktree
        .and_then(|worktree| worktree.branch.as_ref())
        .is_some_and(|branch| branch == &task.branch);
    let branch = match branches_output {
        ObservationOutput::Output(_) => {
            state_from_bool(parsed_branches.contains(&task.branch) || branch_seen_in_worktree)
        }
        ObservationOutput::Unsupported => task
            .git_status
            .as_ref()
            .map(|status| state_from_bool(status.branch_exists))
            .unwrap_or(ResourceState::Unknown),
        ObservationOutput::Unknown if branch_seen_in_worktree => ResourceState::Present,
        ObservationOutput::Unknown => ResourceState::Unknown,
    };

    apply_drop_observation_evidence(context, task, tmux_session, worktree, branch)?;

    Ok(DropObservation {
        agent: observed_agent_state(task, tmux_session),
        tmux_session,
        worktree,
        branch,
    })
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

    if task
        .git_status
        .as_ref()
        .is_none_or(|status| status.worktree_exists)
    {
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
    if task
        .git_status
        .as_ref()
        .is_none_or(|status| status.branch_exists)
    {
        let needs_force = force
            || task
                .git_status
                .as_ref()
                .is_some_and(|status| !status.merged);
        let command = if needs_force {
            git.force_delete_branch(&repo_path, &task.branch)
        } else {
            git.delete_branch(&repo_path, &task.branch)
        };
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

enum ObservationOutput {
    Output(String),
    Unsupported,
    Unknown,
}

fn run_observation_command(
    runner: &mut impl CommandRunner,
    command: &CommandSpec,
) -> Result<ObservationOutput, CommandError> {
    let output = runner.run(command).map_err(CommandError::CommandRun)?;
    if output.status_code == 0 {
        Ok(ObservationOutput::Output(output.stdout))
    } else if output
        .stderr
        .to_ascii_lowercase()
        .contains("unexpected git command")
    {
        Ok(ObservationOutput::Unsupported)
    } else {
        Ok(ObservationOutput::Unknown)
    }
}

fn state_from_bool(value: bool) -> ResourceState {
    if value {
        ResourceState::Present
    } else {
        ResourceState::Absent
    }
}

fn observed_agent_state(task: &Task, tmux_session: ResourceState) -> ResourceState {
    if task.has_side_flag(SideFlag::AgentRunning)
        || task.agent_status == AgentRuntimeStatus::Running
        || task
            .agent_attempts
            .iter()
            .any(|attempt| attempt.status == AgentRuntimeStatus::Running)
    {
        return if tmux_session == ResourceState::Absent {
            ResourceState::Absent
        } else {
            ResourceState::Present
        };
    }

    ResourceState::Absent
}

fn apply_drop_observation_evidence<R: Registry>(
    context: &mut CommandContext<R>,
    task: &Task,
    tmux_session: ResourceState,
    worktree: ResourceState,
    branch: ResourceState,
) -> Result<(), CommandError> {
    if tmux_session != ResourceState::Unknown {
        context
            .registry
            .update_tmux_status(
                &task.id,
                Some(TmuxStatus {
                    exists: tmux_session == ResourceState::Present,
                    session_name: task.tmux_session.clone(),
                }),
            )
            .map_err(CommandError::Registry)?;
    }

    let previous_git = task.git_status.clone();
    if worktree != ResourceState::Unknown || branch != ResourceState::Unknown {
        let mut git_status = previous_git.unwrap_or(crate::models::GitStatus {
            worktree_exists: false,
            branch_exists: false,
            current_branch: None,
            dirty: false,
            ahead: 0,
            behind: 0,
            merged: matches!(
                task.lifecycle_status,
                LifecycleStatus::Merged | LifecycleStatus::Cleanable
            ),
            untracked_files: 0,
            unpushed_commits: 0,
            conflicted: false,
            last_commit: None,
        });
        if worktree != ResourceState::Unknown {
            git_status.worktree_exists = worktree == ResourceState::Present;
            if worktree == ResourceState::Absent {
                git_status.dirty = false;
                git_status.untracked_files = 0;
                git_status.conflicted = false;
                git_status.current_branch = None;
            }
        }
        if branch != ResourceState::Unknown {
            git_status.branch_exists = branch == ResourceState::Present;
            if branch == ResourceState::Absent {
                git_status.current_branch = None;
                git_status.ahead = 0;
                git_status.behind = 0;
                git_status.unpushed_commits = 0;
            }
        }
        context
            .registry
            .update_git_status(&task.id, git_status)
            .map_err(CommandError::Registry)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::mark_task_cleanup_step_completed;
    use super::*;
    use crate::{
        adapters::{CommandMode, CommandSpec},
        commands::CommandContext,
        config::{Config, ManagedRepo},
        models::{AgentClient, GitStatus, LifecycleStatus, Task, TaskId},
        registry::{InMemoryRegistry, Registry},
    };

    fn context_with_task() -> CommandContext<InMemoryRegistry> {
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let mut task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Cleanable;
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
        context.registry.create_task(task).unwrap();
        context
    }

    fn fast_worktree_remove_command() -> CommandSpec {
        CommandSpec {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                "mv \"$2\" \"$3\" && git -C \"$1\" worktree prune && { rm -rf \"$3\" >/dev/null 2>&1 & }"
                    .to_string(),
                "ajax-fast-worktree-remove".to_string(),
                "/repo/web".to_string(),
                "/repo/web__worktrees/ajax-fix-login".to_string(),
                "/repo/web__worktrees/.ajax-trash/fix-login-123".to_string(),
            ],
            cwd: None,
            mode: CommandMode::Capture,
            timeout: None,
        }
    }

    #[test]
    fn fast_worktree_remove_command_marks_worktree_cleanup_completed() {
        let mut context = context_with_task();
        let command = fast_worktree_remove_command();

        let updated =
            mark_task_cleanup_step_completed(&mut context, "web/fix-login", &command).unwrap();

        assert!(updated);
        let task = context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap();
        assert!(task
            .git_status
            .as_ref()
            .is_some_and(|status| !status.worktree_exists));
    }

    #[test]
    fn drop_resource_catalog_preserves_order_states_and_step_keys() {
        let observation = DropObservation {
            agent: ResourceState::Present,
            tmux_session: ResourceState::Absent,
            worktree: ResourceState::Unknown,
            branch: ResourceState::Present,
        };

        let ordered_ops = DROP_TEARDOWN_ORDER.to_vec();

        assert_eq!(
            ordered_ops,
            vec![
                DropOp::EnsureAgentStopped,
                DropOp::EnsureWorktreeAbsent,
                DropOp::EnsureBranchAbsent,
                DropOp::EnsureTmuxSessionAbsent,
            ]
        );
        assert_eq!(
            ordered_ops
                .iter()
                .map(|op| op.observed_state(&observation))
                .collect::<Vec<_>>(),
            vec![
                ResourceState::Present,
                ResourceState::Unknown,
                ResourceState::Present,
                ResourceState::Absent,
            ]
        );

        let step_keys = ordered_ops
            .iter()
            .map(|op| op.step_key())
            .collect::<Vec<_>>();
        assert_eq!(
            step_keys,
            vec![
                "agent_stopped",
                "worktree_absent",
                "branch_absent",
                "tmux_session_absent",
            ]
        );
        assert_eq!(step_keys.len(), BTreeSet::from_iter(step_keys).len());
    }
}
