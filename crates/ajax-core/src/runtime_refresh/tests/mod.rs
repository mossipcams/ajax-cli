#![allow(unused_imports)]
pub(super) use std::{
    cell::Cell,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) use super::{
    apply_github_checks_observation, clear_github_ci_evidence, github_probe_is_retired,
    refresh_runtime_context, refresh_runtime_context_with_tier, AgentStatusSource,
    NoAgentStatusSource, RefreshTier, CI_PROBE_ERROR_KEY, PRIMARY_RUN_ID,
};
pub(super) use crate::{
    adapters::{
        CiChecksObservation, CommandOutput, CommandRunError, CommandRunner, CommandSpec,
        GithubChecksAdapter,
    },
    agent_status::{
        ActivityKind, Confidence, ObservationSource, ProcessLiveness, StatusObservation,
    },
    commands::CommandContext,
    config::{Config, ManagedRepo, RuntimePathRequest},
    live::{LiveObservation, LiveStatusKind},
    models::{
        AgentClient, AgentRuntimeStatus, GitStatus, LifecycleStatus, RuntimeHealth,
        RuntimeObservationSource, RuntimeProjection, SideFlag, StepReceipt, Task, TaskId,
        TaskWindowStatus, TmuxStatus,
    },
    registry::{InMemoryRegistry, Registry, RegistryError, RegistryEvent, RegistryEventKind},
    ui_state::{
        agent_process_is_alive, derive_operator_status, TaskStatus, AGENT_PROCESS_ALIVE_KEY,
    },
};

pub(super) struct ObsSource {
    observations: Vec<StatusObservation>,
    liveness: Option<ProcessLiveness>,
}

impl ObsSource {
    fn new(observations: Vec<StatusObservation>) -> Self {
        Self {
            observations,
            liveness: None,
        }
    }

    fn with_liveness(mut self, liveness: ProcessLiveness) -> Self {
        self.liveness = Some(liveness);
        self
    }
}

/// Reducer-ready lifecycle observation `age_secs` old with a `ttl_secs`
/// freshness window, on the primary run.
pub(super) fn lifecycle_obs(kind: ActivityKind, age_secs: u64, ttl_secs: u64) -> StatusObservation {
    let observed_at = SystemTime::now() - Duration::from_secs(age_secs);
    StatusObservation {
        source: ObservationSource::ProviderLifecycle,
        observed_at,
        expires_at: observed_at + Duration::from_secs(ttl_secs),
        confidence: Confidence::High,
        run_id: PRIMARY_RUN_ID.to_string(),
        parent_run_id: None,
        kind,
    }
}

/// Confirmed wrapper exit `age_secs` old on the primary run.
pub(super) fn exit_obs(kind: ActivityKind, age_secs: u64) -> StatusObservation {
    let observed_at = SystemTime::now() - Duration::from_secs(age_secs);
    StatusObservation {
        source: ObservationSource::ProcessExit,
        observed_at,
        expires_at: observed_at + Duration::from_secs(120),
        confidence: Confidence::High,
        run_id: PRIMARY_RUN_ID.to_string(),
        parent_run_id: None,
        kind,
    }
}

pub(super) const BASE_BRANCH: &str = "main";
pub(super) const REPO_NAME: &str = "web";
pub(super) const REPO_PATH: &str = "/Users/matt/projects/web";
pub(super) const TASK_BRANCH: &str = "ajax/fix-login";
pub(super) const TASK_ID: &str = "task-1";
pub(super) const TASK_SESSION: &str = "ajax-web-fix-login";
pub(super) const TASK_WORKTREE: &str = "/tmp/worktrees/web-fix-login";
pub(super) const TASK_WINDOW: &str = "task";

impl AgentStatusSource for ObsSource {
    fn observations_for_task(&self, _task_id: &TaskId) -> Vec<StatusObservation> {
        self.observations.clone()
    }

    fn process_liveness_for_task(&self, _task_id: &TaskId) -> Option<ProcessLiveness> {
        self.liveness
    }
}

#[derive(Default)]
pub(super) struct RuntimeRefreshRunner;

impl CommandRunner for RuntimeRefreshRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        let stdout = runtime_stdout(&command.args);

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

pub(super) fn runtime_stdout(args: &[String]) -> &'static str {
    match arg(args, 0) {
        "list-sessions" => "ajax-web-fix-login\n",
        "-C" if git_worktree_list(args) => {
            "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\n"
        }
        "-C" if git_branch_list(args) => "main\najax/fix-login\n",
        "list-windows" => "ajax-web-fix-login\ttask\t/tmp/worktrees/web-fix-login\n",
        // Non-wait chrome: Working reconcile captures for Claude/Codex/Cursor;
        // idle composer here would falsely upgrade Working → Waiting.
        "capture-pane" => "{\"type\":\"thinking\"}\n",
        _ => "",
    }
}

pub(super) fn arg(args: &[String], index: usize) -> &str {
    args.get(index).map(String::as_str).unwrap_or_default()
}

pub(super) fn git_worktree_list(args: &[String]) -> bool {
    arg(args, 1) == REPO_PATH
        && arg(args, 2) == "worktree"
        && arg(args, 3) == "list"
        && arg(args, 4) == "--porcelain"
}

pub(super) fn git_branch_list(args: &[String]) -> bool {
    arg(args, 1) == REPO_PATH
        && arg(args, 2) == "branch"
        && arg(args, 3) == "--format=%(refname:short)"
}

pub(super) fn context_with_active_task() -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = task_fixture();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(clean_git_status());
    registry.create_task(task).unwrap();

    CommandContext::new(config, registry)
}

pub(super) fn task_fixture() -> Task {
    Task::new(
        TaskId::new(TASK_ID),
        REPO_NAME,
        "fix-login",
        "Fix login",
        TASK_BRANCH,
        BASE_BRANCH,
        TASK_WORKTREE,
        TASK_SESSION,
        TASK_WINDOW,
        AgentClient::Codex,
    )
}

pub(super) fn clean_git_status() -> GitStatus {
    GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some(TASK_BRANCH.to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    }
}

#[derive(Default)]
pub(super) struct HealthyRefreshRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for HealthyRefreshRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "capture-pane" => "{\"type\":\"thinking\"}\n",
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Default)]
pub(super) struct CountingRegistry {
    inner: InMemoryRegistry,
    list_tasks_calls: Cell<u32>,
    get_task_calls: Cell<u32>,
    task_window_status_updates: Cell<u32>,
}

impl CountingRegistry {
    fn from_registry(inner: InMemoryRegistry) -> Self {
        Self {
            inner,
            list_tasks_calls: Cell::new(0),
            get_task_calls: Cell::new(0),
            task_window_status_updates: Cell::new(0),
        }
    }

    fn list_tasks_calls(&self) -> u32 {
        self.list_tasks_calls.get()
    }

    fn get_task_calls(&self) -> u32 {
        self.get_task_calls.get()
    }

    fn task_window_status_updates(&self) -> u32 {
        self.task_window_status_updates.get()
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
        self.get_task_calls.set(self.get_task_calls.get() + 1);
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

    fn update_task_window_status(
        &mut self,
        task_id: &TaskId,
        status: Option<TaskWindowStatus>,
    ) -> Result<(), RegistryError> {
        self.task_window_status_updates
            .set(self.task_window_status_updates.get() + 1);
        self.inner.update_task_window_status(task_id, status)
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

pub(super) fn context_with_unchanged_running_task() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_active_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.agent_status = AgentRuntimeStatus::Running;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::AgentRunning,
        "agent running",
    ));
    task.add_side_flag(SideFlag::AgentRunning);
    task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
    task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    task.last_activity_at = SystemTime::UNIX_EPOCH + Duration::from_secs(2);
    context
}

pub(super) fn seed_fresh_ci_probe<R: Registry>(context: &mut CommandContext<R>) {
    let now = unix_seconds_for_test(SystemTime::now()).to_string();
    let task_ids = context
        .registry
        .list_tasks()
        .into_iter()
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    for task_id in task_ids {
        context
            .registry
            .get_task_mut(&task_id)
            .unwrap()
            .metadata
            .insert("ci_checks_probed_at".to_string(), now.clone());
    }
}

pub(super) fn context_with_task_for_missing_session() -> CommandContext<CountingRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new(REPO_NAME, REPO_PATH, BASE_BRANCH)],
        ..Config::default()
    };
    let mut registry = InMemoryRegistry::default();
    let mut task = task_fixture();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(clean_git_status());
    task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
    task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
    registry.create_task(task).unwrap();

    CommandContext::new(config, CountingRegistry::from_registry(registry))
}

pub(super) fn context_with_teardown_incomplete_task() -> CommandContext<CountingRegistry> {
    let mut context = context_with_task_for_missing_session();
    let task = context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::TeardownIncomplete;
    task.live_status = Some(LiveObservation::new(
        LiveStatusKind::CommandFailed,
        "drop incomplete at delete branch",
    ));
    task.metadata
        .insert("drop_failed_step".to_string(), "delete branch".to_string());
    task.metadata.insert(
        "drop_failed_detail".to_string(),
        "branch still present".to_string(),
    );
    context
}

#[derive(Default)]
pub(super) struct MissingSessionRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for MissingSessionRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => "ajax-other-task\n",
            [command, ..] if command == "list-windows" => {
                "ajax-other-task\ttask\t/tmp/worktrees/web-other-task\n"
            }
            [command, ..] if command == "capture-pane" => "",
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Default)]
pub(super) struct OrphanRecoveryRunner {
    commands: Vec<CommandSpec>,
    sessions_output: Option<String>,
}

impl CommandRunner for OrphanRecoveryRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "list-sessions" => self
                .sessions_output
                .as_deref()
                .unwrap_or("ajax-web-fix-login\n"),
            [_, repo, subcommand, action, flag]
                if repo == REPO_PATH
                    && subcommand == "worktree"
                    && action == "list"
                    && flag == "--porcelain" =>
            {
                "worktree /Users/matt/projects/web\nHEAD 1111111\nbranch refs/heads/main\n\nworktree /tmp/worktrees/web-fix-login\nHEAD 2222222\nbranch refs/heads/ajax/fix-login\n\nworktree /tmp/worktrees/web-a\nHEAD 3333333\nbranch refs/heads/ajax/a\n\nworktree /tmp/worktrees/web-b\nHEAD 4444444\nbranch refs/heads/ajax/b\n\nworktree /tmp/worktrees/web-c\nHEAD 5555555\nbranch refs/heads/ajax/c\n\n"
            }
            [_, repo, subcommand, format]
                if repo == REPO_PATH
                    && subcommand == "branch"
                    && format == "--format=%(refname:short)" =>
            {
                "main\najax/fix-login\najax/a\najax/b\najax/c\n"
            }
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

mod suite_1;
mod suite_2;
mod suite_3;

#[derive(Default)]
pub(super) struct GitSkippingRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for GitSkippingRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "capture-pane" => "{\"type\":\"thinking\"}\n",
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}

#[derive(Default)]
pub(super) struct CiChecksRunner {
    commands: Vec<CommandSpec>,
    gh_stdout: String,
    gh_stderr: String,
    gh_status: i32,
}

impl CiChecksRunner {
    fn with_gh(stdout: &str, stderr: &str, status_code: i32) -> Self {
        Self {
            gh_stdout: stdout.to_string(),
            gh_stderr: stderr.to_string(),
            gh_status: status_code,
            ..Default::default()
        }
    }

    fn gh_command_count(&self) -> usize {
        self.commands
            .iter()
            .filter(|command| command.program == "gh")
            .count()
    }
}

impl CommandRunner for CiChecksRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        if command.program == "gh" {
            return Ok(CommandOutput {
                status_code: self.gh_status,
                stdout: self.gh_stdout.clone(),
                stderr: self.gh_stderr.clone(),
            });
        }

        Ok(CommandOutput {
            status_code: 0,
            stdout: runtime_stdout(&command.args).to_string(),
            stderr: String::new(),
        })
    }
}

pub(super) fn ci_failed_stdout(check_name: &str) -> String {
    format!(r#"[{{"name":"{check_name}","state":"FAILURE","link":"x"}}]"#)
}

pub(super) fn unix_seconds_for_test(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn task_with_live(kind: LiveStatusKind, summary: &str) -> Task {
    let mut task = task_fixture();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(clean_git_status());
    task.tmux_status = Some(TmuxStatus::present(TASK_SESSION));
    task.task_window_status = Some(TaskWindowStatus::present(TASK_WINDOW, TASK_WORKTREE));
    task.runtime_projection = RuntimeProjection::new(
        RuntimeHealth::Healthy,
        SystemTime::now(),
        RuntimeObservationSource::TmuxProbe,
    );
    task.live_status = Some(LiveObservation::new(kind, summary));
    task
}

pub(super) const CLAUDE_PERMISSION_MENU: &str =
    "Do you want to proceed?\n\n❯ 1. Yes\n  2. No\n\nEsc to cancel";

#[derive(Default)]
pub(super) struct PermissionMenuRunner {
    commands: Vec<CommandSpec>,
}

impl CommandRunner for PermissionMenuRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        self.commands.push(command.clone());
        let stdout = match command.args.as_slice() {
            [command, ..] if command == "capture-pane" => CLAUDE_PERMISSION_MENU,
            _ => runtime_stdout(&command.args),
        };

        Ok(CommandOutput {
            status_code: 0,
            stdout: stdout.to_string(),
            stderr: String::new(),
        })
    }
}
