use super::{CommandContext, CommandError};
use crate::{
    adapters::{CommandRunner, CommandSpec, GitAdapter, TmuxAdapter},
    models::{
        sync_open_attempts, AgentRuntimeStatus, LifecycleStatus, SideFlag, StepReceipt,
        StepReceiptStatus, Task, TaskOperationKind, TmuxStatus,
    },
    registry::{Registry, RegistryEventKind},
};
use std::{collections::BTreeSet, path::Path};

use crate::commands::lookup::{find_task, task_repo_path};

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
    sync_open_attempts(task, std::time::SystemTime::now());
    context
        .registry
        .record_event(
            task_id,
            RegistryEventKind::SubstrateChanged,
            "agent stopped",
        )
        .map_err(CommandError::Registry)
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
    pub remote_branches_output: Option<String>,
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
    if repo_cache.remote_branches_output.is_none() {
        repo_cache.remote_branches_output =
            match run_observation_command(runner, &git.list_remote_branches(&repo_path))? {
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
    let remote_branches_output = repo_cache
        .remote_branches_output
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

    let parsed_worktrees = match &worktrees_output {
        ObservationOutput::Output(output) => GitAdapter::parse_worktrees(output),
        ObservationOutput::Unsupported | ObservationOutput::Unknown => Vec::new(),
    };
    let path_matched_worktree = parsed_worktrees
        .iter()
        .find(|worktree| Path::new(&worktree.path) == task.worktree_path.as_path());
    let worktree = match worktrees_output {
        ObservationOutput::Output(_) => state_from_bool(path_matched_worktree.is_some()),
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
    let parsed_remote_branches = match &remote_branches_output {
        ObservationOutput::Output(output) => GitAdapter::parse_remote_branches(output),
        ObservationOutput::Unsupported | ObservationOutput::Unknown => Vec::new(),
    }
    .into_iter()
    .collect::<BTreeSet<_>>();
    let branch_seen_in_worktree = path_matched_worktree
        .and_then(|worktree| worktree.branch.as_ref())
        .is_some_and(|branch| branch == &task.branch);
    let branch_present = parsed_branches.contains(&task.branch)
        || parsed_remote_branches.contains(&task.branch)
        || branch_seen_in_worktree;
    let branch = match (branches_output, remote_branches_output) {
        (ObservationOutput::Output(_), _) | (_, ObservationOutput::Output(_)) => {
            state_from_bool(branch_present)
        }
        (ObservationOutput::Unsupported, ObservationOutput::Unsupported) => task
            .git_status
            .as_ref()
            .map(|status| state_from_bool(status.branch_exists))
            .unwrap_or(ResourceState::Unknown),
        (ObservationOutput::Unknown, ObservationOutput::Unknown) if branch_seen_in_worktree => {
            ResourceState::Present
        }
        (ObservationOutput::Unknown, ObservationOutput::Unknown) => ResourceState::Unknown,
        (ObservationOutput::Unsupported, ObservationOutput::Unknown)
        | (ObservationOutput::Unknown, ObservationOutput::Unsupported) => task
            .git_status
            .as_ref()
            .map(|status| state_from_bool(status.branch_exists))
            .unwrap_or(ResourceState::Unknown),
    };

    apply_drop_observation_evidence(context, task, tmux_session, worktree, branch)?;

    Ok(DropObservation {
        agent: observed_agent_state(task, tmux_session),
        tmux_session,
        worktree,
        branch,
    })
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
