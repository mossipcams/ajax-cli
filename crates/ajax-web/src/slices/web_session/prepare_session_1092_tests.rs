//! Promote-on-attach for #1092.

use super::prepare_task_session;
use super::SessionRouteError;
use ajax_core::adapters::CommandRunner;
use ajax_core::{
    adapters::{CommandOutput, CommandRunError, CommandSpec},
    models::AgentClient,
    registry::Registry as _,
};

struct PaneRunner {
    stdout: String,
}

impl CommandRunner for PaneRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            status_code: 0,
            stdout: self.stdout.clone(),
            stderr: String::new(),
        })
    }
}

struct FailingTmuxRunner;

impl CommandRunner for FailingTmuxRunner {
    fn run(&mut self, _command: &CommandSpec) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            status_code: 1,
            stdout: String::new(),
            stderr: "no server".to_string(),
        })
    }
}

#[test]
fn prepare_task_session_promotes_dead_pane_codex_issue_1092() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Codex;
    let worktree = std::env::temp_dir().join("ajax-web-session-test-promote-dead-codex");
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    task.worktree_path = worktree;
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = PaneRunner {
        stdout: "zsh\n".to_string(),
    };

    let plan = prepare_task_session(&mut context, &mut runner, "web/fix-login", "auto")
        .expect("promote-on-attach");

    assert_eq!(plan.agent, AgentClient::Codex);
    let task = context
        .registry
        .get_task(&ajax_core::models::TaskId::new("web/fix-login"))
        .expect("task");
    assert!(
        task.skip_interactive_agent(),
        "promote sets provisioned bit"
    );
}

#[test]
fn prepare_task_session_keeps_409_when_live_codex_pane_issue_1092() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Codex;
    let worktree = std::env::temp_dir().join("ajax-web-session-test-promote-live-codex");
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    task.worktree_path = worktree;
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = PaneRunner {
        stdout: "codex\n".to_string(),
    };

    let error =
        prepare_task_session(&mut context, &mut runner, "web/fix-login", "auto").unwrap_err();
    assert_eq!(error, SessionRouteError::NotOrchestrationChat);
    let task = context
        .registry
        .get_task(&ajax_core::models::TaskId::new("web/fix-login"))
        .expect("task");
    assert!(!task.skip_interactive_agent(), "live pane must not promote");
}

#[test]
fn prepare_task_session_promotes_when_tmux_probe_fails_issue_1092() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Codex;
    let worktree = std::env::temp_dir().join("ajax-web-session-test-promote-no-tmux");
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    task.worktree_path = worktree;
    let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let mut runner = FailingTmuxRunner;

    prepare_task_session(&mut context, &mut runner, "web/fix-login", "auto").expect("attach");
    assert!(context
        .registry
        .get_task(&ajax_core::models::TaskId::new("web/fix-login"))
        .expect("task")
        .skip_interactive_agent());
}

#[test]
fn prepare_task_session_admits_durable_restore_capable_provisioned_harness() {
    use ajax_core::adapters::acp_admits_orchestration_chat;

    for agent in [
        AgentClient::Cursor,
        AgentClient::Codex,
        AgentClient::Claude,
        AgentClient::Pi,
    ] {
        assert!(
            acp_admits_orchestration_chat(agent),
            "{agent:?} must be admitted before attach"
        );
        let mut task = crate::test_support::fix_login_task();
        task.selected_agent = agent;
        task.set_skip_interactive_agent(true);
        let worktree = std::env::temp_dir().join(format!(
            "ajax-web-session-test-durable-admit-{}",
            format!("{agent:?}").to_ascii_lowercase()
        ));
        let _ = std::fs::remove_dir_all(&worktree);
        std::fs::create_dir_all(&worktree).expect("worktree dir");
        task.worktree_path = worktree;
        let mut context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let mut runner = PaneRunner {
            stdout: "zsh\n".to_string(),
        };
        let plan = prepare_task_session(&mut context, &mut runner, "web/fix-login", "auto")
            .expect("provisioned durable-capable harness attaches");
        assert_eq!(plan.agent, agent);
    }
}
