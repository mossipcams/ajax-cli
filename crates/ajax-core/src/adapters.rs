pub mod agent;
pub mod command;
pub mod environment;
pub mod git;
pub mod github;
pub mod process;
pub mod tmux;

pub use agent::{
    acp_adapter_packages, acp_admits_orchestration_chat, acp_args_for_candidate,
    acp_launch_for_agent, acp_spawn_model_for_argv, agent_launch_spec,
    canonical_cursor_model_intent, cursor_bracket_token_from_intent,
    cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
    cursor_model_intents_match, cursor_unspecified_spawn_satisfied, is_unspecified_acp_model,
    parse_cursor_model_intent, parse_model_selection, valid_cursor_model_id, AcpLaunch,
    AcpModelSelection, AgentLaunch, CursorModelIntent, ModelSelection, CURSOR_DEFAULT_MODEL,
    CURSOR_DEFAULT_SPAWN_MODEL,
};
pub use command::{
    CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
};
pub use environment::{DoctorEnvironment, REQUIRED_DOCTOR_TOOLS};
pub use git::GitAdapter;
pub use github::{CiChecksObservation, CiChecksReport, CiChecksState, GithubChecksAdapter};
pub use process::{clear_ambient_git_env, ProcessCommandRunner, AMBIENT_GIT_ENV_VARS};
pub use tmux::TmuxAdapter;

#[cfg(test)]
mod tests {
    use super::{
        agent_launch_spec, AgentLaunch, CommandMode, CommandRunner, CommandSpec, GitAdapter,
        RecordingCommandRunner, TmuxAdapter,
    };
    use super::{command, process};
    use crate::models::{TaskWindowStatus, TmuxStatus};
    use proptest::prelude::*;
    use std::path::Path;

    fn safe_token() -> impl Strategy<Value = String> {
        "[A-Za-z0-9_.-]{1,32}"
    }

    fn safe_path() -> impl Strategy<Value = String> {
        prop::collection::vec("[A-Za-z0-9_.-]{1,16}", 1..6)
            .prop_map(|segments| format!("/{}", segments.join("/")))
    }

    #[test]
    fn tmux_adapter_builds_attach_switch_and_task_commands() {
        let adapter = TmuxAdapter::new("tmux");

        assert_eq!(
            adapter.attach_session("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["attach-session", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.switch_client("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.new_detached_task_session("ajax-web-fix-login", "task", "/tmp/worktree"),
            CommandSpec::new(
                "tmux",
                [
                    "new-session",
                    "-d",
                    "-s",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktree"
                ]
            )
        );
        assert_eq!(
            adapter.ensure_task_window("ajax-web-fix-login", "task", "/tmp/worktree"),
            CommandSpec::new(
                "tmux",
                [
                    "new-window",
                    "-t",
                    "ajax-web-fix-login",
                    "-n",
                    "task",
                    "-c",
                    "/tmp/worktree"
                ]
            )
        );
        assert_eq!(
            adapter.kill_window("ajax-web-fix-login", "task"),
            CommandSpec::new("tmux", ["kill-window", "-t", "ajax-web-fix-login:task"])
        );
        assert_eq!(
            adapter.select_window("ajax-web-fix-login", "task"),
            CommandSpec::new("tmux", ["select-window", "-t", "ajax-web-fix-login:task"])
        );
        assert_eq!(
            adapter.switch_client_to_window("ajax-web-fix-login", "task"),
            CommandSpec::new("tmux", ["switch-client", "-t", "ajax-web-fix-login"])
                .with_mode(CommandMode::InheritStdio)
        );
        assert_eq!(
            adapter.send_agent_command("ajax-web-fix-login", "task", "codex --cd /tmp/worktree"),
            CommandSpec::new(
                "tmux",
                [
                    "send-keys",
                    "-t",
                    "ajax-web-fix-login:task",
                    "codex --cd /tmp/worktree",
                    "Enter"
                ]
            )
        );
        assert_eq!(
            adapter.kill_session("ajax-web-fix-login"),
            CommandSpec::new("tmux", ["kill-session", "-t", "ajax-web-fix-login"])
        );
        assert_eq!(
            adapter.list_sessions(),
            CommandSpec::new("tmux", ["list-sessions", "-F", "#{session_name}"])
                .with_timeout(std::time::Duration::from_secs(8))
        );
        assert_eq!(
            adapter.list_windows("ajax-web-fix-login"),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-t",
                    "ajax-web-fix-login",
                    "-F",
                    "#{window_name}\t#{pane_current_path}"
                ]
            )
        );
        assert_eq!(
            adapter.list_all_windows(),
            CommandSpec::new(
                "tmux",
                [
                    "list-windows",
                    "-a",
                    "-F",
                    "#{session_name}\t#{window_name}\t#{pane_current_path}"
                ]
            )
            .with_timeout(std::time::Duration::from_secs(8))
        );
        assert_eq!(
            adapter.capture_pane("ajax-web-fix-login", "task"),
            CommandSpec::new(
                "tmux",
                ["capture-pane", "-p", "-t", "ajax-web-fix-login:task"]
            )
            .with_timeout(std::time::Duration::from_secs(8))
        );
    }

    proptest! {
        #[test]
        fn tmux_adapter_targets_generated_task_inputs(
            session in safe_token(),
            window in safe_token(),
            path in safe_path(),
            command in "[^\\x00]{0,80}"
        ) {
            let adapter = TmuxAdapter::new("tmux");
            let target = format!("{session}:{window}");

            prop_assert_eq!(
                adapter.new_detached_task_session(&session, &window, &path),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-session",
                        "-d",
                        "-s",
                        session.as_str(),
                        "-n",
                        window.as_str(),
                        "-c",
                        path.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.ensure_task_window(&session, &window, &path),
                CommandSpec::new(
                    "tmux",
                    [
                        "new-window",
                        "-t",
                        session.as_str(),
                        "-n",
                        window.as_str(),
                        "-c",
                        path.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.select_window(&session, &window).args,
                vec!["select-window", "-t", target.as_str()]
            );
            prop_assert_eq!(
                adapter.kill_window(&session, &window).args,
                vec!["kill-window", "-t", target.as_str()]
            );
            prop_assert_eq!(
                adapter.capture_pane(&session, &window).args,
                vec!["capture-pane", "-p", "-t", target.as_str()]
            );
            prop_assert_eq!(
                adapter.send_agent_command(&session, &window, &command).args,
                vec!["send-keys", "-t", target.as_str(), command.as_str(), "Enter"]
            );
        }

        #[test]
        fn git_adapter_native_lifecycle_commands_preserve_generated_inputs(
            repo_path in safe_path(),
            worktree_path in safe_path(),
            branch_suffix in safe_token(),
            start_point in safe_token()
        ) {
            let adapter = GitAdapter::new("git");
            let branch = format!("ajax/{branch_suffix}");

            prop_assert_eq!(
                adapter.add_worktree(&repo_path, &worktree_path, &branch, &start_point),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        repo_path.as_str(),
                        "worktree",
                        "add",
                        "-b",
                        branch.as_str(),
                        worktree_path.as_str(),
                        start_point.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.add_worktree_existing_branch(&repo_path, &worktree_path, &branch),
                CommandSpec::new(
                    "git",
                    [
                        "-C",
                        repo_path.as_str(),
                        "worktree",
                        "add",
                        worktree_path.as_str(),
                        branch.as_str(),
                    ],
                )
            );
            prop_assert_eq!(
                adapter.remove_worktree(&repo_path, &worktree_path).args,
                vec!["-C", repo_path.as_str(), "worktree", "remove", worktree_path.as_str()]
            );
            prop_assert_eq!(
                adapter.force_remove_worktree(&repo_path, &worktree_path).args,
                vec![
                    "-C",
                    repo_path.as_str(),
                    "worktree",
                    "remove",
                    "--force",
                    worktree_path.as_str(),
                ]
            );
            prop_assert_eq!(
                adapter.delete_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "branch", "-d", branch.as_str()]
            );
            prop_assert_eq!(
                adapter.force_delete_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "branch", "-D", branch.as_str()]
            );
            prop_assert_eq!(
                adapter.switch_branch(&repo_path, &start_point).args,
                vec!["-C", repo_path.as_str(), "switch", start_point.as_str()]
            );
            prop_assert_eq!(
                adapter.merge_branch(&repo_path, &branch).args,
                vec!["-C", repo_path.as_str(), "merge", "--ff-only", branch.as_str()]
            );
        }
    }

    #[test]
    fn git_adapter_builds_native_lifecycle_commands() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.add_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "ajax/fix-login",
                "main"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "-b",
                    "ajax/fix-login",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login",
                    "main"
                ]
            )
        );
        assert_eq!(
            adapter.add_worktree_existing_branch(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login",
                "ajax/fix-login",
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "add",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login",
                    "ajax/fix-login",
                ]
            )
        );
        assert_eq!(
            adapter.remove_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.force_remove_worktree(
                "/Users/matt/projects/web",
                "/Users/matt/projects/web__worktrees/ajax-fix-login"
            ),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "worktree",
                    "remove",
                    "--force",
                    "/Users/matt/projects/web__worktrees/ajax-fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.delete_branch("/Users/matt/projects/web", "ajax/fix-login"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-d",
                    "ajax/fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.force_delete_branch("/Users/matt/projects/web", "ajax/fix-login"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/Users/matt/projects/web",
                    "branch",
                    "-D",
                    "ajax/fix-login"
                ]
            )
        );
        assert_eq!(
            adapter.switch_branch("/Users/matt/projects/web", "main"),
            CommandSpec::new("git", ["-C", "/Users/matt/projects/web", "switch", "main"])
        );
        assert_eq!(
            adapter.merge_branch("/Users/matt/projects/web", "ajax/fix-login"),
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
        );
    }

    #[test]
    fn tmux_interactive_commands_inherit_stdio() {
        let adapter = TmuxAdapter::new("tmux");

        assert_eq!(
            adapter.attach_session("ajax-web-fix-login").mode,
            CommandMode::InheritStdio
        );
        assert_eq!(
            adapter.switch_client("ajax-web-fix-login").mode,
            CommandMode::InheritStdio
        );
        assert_eq!(adapter.list_sessions().mode, CommandMode::Capture);
    }

    #[test]
    fn tmux_parsers_detect_session_and_task_health() {
        let tmux = TmuxAdapter::parse_session_status(
            "ajax-web-fix-login",
            "ajax-api-add-cache\najax-web-fix-login\n",
        );
        let task = TmuxAdapter::parse_task_window_status(
            "task",
            "/tmp/worktree",
            "agent\t/tmp/worktree\ntask\t/tmp/worktree\n",
        );

        assert_eq!(tmux, TmuxStatus::present("ajax-web-fix-login"));
        assert_eq!(task, TaskWindowStatus::present("task", "/tmp/worktree"));
    }

    #[test]
    fn tmux_task_parser_detects_wrong_path() {
        let task =
            TmuxAdapter::parse_task_window_status("task", "/tmp/worktree", "task\t/tmp/wrong\n");

        assert!(task.exists);
        assert_eq!(task.current_path, std::path::PathBuf::from("/tmp/wrong"));
        assert!(!task.points_at_expected_path);
    }
}
