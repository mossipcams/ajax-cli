use super::{format_execution_outputs, operate, operate_error_code, OperateError, OperateRequest};
use ajax_core::remediation;
use ajax_core::{
    adapters::{CommandOutput, RecordingCommandRunner},
    commands::{CommandContext, CommandError},
    config::{Config, ManagedRepo},
    models::{
        GitStatus, LifecycleStatus, LiveObservation, LiveStatusKind, SideFlag, TaskId, TmuxStatus,
    },
    registry::{InMemoryRegistry, Registry as _},
};

fn context_with_reviewable_task() -> CommandContext<InMemoryRegistry> {
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    crate::test_support::context_with_tasks(&["web"], vec![task])
}

#[test]
fn operate_error_code_maps_known_operate_errors() {
    assert_eq!(
        operate_error_code(&OperateError::UnknownAction("nope".to_string())),
        "unknown_action"
    );
    assert_eq!(
        operate_error_code(&OperateError::UnsupportedCapability(
            "attach requires a terminal"
        )),
        "needs_terminal"
    );
    assert_eq!(
        operate_error_code(&OperateError::UnsupportedCapability("unsupported agent")),
        "unsupported_action"
    );
    assert_eq!(
        operate_error_code(&OperateError::Command(
            CommandError::TaskNotFound("web/missing".to_string()),
            false
        )),
        "task_not_found"
    );
    assert_eq!(
        operate_error_code(&OperateError::Command(
            CommandError::ConfirmationRequired,
            false
        )),
        "confirmation_required"
    );
    assert_eq!(
        operate_error_code(&OperateError::Command(
            CommandError::PlanBlocked(vec!["blocked".to_string()]),
            false
        )),
        "conflict"
    );
    assert_eq!(
        operate_error_code(&OperateError::Command(
            CommandError::RepoNotFound("missing".to_string()),
            false
        )),
        "command_failed"
    );
}

#[test]
fn operate_slice_delegates_resume_to_core_operation_without_attach() {
    let mut context = context_with_reviewable_task();
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap()
        .git_status = Some(GitStatus {
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
    let mut runner = RecordingCommandRunner::default();
    let outcome = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "resume".to_string(),
            confirmed: false,
            branch_adoption: None,
        },
    )
    .unwrap();

    assert!(outcome.state_changed);
    assert!(
        runner.commands().is_empty(),
        "browser resume should acknowledge the open without probing Git"
    );
    assert!(
        !runner
            .commands()
            .iter()
            .any(|command| command.mode == ajax_core::adapters::CommandMode::InheritStdio),
        "resume must not attach to the task terminal"
    );
}

#[test]
fn operate_slice_repair_recreated_worktree_is_marked_present() {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.git_status = Some(GitStatus {
        worktree_exists: false,
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
    task.add_side_flag(SideFlag::WorktreeMissing);
    let mut runner = RecordingCommandRunner::default();

    operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: false,
            branch_adoption: None,
        },
    )
    .unwrap();

    assert!(runner.commands().iter().any(|command| {
        command
            == &ajax_core::adapters::CommandSpec::new(
                "git",
                [
                    "-C",
                    "/repo/web",
                    "worktree",
                    "add",
                    "/repo/web__worktrees/ajax-fix-login",
                    "ajax/fix-login",
                ],
            )
    }));
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert!(task
        .git_status
        .as_ref()
        .is_some_and(|status| status.worktree_exists));
    assert!(!task.has_side_flag(SideFlag::WorktreeMissing));
}

#[test]
fn operate_slice_delegates_review_to_core_operation_and_returns_output() {
    let mut context = context_with_reviewable_task();
    let mut runner = RecordingCommandRunner::default();
    let outcome = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "review".to_string(),
            confirmed: false,
            branch_adoption: None,
        },
    )
    .unwrap();

    assert!(!outcome.state_changed);
    assert!(outcome.output.is_empty());
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn operate_slice_runs_fix_ci_remediation_via_tmux() {
    let mut task = crate::test_support::fix_login_task();
    task.live_status = Some(LiveObservation::new(LiveStatusKind::CiFailed, "ci failed"));
    task.tmux_status = Some(TmuxStatus {
        exists: true,
        session_name: task.tmux_session.clone(),
    });
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = RecordingCommandRunner::default();

    let home = std::env::temp_dir().join(format!("ajax-skill-{}", std::process::id()));
    let skill_dir = home.join("gh-fix-ci");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
    std::env::set_var("AJAX_SKILL_ROOT", &home);

    let outcome = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: remediation::FIX_CI.to_string(),
            confirmed: false,
            branch_adoption: None,
        },
    )
    .unwrap();

    std::env::remove_var("AJAX_SKILL_ROOT");
    let _ = std::fs::remove_dir_all(&home);

    assert!(!outcome.state_changed);
    assert!(outcome.output.contains("Fix CI"));
    assert_eq!(runner.commands().len(), 1);
}

#[test]
fn format_execution_outputs_prefers_stdout() {
    let text = format_execution_outputs(&[CommandOutput {
        status_code: 0,
        stdout: " diff stat\n".to_string(),
        stderr: String::new(),
    }]);

    assert_eq!(text, "diff stat");
}

fn agent_send_keys_line(commands: &[ajax_core::adapters::CommandSpec]) -> &str {
    commands
        .iter()
        .find(|command| {
            command.program == "tmux" && command.args.first() == Some(&"send-keys".to_string())
        })
        .map(|command| command.args[3].as_str())
        .expect("expected tmux send-keys command")
}

#[test]
fn start_task_cursor_agent_command_uses_agent_subcommand_without_cd() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "cursor".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap();

    let line = agent_send_keys_line(runner.commands());
    assert_eq!(
        line,
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- cursor agent"
    );
    assert!(!line.contains("--cd"));
}

#[test]
fn start_task_pi_agent_command_runs_pi_in_task_window() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "pi".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap();

    // pi opens in the current directory; the task window's cwd is
    // the worktree, so the launch needs no extra arguments.
    assert_eq!(
        agent_send_keys_line(runner.commands()),
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- pi"
    );
}

#[test]
fn start_task_claude_agent_command_omits_cd_flag_and_skips_permissions() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "claude".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap();

    assert_eq!(
        agent_send_keys_line(runner.commands()),
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- claude --dangerously-skip-permissions"
    );
}

#[test]
fn start_task_creates_a_new_task_in_the_registry() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let outcome = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap();

    assert!(outcome.state_changed);
    let tasks = context.registry.list_tasks();
    assert!(
        tasks
            .iter()
            .any(|task| task.qualified_handle() == "web/fix-login"),
        "expected new task in registry, got {:?}",
        tasks
            .iter()
            .map(|t| t.qualified_handle())
            .collect::<Vec<_>>()
    );
}

#[test]
fn start_task_rejects_empty_title() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let error = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "   ".to_string(),
            agent: "codex".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        OperateError::UnsupportedCapability("start requires a non-empty task title")
    );
    assert!(runner.commands().is_empty());
    assert!(context.registry.list_tasks().is_empty());
}

#[test]
fn start_task_rejects_unsupported_agent() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let error = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "/bin/sh".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        OperateError::UnsupportedCapability("unsupported agent")
    );
    assert!(runner.commands().is_empty());
    assert!(context.registry.list_tasks().is_empty());
}

#[test]
fn start_task_surfaces_unknown_repo_as_command_error() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let error = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "missing".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap_err();

    assert!(
        matches!(error, OperateError::Command(_, false)),
        "{error:?}"
    );
    assert!(runner.commands().is_empty());
}

#[test]
fn start_task_skips_fetch_when_origin_fetch_is_fresh() {
    let root = std::env::temp_dir().join(format!(
        "ajax-web-start-task-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join(".git")).unwrap();
    let mut file = std::fs::File::create(root.join(".git/FETCH_HEAD")).unwrap();
    use std::io::Write;
    writeln!(file, "ref: origin/main").unwrap();
    let mut context = context_with_repo_path(&root);
    let mut runner = RecordingCommandRunner::default();

    super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            request_id: String::new(),
        },
    )
    .unwrap();

    assert!(
        runner
            .commands()
            .iter()
            .all(|command| !command.args.iter().any(|arg| arg == "fetch")),
        "unexpected fetch command: {:?}",
        runner.commands()
    );
    let _ = std::fs::remove_dir_all(root);
}

fn context_with_managed_repo() -> CommandContext<InMemoryRegistry> {
    crate::test_support::context_with_tasks(&["web"], vec![])
}

fn context_with_repo_path(repo_path: &std::path::Path) -> CommandContext<InMemoryRegistry> {
    let config = Config {
        repos: vec![ManagedRepo::new(
            "web",
            repo_path.display().to_string(),
            "main",
        )],
        ..Config::default()
    };
    CommandContext::new(config, InMemoryRegistry::default())
}

fn context_with_named_checkout_mismatch() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: true,
        current_branch: Some("fix/pane-stuck".to_string()),
        dirty: false,
        ahead: 0,
        behind: 0,
        merged: false,
        untracked_files: 0,
        unpushed_commits: 0,
        conflicted: false,
        last_commit: None,
    });
    context
}

struct QueuedRefreshRunner {
    outputs: std::collections::VecDeque<CommandOutput>,
    commands: Vec<ajax_core::adapters::CommandSpec>,
}

impl QueuedRefreshRunner {
    fn new(outputs: Vec<CommandOutput>) -> Self {
        Self {
            outputs: outputs.into(),
            commands: Vec::new(),
        }
    }
}

impl ajax_core::adapters::CommandRunner for QueuedRefreshRunner {
    fn run(
        &mut self,
        command: &ajax_core::adapters::CommandSpec,
    ) -> Result<CommandOutput, ajax_core::adapters::CommandRunError> {
        self.commands.push(command.clone());
        self.outputs
            .pop_front()
            .ok_or(ajax_core::adapters::CommandRunError::SpawnFailed(
                "queued refresh runner exhausted".to_string(),
            ))
    }
}

fn mismatch_refresh_outputs(observed_branch: &str) -> Vec<CommandOutput> {
    vec![
            CommandOutput {
                status_code: 0,
                stdout: format!(
                    "worktree /repo/web__worktrees/ajax-fix-login\nHEAD 2222222\nbranch refs/heads/{observed_branch}\n\n"
                ),
                stderr: String::new(),
            },
            CommandOutput {
                status_code: 0,
                stdout: "main\najax/fix-login\n".to_string(),
                stderr: String::new(),
            },
        ]
}

fn is_git_substrate_observation(command: &ajax_core::adapters::CommandSpec) -> bool {
    command.program == "git"
        && (command
            .args
            .windows(2)
            .any(|window| window == ["worktree", "list"])
            || command
                .args
                .windows(2)
                .any(|window| window == ["branch", "--format=%(refname:short)"]))
}

fn assert_only_git_substrate_observations(commands: &[ajax_core::adapters::CommandSpec]) {
    assert_eq!(
        commands.len(),
        2,
        "expected only git substrate observations"
    );
    assert!(commands.iter().all(is_git_substrate_observation));
    assert!(!commands.iter().any(|command| {
        command.args.iter().any(|arg| {
            arg == "switch"
                || arg == "checkout"
                || arg == "worktree" && command.args.contains(&"add".to_string())
        }) || command.program == "tmux"
            || command.program == "ajax-cli"
    }));
}

#[test]
fn operate_slice_mismatch_repair_requires_typed_confirmation() {
    let mut context = context_with_named_checkout_mismatch();
    let branch_before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .branch
        .clone();
    let events_before = context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .len();
    let mut runner = QueuedRefreshRunner::new(mismatch_refresh_outputs("fix/pane-stuck"));

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: true,
            branch_adoption: None,
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        OperateError::Command(
            ajax_core::commands::CommandError::ConfirmationRequired,
            false
        )
    ));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .branch,
        branch_before
    );
    assert_eq!(
        context
            .registry
            .events_for_task(&TaskId::new("web/fix-login"))
            .len(),
        events_before
    );
    assert_only_git_substrate_observations(runner.commands.as_slice());
}

#[test]
fn operate_slice_confirmed_mismatch_repair_adopts_requested_pair_without_mutation_commands() {
    let mut context = context_with_named_checkout_mismatch();
    let task_before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .clone();
    let mut runner = QueuedRefreshRunner::new(mismatch_refresh_outputs("fix/pane-stuck"));

    let outcome = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: true,
            branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                expected_branch: "ajax/fix-login".to_string(),
                observed_branch: "fix/pane-stuck".to_string(),
            }),
        },
    )
    .unwrap();

    assert!(outcome.state_changed);
    assert!(outcome.output.is_empty());
    let task_after = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(task_after.branch, "fix/pane-stuck");
    assert_eq!(task_after.id, task_before.id);
    assert_eq!(task_after.worktree_path, task_before.worktree_path);
    assert_eq!(task_after.tmux_session, task_before.tmux_session);
    assert!(!task_after.has_checkout_mismatch());
    assert_only_git_substrate_observations(runner.commands.as_slice());
}

#[test]
fn operate_slice_stale_mismatch_confirmation_rejects_changed_checkout() {
    const STALE_REASON: &str = "checkout changed since repair was planned; refresh and retry";
    let mut context = context_with_named_checkout_mismatch();
    context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap()
        .git_status
        .as_mut()
        .unwrap()
        .current_branch = Some("other/branch".to_string());
    let branch_before = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap()
        .branch
        .clone();
    let events_before = context
        .registry
        .events_for_task(&TaskId::new("web/fix-login"))
        .len();
    let mut runner = QueuedRefreshRunner::new(mismatch_refresh_outputs("other/branch"));

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: true,
            branch_adoption: Some(ajax_core::commands::BranchAdoptionPlan {
                expected_branch: "ajax/fix-login".to_string(),
                observed_branch: "fix/pane-stuck".to_string(),
            }),
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        OperateError::Command(ajax_core::commands::CommandError::PlanBlocked(reasons), false)
        if reasons == [STALE_REASON.to_string()]
    ));
    assert_eq!(
        context
            .registry
            .get_task(&TaskId::new("web/fix-login"))
            .unwrap()
            .branch,
        branch_before
    );
    assert_eq!(
        context
            .registry
            .events_for_task(&TaskId::new("web/fix-login"))
            .len(),
        events_before
    );
    assert_only_git_substrate_observations(runner.commands.as_slice());
}
