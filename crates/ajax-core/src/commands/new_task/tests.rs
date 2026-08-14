use super::{
    is_git_worktree_add_command, mark_new_task_provisioning_step_completed, new_task_plan,
    new_task_plan_with_observation, record_new_task, task_from_new_request, NewTaskRequest,
    StartPlanObservation, StartProvisioningStep, DEFAULT_TASK_WINDOW_NAME,
};
use crate::{
    adapters::GitAdapter,
    commands::CommandContext,
    config::{Config, ManagedRepo, RuntimePathRequest, WorktreePlacement},
    models::{AgentRuntimeStatus, LifecycleStatus, SideFlag},
    registry::{InMemoryRegistry, Registry},
};
use std::{path::Path, time::Duration};

fn context() -> CommandContext<InMemoryRegistry> {
    CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    )
}

fn agent_send_keys_line(plan: &crate::commands::CommandPlan) -> &str {
    plan.commands
        .iter()
        .find(|command| {
            command.program == "tmux" && command.args.first() == Some(&"send-keys".to_string())
        })
        .map(|command| command.args[3].as_str())
        .expect("expected tmux send-keys command")
}

#[test]
fn skip_interactive_agent_cursor_plan_skips_agent_send_keys() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "cursor".to_string(),
        skip_interactive_agent: true,
    };
    let plan = new_task_plan(&context, request).expect("plan");
    assert!(plan.commands.iter().all(|command| {
        !(command.program == "tmux" && command.args.first() == Some(&"send-keys".to_string()))
    }));
}

#[test]
fn task_from_new_request_sets_skip_interactive_agent_for_cursor() {
    let context = context();
    let task = task_from_new_request(
        &context,
        &NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "cursor".to_string(),
            skip_interactive_agent: true,
        },
    )
    .unwrap();
    assert!(task.skip_interactive_agent());
}

#[test]
fn task_from_new_request_skips_bit_when_cursor_interactive() {
    let context = context();
    let task = task_from_new_request(
        &context,
        &NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "cursor".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();
    assert!(!task.skip_interactive_agent());
}

#[test]
fn task_from_new_request_skips_bit_for_non_cursor_even_when_flag_true() {
    let context = context();
    let task = task_from_new_request(
        &context,
        &NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: true,
        },
    )
    .unwrap();
    assert!(!task.skip_interactive_agent());
}

#[test]
fn rooted_repo_dir_hash_is_stable_for_known_path() {
    let path = Path::new("/Users/matt/projects/web");
    let first = super::rooted_repo_dir("web", path);
    let second = super::rooted_repo_dir("web", path);

    assert_eq!(first, "web-8ac1d219");
    assert_eq!(second, first);
}

#[test]
fn start_task_identity_uses_core_slug_rules() {
    let first = super::start_task_identity("web", "Fix login");
    let second = super::start_task_identity("web", "Fix login!");

    assert_eq!(first, crate::models::TaskId::new("web/fix-login"));
    assert_eq!(second, first);
}

#[test]
fn unknown_agent_is_preserved_for_execution_but_classified_other() {
    let context = context();
    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "custom-agent-cli".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.ends_with("-- custom-agent-cli"));
    assert_eq!(
        task_from_new_request(
            &context,
            &NewTaskRequest {
                repo: "web".to_string(),
                title: "Fix login".to_string(),
                agent: "custom-agent-cli".to_string(),
                skip_interactive_agent: false,
            }
        )
        .unwrap()
        .selected_agent,
        crate::models::AgentClient::Other
    );
}

#[test]
fn punctuation_only_title_uses_deterministic_fallback_id() {
    let first = super::start_task_identity("web", "!!!");
    let second = super::start_task_identity("web", "!!!");

    assert_eq!(first, crate::models::TaskId::new("web/task"));
    assert_eq!(second, first);
}

#[test]
fn repo_name_cannot_escape_managed_namespace() {
    let context = context();
    for repo in ["../escape", "web/evil", r"web\evil", ".."] {
        let error = new_task_plan(
            &context,
            NewTaskRequest {
                repo: repo.to_string(),
                title: "Fix login".to_string(),
                agent: "codex".to_string(),
                skip_interactive_agent: false,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            crate::commands::CommandError::PlanBlocked(vec![format!("invalid repo name: {repo}")])
        );
    }
}

#[test]
fn new_task_plan_claude_agent_command_omits_cd_flag_and_skips_permissions() {
    let context = context();
    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "claude".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.contains("ajax-cli __agent-runtime --task-id web/fix-login"));
    assert!(launch.ends_with("-- claude --dangerously-skip-permissions"));
    assert!(!launch.contains("--cd"));
}

#[test]
fn new_task_plan_cursor_agent_command_uses_agent_subcommand() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "cursor".to_string(),
        skip_interactive_agent: false,
    };
    let plan = new_task_plan(&context, request.clone()).unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.contains("ajax-cli __agent-runtime --task-id web/fix-login"));
    assert!(
        launch.ends_with("-- cursor agent --model cursor-grok-4.6-high"),
        "cursor launch must pin Grok 4.6 high, got: {launch}"
    );
    assert_eq!(
        task_from_new_request(&context, &request)
            .unwrap()
            .selected_agent,
        crate::models::AgentClient::Cursor
    );
}

#[test]
fn new_task_plan_pi_agent_stores_pi_client() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "pi".to_string(),
        skip_interactive_agent: false,
    };
    let plan = new_task_plan(&context, request.clone()).unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.contains("ajax-cli __agent-runtime --task-id web/fix-login"));
    assert!(launch.ends_with("-- pi"));
    assert_eq!(
        task_from_new_request(&context, &request)
            .unwrap()
            .selected_agent,
        crate::models::AgentClient::Pi
    );
}

#[test]
fn new_task_plan_launches_agent_through_runtime_wrapper() {
    let context = CommandContext::with_runtime_paths(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
        RuntimePathRequest::new("/home/test").resolve(),
    );

    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    assert_eq!(
        agent_send_keys_line(&plan),
        "if [ -f package.json ] && [ -f .husky/pre-commit ]; then npm exec --yes husky; fi; ajax-cli __agent-runtime --task-id web/fix-login --state-root /home/test/.cache/ajax/agent-runtime -- codex --cd /repo/web__worktrees/ajax-fix-login"
    );
}

#[test]
fn new_task_plan_has_no_standalone_setup_command() {
    let context = context();
    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    assert!(
        !plan.commands.iter().any(|command| command.program == "sh"),
        "expected no standalone setup command: {:?}",
        plan.commands
    );
}

#[test]
fn new_task_plan_folds_husky_into_agent_send_keys() {
    let context = context();
    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.contains("npm exec --yes husky"));
    assert!(launch.contains("ajax-cli __agent-runtime --task-id web/fix-login"));
    assert!(launch.ends_with(
        "ajax-cli __agent-runtime --task-id web/fix-login --state-root .cache/ajax/agent-runtime -- codex --cd /repo/web__worktrees/ajax-fix-login"
    ));
}

#[test]
fn new_task_plan_chains_bootstrap_between_husky_and_agent() {
    let mut repo = ManagedRepo::new("web", "/repo/web", "main");
    repo.bootstrap = Some("npm install".to_string());
    let context = CommandContext::new(
        Config {
            repos: vec![repo],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    );
    let plan = new_task_plan(
        &context,
        NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        },
    )
    .unwrap();

    let launch = agent_send_keys_line(&plan);
    assert!(launch.contains("npm exec --yes husky"));
    assert!(launch.contains("npm install && ajax-cli __agent-runtime"));
    assert!(
        !plan.commands.iter().any(|command| command.program == "sh"),
        "expected no standalone bootstrap command: {:?}",
        plan.commands
    );
}

#[test]
fn new_task_plan_fetches_origin_and_branches_from_remote_tracking_ref() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };

    let plan = new_task_plan(&context, request).unwrap();
    let git = GitAdapter::new("git");

    assert_eq!(plan.commands.len(), 4);
    assert_eq!(
        plan.commands[0],
        git.fetch_origin_branch("/repo/web", "main")
    );
    assert_eq!(
        plan.commands[1],
        git.add_worktree(
            "/repo/web",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax/fix-login",
            "origin/main"
        )
    );
}

#[test]
fn new_task_plan_skips_fetch_when_origin_fetch_is_fresh() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };
    let observation = StartPlanObservation {
        origin_fetch_age: Some(Duration::from_secs(30)),
        target_branch_exists: false,
    };

    let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
    let git = GitAdapter::new("git");

    assert_eq!(plan.commands.len(), 3);
    assert_eq!(
        plan.commands[0],
        git.add_worktree(
            "/repo/web",
            "/repo/web__worktrees/ajax-fix-login",
            "ajax/fix-login",
            "origin/main"
        )
    );
    assert!(plan
        .commands
        .iter()
        .all(|command| !command.args.iter().any(|arg| arg == "fetch")));
}

#[test]
fn new_task_plan_fetches_when_origin_fetch_is_stale() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };
    let observation = StartPlanObservation {
        origin_fetch_age: Some(Duration::from_secs(120)),
        target_branch_exists: false,
    };

    let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
    let git = GitAdapter::new("git");

    assert_eq!(plan.commands.len(), 4);
    assert_eq!(
        plan.commands[0],
        git.fetch_origin_branch("/repo/web", "main")
    );
}

#[test]
fn new_task_plan_fetches_when_origin_fetch_age_is_unknown() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };
    let observation = StartPlanObservation {
        origin_fetch_age: None,
        target_branch_exists: false,
    };

    let plan = new_task_plan_with_observation(&context, request, &observation).unwrap();
    let git = GitAdapter::new("git");

    assert_eq!(plan.commands.len(), 4);
    assert_eq!(
        plan.commands[0],
        git.fetch_origin_branch("/repo/web", "main")
    );
}

#[test]
fn default_new_task_plan_preserves_legacy_sibling_worktree_path() {
    let context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };

    let plan = new_task_plan(&context, request).unwrap();

    let worktree_command = plan
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("worktree add command");
    assert!(worktree_command
        .args
        .contains(&"/repo/web__worktrees/ajax-fix-login".to_string()));
}

#[test]
fn rooted_new_task_plan_and_recorded_task_use_runtime_worktree_root() {
    let runtime_paths = RuntimePathRequest::new("/Users/matt")
        .with_cli_profile("dev")
        .resolve();
    let worktree_root = match &runtime_paths.worktree_placement {
        WorktreePlacement::Root(root) => root.clone(),
        WorktreePlacement::LegacySibling => panic!("expected rooted placement"),
    };
    let context = CommandContext::with_runtime_paths(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
        runtime_paths,
    );
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };

    let plan = new_task_plan(&context, request.clone()).unwrap();
    let task = task_from_new_request(&context, &request).unwrap();
    let worktree_command = plan
        .commands
        .iter()
        .find(|command| is_git_worktree_add_command(command))
        .expect("worktree add command");
    let planned_worktree = worktree_command
        .args
        .iter()
        .find(|arg| arg.starts_with(worktree_root.to_str().unwrap()))
        .unwrap();

    assert!(task.worktree_path.starts_with(&worktree_root));
    assert_eq!(Path::new(planned_worktree), task.worktree_path);
    assert!(plan.commands.iter().any(|command| command
        .args
        .iter()
        .any(|arg| arg == task.worktree_path.to_str().unwrap())));
}

#[test]
fn start_provisioning_named_steps_update_state_without_numeric_command_indexes() {
    let mut context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };
    let task = record_new_task(&mut context, &request).unwrap();
    let task_id = task.id.clone();

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::WorktreeCreated,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    let git = task.git_status.as_ref().unwrap();
    assert!(git.worktree_exists);
    assert!(git.branch_exists);
    assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::TaskSessionCreated,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    assert!(task
        .tmux_status
        .as_ref()
        .is_some_and(|status| status.exists));
    assert!(task
        .task_window_status
        .as_ref()
        .is_some_and(|status| status.exists && status.points_at_expected_path));
    assert_eq!(task.lifecycle_status, LifecycleStatus::Provisioning);

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::AgentCommandSent,
    )
    .unwrap();
    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(task.has_side_flag(SideFlag::AgentRunning));
    assert_eq!(task.agent_attempts.len(), 1);
    assert_eq!(task.agent_attempts[0].status, AgentRuntimeStatus::Running);
}

fn start_collision_task(
    repo: &str,
    handle: &str,
    branch: &str,
    worktree_path: std::path::PathBuf,
) -> crate::models::Task {
    use crate::models::{AgentClient, Task, TaskId};
    let tmux_session = format!("ajax-{repo}-{handle}");
    Task::new(
        TaskId::new(format!("{repo}/{handle}")),
        repo.to_string(),
        handle.to_string(),
        handle.to_string(),
        branch.to_string(),
        "main".to_string(),
        worktree_path,
        tmux_session,
        DEFAULT_TASK_WINDOW_NAME.to_string(),
        AgentClient::Codex,
    )
}

#[test]
fn new_task_plan_blocks_when_worktree_path_already_exists() {
    let root = std::env::temp_dir().join(format!(
        "ajax-start-blocked-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let repo_path = root.join("web");
    let worktree_path = root.join("web__worktrees").join("ajax-fix-login");
    std::fs::create_dir_all(&worktree_path).unwrap();

    let context = CommandContext::new(
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
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };

    let error = new_task_plan(&context, request).unwrap_err();
    let crate::commands::CommandError::PlanBlocked(messages) = &error else {
        panic!("expected PlanBlocked, got {error:?}");
    };
    let message = messages.join("\n");
    assert!(
        message.contains(&worktree_path.display().to_string()),
        "expected message to mention worktree path: {message}"
    );
    assert!(message.contains("already exists"), "message: {message}");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn new_task_plan_blocks_when_target_branch_already_exists() {
    let context = context();
    let observation = StartPlanObservation {
        origin_fetch_age: None,
        target_branch_exists: true,
    };
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "codex".to_string(),
        skip_interactive_agent: false,
    };

    let error = new_task_plan_with_observation(&context, request, &observation).unwrap_err();
    let crate::commands::CommandError::PlanBlocked(messages) = &error else {
        panic!("expected PlanBlocked, got {error:?}");
    };
    let message = messages.join("\n");
    assert!(message.contains("ajax/fix-login"), "message: {message}");
    assert!(message.contains("branch"), "message: {message}");
}

#[test]
fn new_task_plan_blocks_when_registry_claims_worktree_path_or_branch() {
    use std::path::PathBuf;

    // worktree-path claim
    {
        let mut context = context();
        context
            .registry
            .create_task(start_collision_task(
                "web",
                "owasp",
                "ajax/owasp",
                PathBuf::from("/repo/web__worktrees/ajax-fix-login"),
            ))
            .unwrap();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        };

        let error = new_task_plan(&context, request).unwrap_err();
        let crate::commands::CommandError::PlanBlocked(messages) = &error else {
            panic!("expected PlanBlocked, got {error:?}");
        };
        let message = messages.join("\n");
        assert!(
            message.contains("web/owasp"),
            "expected claiming handle: {message}"
        );
        assert!(
            message.contains("/repo/web__worktrees/ajax-fix-login"),
            "message: {message}"
        );
    }

    // branch claim
    {
        let mut context = context();
        context
            .registry
            .create_task(start_collision_task(
                "web",
                "owasp",
                "ajax/fix-login",
                PathBuf::from("/repo/web__worktrees/ajax-owasp"),
            ))
            .unwrap();
        let request = NewTaskRequest {
            repo: "web".to_string(),
            title: "Fix login".to_string(),
            agent: "codex".to_string(),
            skip_interactive_agent: false,
        };

        let error = new_task_plan(&context, request).unwrap_err();
        let crate::commands::CommandError::PlanBlocked(messages) = &error else {
            panic!("expected PlanBlocked, got {error:?}");
        };
        let message = messages.join("\n");
        assert!(
            message.contains("web/owasp"),
            "expected claiming handle: {message}"
        );
        assert!(message.contains("ajax/fix-login"), "message: {message}");
    }
}
