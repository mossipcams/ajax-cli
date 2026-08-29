use super::{
    mark_new_task_provisioning_step_completed, new_task_plan, new_task_plan_with_observation,
    record_new_task, NewTaskRequest, StartPlanObservation, StartProvisioningStep,
    DEFAULT_TASK_WINDOW_NAME,
};
use crate::{
    commands::CommandContext,
    config::{Config, ManagedRepo},
    models::{AgentClient, AgentRuntimeStatus, LifecycleStatus, SideFlag, Task, TaskId},
    registry::{InMemoryRegistry, Registry},
};

fn context() -> CommandContext<InMemoryRegistry> {
    CommandContext::new(
        Config {
            repos: vec![ManagedRepo::new("web", "/repo/web", "main")],
            ..Config::default()
        },
        InMemoryRegistry::default(),
    )
}

/// GitHub #1096 — Wave 1 failing test: launch episode must close when spawn/auth
/// never starts a turn. Today `AgentCommandSent` opens a Running attempt while
/// `agent_status` stays `NotStarted`.
#[test]
fn wave1_issue_1096_open_attempt_must_not_run_while_agent_not_started() {
    let mut context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "cursor".to_string(),
        skip_interactive_agent: true,
        model: None,
    };
    let task = record_new_task(&mut context, &request).unwrap();
    let task_id = task.id.clone();

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::AgentCommandSent,
    )
    .unwrap();

    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.agent_status, AgentRuntimeStatus::NotStarted);
    assert_eq!(task.agent_attempts.len(), 1);
    assert_ne!(
        task.agent_attempts[0].status,
        AgentRuntimeStatus::Running,
        "launch episode should close when spawn/auth never starts a turn (#1096)"
    );
    assert!(
        task.agent_attempts[0].finished_at.is_some(),
        "finished_at must be set when the launch ends without a turn (#1096)"
    );
}

#[test]
fn provisioned_acp_agent_command_sent_does_not_claim_agent_working() {
    let mut context = context();
    let request = NewTaskRequest {
        repo: "web".to_string(),
        title: "Fix login".to_string(),
        agent: "cursor".to_string(),
        skip_interactive_agent: true,
        model: None,
    };
    let task = record_new_task(&mut context, &request).unwrap();
    let task_id = task.id.clone();

    mark_new_task_provisioning_step_completed(
        &mut context,
        &task_id,
        StartProvisioningStep::AgentCommandSent,
    )
    .unwrap();

    let task = context.registry.get_task(&task_id).unwrap();
    assert_eq!(task.lifecycle_status, LifecycleStatus::Active);
    assert!(
        !task.has_side_flag(SideFlag::AgentRunning),
        "provisioned ACP tasks must not read Agent working before the first turn (#1069)"
    );
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
        model: None,
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
        model: None,
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
            model: None,
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
            model: None,
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
