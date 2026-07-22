pub mod agent;
pub mod command;
pub mod environment;
pub mod git;
pub mod github;
pub mod process;
pub mod tmux;

pub use agent::{agent_launch_spec, AgentLaunch};
pub use command::{
    CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
};
pub use environment::{DoctorEnvironment, REQUIRED_DOCTOR_TOOLS};
pub use git::GitAdapter;
pub use github::{CiChecksObservation, GithubChecksAdapter};
pub use process::ProcessCommandRunner;
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

    #[test]
    fn git_adapter_builds_status_commands_for_worktrees() {
        let adapter = GitAdapter::new("git");

        assert_eq!(
            adapter.status("/tmp/worktree"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktree",
                    "status",
                    "--porcelain=v1",
                    "--branch"
                ]
            )
        );
        assert_eq!(
            adapter.merge_base_is_ancestor("/tmp/worktree", "ajax/fix-login", "main"),
            CommandSpec::new(
                "git",
                [
                    "-C",
                    "/tmp/worktree",
                    "merge-base",
                    "--is-ancestor",
                    "ajax/fix-login",
                    "main"
                ]
            )
        );
    }

    #[test]
    fn agent_adapter_builds_launch_command() {
        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: "fix login".to_string(),
        };

        assert_eq!(
            agent_launch_spec("codex", crate::models::AgentClient::Codex, &launch),
            CommandSpec::new("codex", ["--cd", "/tmp/worktree", "fix login"])
        );
    }

    #[test]
    fn agent_adapter_omits_blank_launch_prompt() {
        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: String::new(),
        };

        assert_eq!(
            agent_launch_spec("codex", crate::models::AgentClient::Codex, &launch),
            CommandSpec::new("codex", ["--cd", "/tmp/worktree"])
        );
    }

    #[test]
    fn agent_adapter_claude_launch_omits_cd_flag_and_skips_permissions() {
        use crate::models::AgentClient;

        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: String::new(),
        };

        assert_eq!(
            agent_launch_spec("claude", AgentClient::Claude, &launch),
            CommandSpec::new("claude", ["--dangerously-skip-permissions"])
        );
    }

    #[test]
    fn agent_adapter_cursor_launch_uses_agent_subcommand() {
        use crate::models::AgentClient;

        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: "fix login".to_string(),
        };

        assert_eq!(
            agent_launch_spec("cursor", AgentClient::Cursor, &launch),
            CommandSpec::new("cursor", ["agent", "fix login"])
        );
    }

    #[test]
    fn recording_runner_captures_planned_commands_without_executing() {
        let mut runner = RecordingCommandRunner::default();
        let output = runner.run(&CommandSpec::new("git", ["status"])).unwrap();

        assert_eq!(output.status_code, 0);
        assert_eq!(runner.commands(), &[CommandSpec::new("git", ["status"])]);
    }

    #[test]
    fn command_spec_cwd_preserves_path_boundary() {
        let command = CommandSpec::new("git", ["status"]).with_cwd("/tmp/ajax worktrees/feat a");

        assert_eq!(
            command.cwd.as_deref(),
            Some(Path::new("/tmp/ajax worktrees/feat a"))
        );
    }

    #[test]
    fn process_runner_modes_map_to_process_behavior() {
        fn accepts_port_and_process_runner(
            runner: &mut dyn command::CommandRunner,
        ) -> Result<(), command::CommandRunError> {
            let capture = runner.run(&command::CommandSpec::new(
                "sh",
                ["-c", "printf ajax-capture"],
            ))?;
            assert_eq!(capture.status_code, 0);
            assert_eq!(capture.stdout, "ajax-capture");

            let inherited = runner.run(
                &command::CommandSpec::new("sh", ["-c", "printf ajax-inherit"])
                    .with_mode(command::CommandMode::InheritStdio),
            )?;
            assert_eq!(inherited.status_code, 0);
            assert!(inherited.stdout.is_empty());
            assert!(inherited.stderr.is_empty());

            Ok(())
        }

        let mut runner = process::ProcessCommandRunner;

        accepts_port_and_process_runner(&mut runner).unwrap();
    }

    #[test]
    fn git_status_parser_detects_dirty_untracked_conflicts_and_divergence() {
        let status = GitAdapter::parse_status(
            "## ajax/fix-login...origin/ajax/fix-login [ahead 2, behind 1]\n M src/main.rs\n?? scratch.txt\nUU src/auth.rs\n",
            true,
        );

        assert!(status.worktree_exists);
        assert!(status.branch_exists);
        assert_eq!(status.current_branch.as_deref(), Some("ajax/fix-login"));
        assert!(status.dirty);
        assert_eq!(status.ahead, 2);
        assert_eq!(status.behind, 1);
        assert_eq!(status.untracked_files, 1);
        assert_eq!(status.unpushed_commits, 2);
        assert!(status.conflicted);
        assert!(status.merged);
    }

    #[test]
    fn git_status_parser_handles_clean_local_branch() {
        let status = GitAdapter::parse_status("## main\n", false);

        assert!(status.worktree_exists);
        assert!(status.branch_exists);
        assert_eq!(status.current_branch.as_deref(), Some("main"));
        assert!(!status.dirty);
        assert_eq!(status.ahead, 0);
        assert_eq!(status.behind, 0);
        assert_eq!(status.untracked_files, 0);
        assert_eq!(status.unpushed_commits, 0);
        assert!(!status.conflicted);
        assert!(!status.merged);
    }
}
