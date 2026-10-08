use std::collections::BTreeMap;

use crate::{
    adapters::{CommandOutput, CommandRunError, CommandRunner, CommandSpec, TmuxAdapter},
    commands::{self, CommandContext, CommandError},
    registry::Registry,
};

use crate::task_operations::drop_task::{complete_drop_task_operation, DropTaskCompletion};

pub fn execute_sweep_cleanup_operation<R: Registry>(
    context: &mut CommandContext<R>,
    confirmed: bool,
    runner: &mut impl CommandRunner,
    orphan_mode: Option<commands::OrphanGcMode>,
) -> Result<(Vec<CommandOutput>, bool), (CommandError, bool)> {
    let mut outputs = Vec::new();
    let mut state_changed = false;
    for command in sweep_trash_commands_guarded(context) {
        let output = runner
            .run(&command)
            .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
        if output.status_code != 0 {
            return Err((
                CommandError::CommandRun(CommandRunError::NonZeroExit {
                    program: command.program.clone(),
                    status_code: output.status_code,
                    stderr: output.stderr,
                    cwd: command.cwd.clone(),
                }),
                state_changed,
            ));
        }
        outputs.push(output);
    }
    let candidates = commands::sweep_cleanup_candidates(context);
    let tmux = TmuxAdapter::new("tmux");
    let shared_sessions = runner
        .run(&tmux.list_sessions())
        .ok()
        .filter(|output| output.status_code == 0)
        .map(|output| output.stdout);
    let mut repo_observations = BTreeMap::<String, commands::RepoDropObservationCache>::new();

    for candidate in &candidates {
        let plan = commands::clean_task_plan(context, candidate)
            .map_err(|error| (error, state_changed))?;
        if !plan.blocked_reasons.is_empty() {
            return Err((
                CommandError::PlanBlocked(plan.blocked_reasons),
                state_changed,
            ));
        }
        if plan.requires_confirmation && !confirmed {
            return Err((CommandError::ConfirmationRequired, state_changed));
        }

        for command in &plan.commands {
            let output = runner
                .run(command)
                .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
            if output.status_code != 0 {
                return Err((
                    CommandError::CommandRun(CommandRunError::NonZeroExit {
                        program: command.program.clone(),
                        status_code: output.status_code,
                        stderr: output.stderr.clone(),
                        cwd: command.cwd.clone(),
                    }),
                    state_changed,
                ));
            }
            outputs.push(output);
            state_changed |=
                commands::mark_task_cleanup_step_completed(context, candidate, command)
                    .map_err(|error| (error, state_changed))?;
        }

        let task = context
            .registry
            .list_tasks()
            .into_iter()
            .find(|task| task.qualified_handle() == *candidate)
            .cloned()
            .ok_or_else(|| (CommandError::TaskNotFound(candidate.clone()), state_changed))?;
        let repo_cache = repo_observations.entry(task.repo.clone()).or_default();
        let observation = commands::observe_drop_resources_with_cache(
            context,
            &task,
            runner,
            shared_sessions.as_deref(),
            repo_cache,
        )
        .map_err(|error| (error, state_changed))?;
        match complete_drop_task_operation(context, candidate, &observation)
            .map_err(|error| (error, state_changed))?
        {
            DropTaskCompletion::Removed | DropTaskCompletion::TeardownIncomplete { .. } => {
                state_changed = true;
            }
        }
    }

    if let Some(mode) = orphan_mode {
        let orphan_commands = commands::collect_orphan_gc_commands(context, runner, mode)
            .map_err(|error| (error, state_changed))?;
        if !orphan_commands.is_empty() && !confirmed {
            return Err((CommandError::ConfirmationRequired, state_changed));
        }
        for command in &orphan_commands {
            let output = runner
                .run(command)
                .map_err(|error| (CommandError::CommandRun(error), state_changed))?;
            if output.status_code != 0 {
                return Err((
                    CommandError::CommandRun(CommandRunError::NonZeroExit {
                        program: command.program.clone(),
                        status_code: output.status_code,
                        stderr: output.stderr,
                        cwd: command.cwd.clone(),
                    }),
                    state_changed,
                ));
            }
            outputs.push(output);
        }
    }

    Ok((outputs, state_changed))
}

fn sweep_trash_commands_guarded<R: Registry>(context: &CommandContext<R>) -> Vec<CommandSpec> {
    commands::sweep_trash_commands(context)
        .into_iter()
        .flat_map(|command| {
            let Some(trash_dir) = command.args.last().map(std::path::PathBuf::from) else {
                return vec![command];
            };
            let Ok(entries) = std::fs::read_dir(trash_dir) else {
                return vec![command];
            };
            let mut has_repository_entry = false;
            let mut plain_entries: Vec<std::path::PathBuf> = Vec::new();
            for entry in entries.flatten() {
                if entry.path().is_dir() && entry.path().join(".git").is_dir() {
                    has_repository_entry = true;
                } else {
                    plain_entries.push(entry.path());
                }
            }
            if !has_repository_entry {
                return vec![command];
            }
            plain_entries
                .into_iter()
                .map(|entry_path| {
                    CommandSpec::new(
                        "sh",
                        [
                            "-c",
                            "if [ -e \"$1\" ]; then find \"$1\" -mindepth 1 -maxdepth 1 -mmin +60 -exec rm -rf {} +; fi",
                            "ajax-trash-sweep",
                            &entry_path.display().to_string(),
                        ],
                    )
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod trash_sweep_guard_tests {
    use super::*;
    use crate::config::{Config, ManagedRepo};
    use crate::models::{AgentClient, Task, TaskId};
    use crate::registry::InMemoryRegistry;

    fn context_with_worktree_root(root: &std::path::Path) -> CommandContext<InMemoryRegistry> {
        let repo = root.join("repo");
        let mut context = CommandContext::new(
            Config {
                repos: vec![ManagedRepo::new("web", repo.display().to_string(), "main")],
                ..Config::default()
            },
            InMemoryRegistry::default(),
        );
        let task = Task::new(
            TaskId::new("web/fix-login"),
            "web",
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            root.join("wt").display().to_string(),
            "ajax-web-fix-login",
            "task",
            AgentClient::Codex,
        );
        context.registry.create_task(task).unwrap();
        context
    }

    #[test]
    fn sweep_skips_trash_entries_holding_full_repositories() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("ajax-sweep-guard-{}-{nanos}", std::process::id()));
        let trash_dir = root.join(".ajax-trash");
        std::fs::create_dir_all(trash_dir.join("repo-entry/.git")).unwrap();
        std::fs::create_dir_all(trash_dir.join("plain-entry")).unwrap();

        let context = context_with_worktree_root(&root);
        let commands = sweep_trash_commands_guarded(&context);

        assert!(
            !commands
                .iter()
                .any(|command| { command.args.iter().any(|arg| arg.contains("repo-entry")) }),
            "no sweep command may target a repository entry, got {:?}",
            commands
                .iter()
                .map(|command| command.args.clone())
                .collect::<Vec<_>>()
        );
        assert!(
            commands.iter().any(|command| {
                command.program == "sh"
                    && command.args.iter().any(|arg| arg.ends_with("plain-entry"))
            }),
            "a plain entry must still be swept, got {:?}",
            commands
                .iter()
                .map(|command| command.args.clone())
                .collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
