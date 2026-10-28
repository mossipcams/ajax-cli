use super::{format_execution_outputs, operate, operate_error_code, OperateError, OperateRequest};
use ajax_core::remediation;
use ajax_core::{
    adapters::{
        CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
    },
    commands::{CommandContext, CommandError},
    config::{Config, ManagedRepo},
    diff_review::{remember_pull_requests, stored_pull_requests, PullRequestRef, PullRequestState},
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
fn operate_slice_ship_with_unpushed_commits_runs_once_confirmed() {
    let mut task = crate::test_support::fix_login_task();
    task.lifecycle_status = LifecycleStatus::Reviewable;
    task.add_side_flag(SideFlag::Unpushed);
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = RecordingCommandRunner::default();

    let outcome = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "ship".to_string(),
            confirmed: true,
        },
    )
    .unwrap();

    assert!(outcome.state_changed);
    assert!(!runner.commands().is_empty());
}

fn open_pull_request(number: u64) -> PullRequestRef {
    PullRequestRef {
        number,
        title: "feat(web): merge".to_string(),
        url: format!("https://github.com/mossipcams/ajax-cli/pull/{number}"),
        state: PullRequestState::Open,
        head_ref: "ajax/fix-login".to_string(),
        head_sha: None,
    }
}

fn merge_pr_request(confirmed: bool) -> OperateRequest {
    OperateRequest {
        task_handle: "web/fix-login".to_string(),
        action: remediation::MERGE_PR.to_string(),
        confirmed,
    }
}

#[test]
fn operate_slice_merge_pr_runs_squash_merge_once_and_records_merged() {
    let mut task = crate::test_support::fix_login_task();
    remember_pull_requests(&mut task, &[open_pull_request(7)]);
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = RecordingCommandRunner::default();

    operate(&mut context, &mut runner, merge_pr_request(true)).unwrap();

    assert_eq!(runner.commands().len(), 1);
    assert_eq!(runner.commands()[0].program, "gh");
    assert_eq!(
        runner.commands()[0].args,
        ["pr", "merge", "7", "--squash"].map(String::from)
    );
    let task = context
        .registry
        .get_task(&TaskId::new("web/fix-login"))
        .unwrap();
    assert_eq!(
        stored_pull_requests(task),
        vec![PullRequestRef {
            state: PullRequestState::Merged,
            ..open_pull_request(7)
        }]
    );
}

#[test]
fn operate_slice_merge_pr_without_confirmation_runs_nothing() {
    let mut task = crate::test_support::fix_login_task();
    remember_pull_requests(&mut task, &[open_pull_request(7)]);
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = RecordingCommandRunner::default();

    let error = operate(&mut context, &mut runner, merge_pr_request(false)).unwrap_err();

    assert_eq!(operate_error_code(&error), "confirmation_required");
    assert!(runner.commands().is_empty());
}

#[test]
fn operate_slice_rejects_repair_as_unsupported_capability() {
    let mut context = context_with_reviewable_task();
    let mut runner = RecordingCommandRunner::default();

    let error = operate(
        &mut context,
        &mut runner,
        OperateRequest {
            task_handle: "web/fix-login".to_string(),
            action: "repair".to_string(),
            confirmed: false,
        },
    )
    .unwrap_err();

    assert!(matches!(error, OperateError::UnsupportedCapability(_)));
    assert!(runner.commands().is_empty());
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
            orchestration_chat: false,
            model: None,
        },
    )
    .unwrap();

    let line = agent_send_keys_line(runner.commands());
    assert_eq!(
        line,
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- cursor agent --model cursor-grok-4.6-high"
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
            orchestration_chat: false,
            model: None,
        },
    )
    .unwrap();

    assert_eq!(
        agent_send_keys_line(runner.commands()),
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- pi"
    );
}

#[test]
fn start_task_cursor_agent_command_uses_selected_model() {
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
            orchestration_chat: false,
            model: Some("composer-2.5".to_string()),
        },
    )
    .unwrap();

    let line = agent_send_keys_line(runner.commands());
    assert!(
        line.ends_with("cursor agent --model composer-2.5"),
        "{line}"
    );
}

#[test]
fn start_task_stores_the_model_for_a_bridge_harness() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            request_id: String::new(),
            orchestration_chat: true,
            model: Some("gpt-5.6-sol[high]".to_string()),
        },
    )
    .unwrap();

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("task created");
    assert_eq!(task.session_model(), Some("gpt-5.6-sol[high]"));
}

#[test]
fn start_task_rejects_model_for_agent_without_acp() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let error = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "aider".to_string(),
            request_id: String::new(),
            orchestration_chat: false,
            model: Some("composer-2.5".to_string()),
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        super::OperateError::UnsupportedCapability("unsupported agent")
    );
}

#[test]
fn start_task_rejects_shell_bearing_model_id() {
    let mut context = context_with_managed_repo();
    let mut runner = RecordingCommandRunner::default();

    let error = super::start_task(
        &mut context,
        &mut runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "cursor".to_string(),
            request_id: String::new(),
            orchestration_chat: false,
            model: Some("auto; rm -rf /".to_string()),
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        super::OperateError::UnsupportedCapability("unsupported model")
    );
    assert!(runner.commands().is_empty());
}

#[test]
fn set_task_session_model_persists_on_a_provisioned_task() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);

    super::set_task_session_model(&mut context, "web/fix-login", "claude-opus-5").unwrap();

    let stored = context
        .registry
        .get_task(&ajax_core::models::TaskId::new("web/fix-login"))
        .expect("task");
    assert_eq!(stored.session_model(), Some("claude-opus-5"));
}

#[test]
fn set_task_session_model_persists_none_for_auto() {
    let mut task = crate::test_support::fix_login_task();
    task.set_skip_interactive_agent(true);
    task.set_session_model(Some("claude-opus-5"));
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);

    super::set_task_session_model(&mut context, "web/fix-login", "auto").unwrap();

    let stored = context
        .registry
        .get_task(&ajax_core::models::TaskId::new("web/fix-login"))
        .expect("task");
    assert_eq!(stored.session_model(), None);
}

#[test]
fn start_task_persists_none_for_auto_model() {
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
            orchestration_chat: true,
            model: Some("auto".to_string()),
        },
    )
    .unwrap();

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("task created");
    assert_eq!(task.session_model(), None);
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
            orchestration_chat: false,
            model: None,
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
            orchestration_chat: false,
            model: None,
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
            orchestration_chat: false,
            model: None,
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
fn start_task_orchestration_chat_skips_send_keys_for_cursor() {
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
            orchestration_chat: true,
            model: None,
        },
    )
    .unwrap();

    let task = context
        .registry
        .list_tasks()
        .into_iter()
        .find(|task| task.qualified_handle() == "web/fix-login")
        .expect("task created");
    assert!(task.skip_interactive_agent());

    assert!(
        runner
            .commands()
            .iter()
            .all(|command| !(command.program == "tmux"
                && command.args.first() == Some(&"send-keys".to_string()))),
        "orchestration chat must not send interactive cursor CLI keys"
    );
}

#[test]
fn start_task_orchestration_chat_provisions_each_acp_harness() {
    for agent in ["codex", "claude", "pi"] {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        super::start_task(
            &mut context,
            &mut runner,
            super::StartTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: agent.to_string(),
                request_id: String::new(),
                orchestration_chat: true,
                model: None,
            },
        )
        .unwrap();

        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .expect("task created");
        assert!(task.skip_interactive_agent(), "{agent} should provision");
        assert!(
            runner
                .commands()
                .iter()
                .all(|command| !(command.program == "tmux"
                    && command.args.first() == Some(&"send-keys".to_string()))),
            "{agent} must not also launch interactively in tmux"
        );
    }
}

fn start_chat_task(
    context: &mut CommandContext<InMemoryRegistry>,
    runner: &mut impl CommandRunner,
    agent: &str,
    orchestration_chat: bool,
) {
    super::start_task(
        context,
        runner,
        super::StartTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: agent.to_string(),
            request_id: String::new(),
            orchestration_chat,
            model: None,
        },
    )
    .unwrap();
}

fn is_tmux_command(command: &CommandSpec) -> bool {
    command.program == "tmux"
}

struct TmuxMissingRunner {
    inner: RecordingCommandRunner,
}

impl CommandRunner for TmuxMissingRunner {
    fn run(&mut self, command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        if is_tmux_command(command) {
            return Err(CommandRunError::SpawnFailed(
                "No such file or directory (os error 2)".to_string(),
            ));
        }
        self.inner.run(command)
    }
}

#[test]
fn start_task_orchestration_chat_issues_no_tmux_commands_for_every_acp_harness() {
    for agent in ["codex", "claude", "pi", "cursor"] {
        let mut context = context_with_managed_repo();
        let mut runner = RecordingCommandRunner::default();

        start_chat_task(&mut context, &mut runner, agent, true);

        assert!(
            runner
                .commands()
                .iter()
                .all(|command| !is_tmux_command(command)),
            "{agent} chat provisioning must not invoke tmux, got {:?}",
            runner
                .commands()
                .iter()
                .filter(|command| is_tmux_command(command))
                .map(|command| &command.args)
                .collect::<Vec<_>>()
        );
        assert!(
            runner.commands().iter().any(|command| {
                command.program == "git"
                    && command.args.first().map(String::as_str) == Some("-C")
                    && command.args.iter().any(|arg| arg == "worktree")
                    && command.args.iter().any(|arg| arg == "add")
            }),
            "{agent} chat provisioning must still create a worktree"
        );

        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .expect("task created")
            .clone();
        assert!(
            task.skip_interactive_agent(),
            "{agent} should be persisted as chat"
        );

        let mut terminal_context = context_with_managed_repo();
        let mut terminal_runner = RecordingCommandRunner::default();
        start_chat_task(&mut terminal_context, &mut terminal_runner, agent, false);
        let terminal_task = terminal_context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .expect("terminal task created");
        assert_eq!(
            task.lifecycle_status, terminal_task.lifecycle_status,
            "{agent} chat task should reach the same post-start lifecycle as a terminal task"
        );
    }
}

#[test]
fn start_task_orchestration_chat_succeeds_when_tmux_is_not_installed() {
    for agent in ["codex", "claude", "pi", "cursor"] {
        let mut context = context_with_managed_repo();
        let mut runner = TmuxMissingRunner {
            inner: RecordingCommandRunner::default(),
        };

        start_chat_task(&mut context, &mut runner, agent, true);

        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == "web/fix-login")
            .expect("task created")
            .clone();
        assert!(
            task.skip_interactive_agent(),
            "{agent} should be persisted as chat"
        );

        ajax_core::runtime_refresh::refresh_runtime_context(&mut context, &mut runner)
            .expect("refresh succeeds without tmux");

        let refreshed = context
            .registry
            .get_task(&task.id)
            .expect("chat task survives refresh");
        assert!(
            !refreshed.has_side_flag(SideFlag::TmuxMissing),
            "{agent} chat task must not be flagged tmux-missing without tmux"
        );
        assert!(
            !refreshed.has_side_flag(SideFlag::TaskWindowMissing),
            "{agent} chat task must not be flagged task-window-missing without tmux"
        );
        assert_eq!(
            refreshed.runtime_projection.observation_error, None,
            "{agent} chat task must not record a tmux probe error"
        );
    }
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
            orchestration_chat: false,
            model: None,
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
            orchestration_chat: false,
            model: None,
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
            orchestration_chat: false,
            model: None,
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

fn context_with_renamed_branch() -> CommandContext<InMemoryRegistry> {
    let mut context = context_with_reviewable_task();
    let task = context
        .registry
        .get_task_mut(&TaskId::new("web/fix-login"))
        .unwrap();
    task.lifecycle_status = LifecycleStatus::Active;
    task.git_status = Some(GitStatus {
        worktree_exists: true,
        branch_exists: false,
        current_branch: Some("fix/web/login".to_string()),
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

#[path = "git_evidence_tests.rs"]
mod git_evidence_tests;
