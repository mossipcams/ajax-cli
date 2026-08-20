pub mod agent;
pub mod command;
pub mod environment;
pub mod git;
pub mod github;
pub mod process;
pub mod tmux;

pub use agent::{
    acp_adapter_packages, acp_args_for_candidate, acp_launch_for_agent, acp_spawn_model_for_argv,
    agent_launch_spec, cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
    cursor_family_stem, cursor_model_intents_match, cursor_model_intents_match_with_raw,
    cursor_thinking_bases_match, cursor_unspecified_spawn_satisfied,
    encode_cursor_intent_to_storage_pipe, is_unspecified_acp_model, parse_cursor_model_intent,
    parse_model_selection, valid_cursor_model_id, AcpLaunch, AcpModelSelection, AgentLaunch,
    CursorModelIntent, ModelSelection, CURSOR_DEFAULT_MODEL, CURSOR_DEFAULT_SPAWN_MODEL,
};
pub use command::{
    CommandMode, CommandOutput, CommandRunError, CommandRunner, CommandSpec, RecordingCommandRunner,
};
pub use environment::{DoctorEnvironment, REQUIRED_DOCTOR_TOOLS};
pub use git::GitAdapter;
pub use github::{CiChecksObservation, GithubChecksAdapter};
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
            model: None,
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
            model: None,
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
            model: None,
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
            model: None,
        };

        assert_eq!(
            agent_launch_spec("cursor", AgentClient::Cursor, &launch),
            CommandSpec::new(
                "cursor",
                [
                    "agent",
                    "--model",
                    crate::adapters::agent::CURSOR_DEFAULT_MODEL,
                    "fix login"
                ]
            )
        );
    }

    #[test]
    fn acp_launch_maps_every_supported_harness_to_its_entry_point() {
        use crate::adapters::agent::acp_launch_for_agent;
        use crate::models::AgentClient;

        let cursor = acp_launch_for_agent(AgentClient::Cursor).expect("cursor acp");
        assert_eq!(cursor.candidates[0], ("agent", &["acp"][..]));
        assert!(cursor.model_pins_at_spawn());

        for (client, program) in [
            (AgentClient::Codex, "codex-acp"),
            (AgentClient::Claude, "claude-agent-acp"),
            (AgentClient::Pi, "pi-acp"),
        ] {
            let launch = acp_launch_for_agent(client).expect("bridge acp");
            assert_eq!(launch.candidates[0].0, program);
            assert!(launch.candidates[0].1.is_empty());
            // The bridges take no model on argv; they select in-band.
            assert!(!launch.model_pins_at_spawn());
        }

        assert!(acp_launch_for_agent(AgentClient::Other).is_none());
    }

    #[test]
    fn acp_spawn_model_for_argv_pins_cursor_default_when_unspecified() {
        use crate::adapters::agent::{acp_launch_for_agent, acp_spawn_model_for_argv};
        use crate::models::AgentClient;

        let launch = acp_launch_for_agent(AgentClient::Cursor).expect("cursor");
        assert_eq!(
            acp_spawn_model_for_argv(launch, None),
            Some(crate::adapters::agent::CURSOR_DEFAULT_SPAWN_MODEL.to_string())
        );
        assert_eq!(
            acp_spawn_model_for_argv(launch, Some("auto")),
            Some(crate::adapters::agent::CURSOR_DEFAULT_SPAWN_MODEL.to_string())
        );
        assert_eq!(
            acp_spawn_model_for_argv(launch, Some("composer-2.5")),
            Some("composer-2.5".to_string())
        );

        let codex = acp_launch_for_agent(AgentClient::Codex).expect("codex");
        assert_eq!(acp_spawn_model_for_argv(codex, None), None);
        assert_eq!(acp_spawn_model_for_argv(codex, Some("auto")), None);
    }

    #[test]
    fn cursor_catalog_maps_grok_high_to_acp_spawn_token_issue_979() {
        use crate::adapters::agent::{
            cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
        };

        assert_eq!(
            cursor_catalog_to_acp_spawn_token("cursor-grok-4.6-high"),
            "cursor-grok-4.6-high"
        );
        assert_eq!(
            cursor_catalog_to_acp_spawn_token("cursor-grok-4.6-high-fast"),
            "cursor-grok-4.6-high-fast"
        );
        assert_eq!(
            cursor_catalog_to_acp_in_band_token("cursor-grok-4.6-high"),
            "grok-4.6[effort=high,fast=false]"
        );
        assert_eq!(
            cursor_catalog_to_acp_in_band_token("cursor-grok-4.6-high-fast"),
            "grok-4.6[effort=high,fast=true]"
        );
        assert_eq!(
            cursor_catalog_to_acp_spawn_token("composer-2.5"),
            "composer-2.5"
        );
    }

    // Regression for #989: explicit Cursor catalog ids pass through on spawn argv.
    #[test]
    fn cursor_catalog_spawn_passes_through_explicit_catalog_ids_issue_989() {
        use crate::adapters::agent::{
            acp_args_for_candidate, acp_launch_for_agent, acp_spawn_model_for_argv,
            cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
        };
        use crate::models::AgentClient;

        for catalog_id in [
            "claude-opus-5-medium",
            "claude-opus-5-thinking-high",
            "gpt-5.6-sol-high",
            "gpt-5.6-sol-high-fast",
            "composer-2.5",
            "cursor-grok-4.6-high",
        ] {
            assert_eq!(
                cursor_catalog_to_acp_spawn_token(catalog_id),
                catalog_id,
                "spawn argv must keep catalog id {catalog_id} unchanged"
            );
            assert_ne!(
                cursor_catalog_to_acp_in_band_token(catalog_id),
                catalog_id,
                "in-band token must still map {catalog_id}"
            );
        }

        let launch = acp_launch_for_agent(AgentClient::Cursor).expect("cursor");
        assert_eq!(
            acp_spawn_model_for_argv(launch, Some("claude-opus-5-medium")),
            Some("claude-opus-5-medium".to_string())
        );
        assert_eq!(
            acp_args_for_candidate(launch, &["acp"], Some("claude-opus-5-medium")),
            vec![
                "--model".to_string(),
                "claude-opus-5-medium".to_string(),
                "acp".to_string(),
            ]
        );
    }

    #[test]
    fn parse_cursor_model_intent_reads_forum_reasoning_and_skips_auto_default() {
        use crate::adapters::agent::{
            cursor_unspecified_spawn_satisfied, is_unspecified_acp_model, parse_cursor_model_intent,
        };

        assert!(parse_cursor_model_intent("auto").is_none());
        assert!(parse_cursor_model_intent("default").is_none());
        assert!(is_unspecified_acp_model(Some("default")));
        assert!(cursor_unspecified_spawn_satisfied("default"));

        let gpt =
            parse_cursor_model_intent("gpt-5.5[context=272k,reasoning=medium,fast=false]").unwrap();
        assert_eq!(gpt.base, "gpt-5.5");
        assert_eq!(gpt.effort.as_deref(), Some("medium"));
        assert_eq!(gpt.fast, Some(false));

        let claude = parse_cursor_model_intent(
            "claude-opus-4-8[thinking=true,context=300k,effort=high,fast=false]",
        )
        .unwrap();
        assert_eq!(claude.base, "claude-opus-4-8");
        assert_eq!(claude.effort.as_deref(), Some("high"));
        assert_eq!(claude.fast, Some(false));

        let pipe = parse_cursor_model_intent("gpt-5.2|reasoning=medium|fast=false").unwrap();
        assert_eq!(pipe.base, "gpt-5.2");
        assert_eq!(pipe.effort.as_deref(), Some("medium"));
    }

    #[test]
    fn parse_cursor_model_intent_accepts_pipe_form_issue_991() {
        use crate::adapters::agent::parse_cursor_model_intent;

        let intent = parse_cursor_model_intent("grok-4.6|effort=high|fast=false").unwrap();
        assert_eq!(intent.base, "grok-4.6");
        assert_eq!(intent.effort.as_deref(), Some("high"));
        assert_eq!(intent.fast, Some(false));

        let fast = parse_cursor_model_intent("grok-4.6|effort=high|fast=true").unwrap();
        assert_eq!(fast.fast, Some(true));
    }

    // Regression for #991: pipe-form Cursor picks reconstruct catalog ids on spawn argv.
    #[test]
    fn cursor_catalog_pipe_form_reconstructs_spawn_catalog_ids_issue_991() {
        use crate::adapters::agent::{
            acp_args_for_candidate, acp_launch_for_agent, acp_spawn_model_for_argv,
            cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
        };
        use crate::models::AgentClient;

        let cases = [
            ("grok-4.6|effort=high|fast=false", "cursor-grok-4.6-high"),
            (
                "grok-4.6|effort=high|fast=true",
                "cursor-grok-4.6-high-fast",
            ),
            (
                "claude-opus-5|effort=medium|fast=false",
                "claude-opus-5-medium",
            ),
            ("claude-opus-5|effort=high|fast=false", "claude-opus-5-high"),
            ("composer-2.5|fast=true", "composer-2.5-fast"),
            ("composer-2.5|fast=false", "composer-2.5"),
            ("gpt-5.6-sol|effort=high|fast=false", "gpt-5.6-sol-high"),
            ("gpt-5.6-sol|effort=high|fast=true", "gpt-5.6-sol-high-fast"),
        ];
        for (pipe_form, catalog_id) in cases {
            let spawn = cursor_catalog_to_acp_spawn_token(pipe_form);
            assert_eq!(
                spawn, catalog_id,
                "pipe form {pipe_form} must reconstruct {catalog_id}"
            );
            assert!(
                !spawn.contains('['),
                "spawn argv must not synthesize bracket tokens for {pipe_form}"
            );
            assert!(
                !spawn.contains("-thinking-"),
                "spawn argv must not infer thinking variants for {pipe_form}"
            );
        }

        assert_eq!(
            cursor_catalog_to_acp_in_band_token("grok-4.6|effort=high|fast=false"),
            "grok-4.6[effort=high,fast=false]"
        );

        let launch = acp_launch_for_agent(AgentClient::Cursor).expect("cursor");
        assert_eq!(
            acp_spawn_model_for_argv(launch, Some("composer-2.5|fast=true")),
            Some("composer-2.5-fast".to_string())
        );
        assert_eq!(
            acp_args_for_candidate(launch, &["acp"], Some("gpt-5.6-sol|effort=high|fast=false")),
            vec![
                "--model".to_string(),
                "gpt-5.6-sol-high".to_string(),
                "acp".to_string(),
            ]
        );
    }

    // Regression for #984: effort-suffixed Sol catalog ids map to in-band bracket tokens.
    #[test]
    fn cursor_catalog_maps_sol_high_to_acp_in_band_token_issue_984() {
        use crate::adapters::agent::{
            cursor_catalog_to_acp_in_band_token, cursor_catalog_to_acp_spawn_token,
            parse_cursor_model_intent,
        };

        assert_eq!(
            cursor_catalog_to_acp_spawn_token("gpt-5.6-sol-high"),
            "gpt-5.6-sol-high"
        );
        assert_eq!(
            cursor_catalog_to_acp_in_band_token("gpt-5.6-sol-high"),
            "gpt-5.6-sol[effort=high,fast=false]"
        );
        assert_eq!(
            cursor_catalog_to_acp_in_band_token("gpt-5.6-sol-high-fast"),
            "gpt-5.6-sol[effort=high,fast=true]"
        );
        let intent = parse_cursor_model_intent("gpt-5.6-sol-high").unwrap();
        assert_eq!(intent.base, "gpt-5.6-sol");
        assert_eq!(intent.effort.as_deref(), Some("high"));
        assert_eq!(intent.fast, Some(false));
    }

    #[test]
    fn parse_cursor_model_intent_maps_effortless_cursor_grok_to_grok_base() {
        use crate::adapters::agent::parse_cursor_model_intent;

        let base = parse_cursor_model_intent("cursor-grok-4.6").unwrap();
        assert_eq!(base.base, "grok-4.6");
        assert_eq!(base.effort, None);
        assert_eq!(base.fast, Some(false));

        let fast = parse_cursor_model_intent("cursor-grok-4.6-fast").unwrap();
        assert_eq!(fast.base, "grok-4.6");
        assert_eq!(fast.effort, None);
        assert_eq!(fast.fast, Some(true));
    }

    #[test]
    fn parse_cursor_model_intent_keeps_thinking_in_base_issue_1004() {
        use crate::adapters::agent::parse_cursor_model_intent;

        let thinking = parse_cursor_model_intent("claude-opus-5-thinking-high").unwrap();
        assert_eq!(thinking.base, "claude-opus-5-thinking");
        assert_eq!(thinking.effort.as_deref(), Some("high"));

        let plain = parse_cursor_model_intent("claude-opus-5-high").unwrap();
        assert_eq!(plain.base, "claude-opus-5");
        assert_eq!(plain.effort.as_deref(), Some("high"));
    }

    #[test]
    fn cursor_model_intents_match_requires_matching_fast_issue_979() {
        use crate::adapters::{cursor_model_intents_match, parse_cursor_model_intent};

        let desired = parse_cursor_model_intent("cursor-grok-4.6-high").unwrap();
        let non_fast = parse_cursor_model_intent("grok-4.6[effort=high,fast=false]").unwrap();
        let fast = parse_cursor_model_intent("grok-4.6[effort=high,fast=true]").unwrap();
        let composer_fast = parse_cursor_model_intent("composer-2.5[fast=true]").unwrap();
        assert!(cursor_model_intents_match(&desired, &non_fast));
        assert!(!cursor_model_intents_match(&desired, &fast));
        assert!(!cursor_model_intents_match(&desired, &composer_fast));
    }

    #[test]
    fn acp_args_for_candidate_pins_cursor_default_on_unspecified() {
        use crate::adapters::agent::{acp_args_for_candidate, acp_launch_for_agent};
        use crate::models::AgentClient;

        let launch = acp_launch_for_agent(AgentClient::Cursor).expect("cursor");
        assert_eq!(
            acp_args_for_candidate(launch, &["acp"], None),
            vec![
                "--model".to_string(),
                crate::adapters::agent::CURSOR_DEFAULT_SPAWN_MODEL.to_string(),
                "acp".to_string()
            ]
        );
    }

    #[test]
    fn cursor_unspecified_spawn_satisfied_accepts_spawn_default_and_rejects_fast() {
        use crate::adapters::agent::{
            cursor_unspecified_spawn_satisfied, CURSOR_DEFAULT_SPAWN_MODEL,
        };

        assert!(cursor_unspecified_spawn_satisfied(
            CURSOR_DEFAULT_SPAWN_MODEL
        ));
        assert!(cursor_unspecified_spawn_satisfied(
            "grok-4.6[effort=high,fast=false]"
        ));
        assert!(!cursor_unspecified_spawn_satisfied(
            "composer-2.5[fast=true]"
        ));
        assert!(!cursor_unspecified_spawn_satisfied(
            "grok-4.6[effort=high,fast=true]"
        ));
    }

    #[test]
    fn agent_adapter_cursor_launch_uses_selected_model() {
        use crate::models::AgentClient;

        let launch = AgentLaunch {
            worktree_path: "/tmp/worktree".to_string(),
            prompt: String::new(),
            model: Some("composer-2.5".to_string()),
        };

        assert_eq!(
            agent_launch_spec("cursor", AgentClient::Cursor, &launch),
            CommandSpec::new("cursor", ["agent", "--model", "composer-2.5"])
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
