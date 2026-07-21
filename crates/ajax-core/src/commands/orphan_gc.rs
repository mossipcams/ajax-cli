use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use crate::{
    adapters::{git::GitWorktree, CommandRunError, CommandRunner, CommandSpec, GitAdapter},
    config::WorktreePlacement,
    registry::Registry,
};

use super::{context::CommandPlan, CommandContext, CommandError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrphanGcMode {
    AjaxShaped,
    /// Opt-in via `tidy --orphans=all`: also remove unregistered foreign sibling
    /// worktrees (still never deletes non-`ajax/*` branches).
    All,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrphanGcTarget {
    Worktree {
        repo_path: String,
        worktree_path: String,
        branch: Option<String>,
    },
    Branch {
        repo_path: String,
        branch: String,
    },
}

pub fn classify_orphans(
    claimed_paths: &BTreeSet<String>,
    claimed_branches: &BTreeSet<String>,
    worktrees: &[GitWorktree],
    branches: &[String],
    mode: OrphanGcMode,
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> Vec<OrphanGcTarget> {
    match mode {
        OrphanGcMode::AjaxShaped => classify_ajax_shaped_orphans(
            claimed_paths,
            claimed_branches,
            worktrees,
            branches,
            repo_path,
            placement,
        ),
        OrphanGcMode::All => classify_all_orphans(
            claimed_paths,
            claimed_branches,
            worktrees,
            branches,
            repo_path,
            placement,
        ),
    }
}

fn classify_all_orphans(
    claimed_paths: &BTreeSet<String>,
    claimed_branches: &BTreeSet<String>,
    worktrees: &[GitWorktree],
    branches: &[String],
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> Vec<OrphanGcTarget> {
    let mut targets = classify_ajax_shaped_orphans(
        claimed_paths,
        claimed_branches,
        worktrees,
        branches,
        repo_path,
        placement,
    );
    let already: BTreeSet<_> = targets
        .iter()
        .filter_map(|target| match target {
            OrphanGcTarget::Worktree { worktree_path, .. } => Some(worktree_path.clone()),
            OrphanGcTarget::Branch { .. } => None,
        })
        .collect();
    let repo_path_str = repo_path.display().to_string();

    for worktree in worktrees {
        if claimed_paths.contains(&worktree.path) || already.contains(&worktree.path) {
            continue;
        }
        if Path::new(&worktree.path) == repo_path {
            continue;
        }
        if !managed_worktree_path(Path::new(&worktree.path), repo_path, placement) {
            continue;
        }
        if worktree
            .path
            .rsplit('/')
            .next()
            .is_some_and(|name| name == "main")
        {
            continue;
        }
        targets.push(OrphanGcTarget::Worktree {
            repo_path: repo_path_str.clone(),
            worktree_path: worktree.path.clone(),
            branch: worktree.branch.clone(),
        });
    }

    targets
}

fn managed_worktree_path(
    worktree_path: &Path,
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> bool {
    match placement {
        WorktreePlacement::LegacySibling => {
            worktree_path.parent() == Some(legacy_sibling_worktrees_dir(repo_path).as_path())
        }
        WorktreePlacement::Root(root) => worktree_path.starts_with(root),
    }
}

fn classify_ajax_shaped_orphans(
    claimed_paths: &BTreeSet<String>,
    claimed_branches: &BTreeSet<String>,
    worktrees: &[GitWorktree],
    branches: &[String],
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> Vec<OrphanGcTarget> {
    let repo_path_str = repo_path.display().to_string();
    let mut targets = Vec::new();
    let mut branches_from_worktrees = BTreeSet::new();

    for worktree in worktrees {
        if claimed_paths.contains(&worktree.path) {
            continue;
        }
        if Path::new(&worktree.path) == repo_path {
            continue;
        }
        if !ajax_shaped_worktree_path(Path::new(&worktree.path), repo_path, placement) {
            continue;
        }
        if let Some(branch) = worktree.branch.as_ref() {
            branches_from_worktrees.insert(branch.clone());
        }
        targets.push(OrphanGcTarget::Worktree {
            repo_path: repo_path_str.clone(),
            worktree_path: worktree.path.clone(),
            branch: worktree.branch.clone(),
        });
    }

    for branch in branches {
        if !branch.starts_with("ajax/") {
            continue;
        }
        if claimed_branches.contains(branch) {
            continue;
        }
        if branches_from_worktrees.contains(branch) {
            continue;
        }
        targets.push(OrphanGcTarget::Branch {
            repo_path: repo_path_str.clone(),
            branch: branch.clone(),
        });
    }

    targets
}

fn ajax_shaped_worktree_path(
    worktree_path: &Path,
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> bool {
    match placement {
        WorktreePlacement::LegacySibling => {
            let worktrees_dir = legacy_sibling_worktrees_dir(repo_path);
            worktree_path.parent() == Some(worktrees_dir.as_path())
                && worktree_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("ajax-"))
        }
        WorktreePlacement::Root(root) => worktree_path.starts_with(root),
    }
}

fn legacy_sibling_worktrees_dir(repo_path: &Path) -> PathBuf {
    let repo_dir = repo_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("repo");
    repo_path
        .parent()
        .unwrap_or(repo_path)
        .join(format!("{repo_dir}__worktrees"))
}

pub fn orphan_gc_commands(repo_path: &str, targets: &[OrphanGcTarget]) -> Vec<CommandSpec> {
    let git = GitAdapter::new("git");
    let mut commands = Vec::new();

    for target in targets {
        match target {
            OrphanGcTarget::Worktree {
                worktree_path,
                branch,
                ..
            } => {
                commands.push(git.force_remove_worktree(repo_path, worktree_path));
                if let Some(branch) = branch
                    .as_deref()
                    .filter(|branch| branch.starts_with("ajax/"))
                {
                    commands.push(git.force_delete_branch(repo_path, branch));
                }
            }
            OrphanGcTarget::Branch { branch, .. } => {
                commands.push(git.force_delete_branch(repo_path, branch));
            }
        }
    }

    commands
}

pub fn plan_orphan_gc_for_repo(
    claimed_paths: &BTreeSet<String>,
    claimed_branches: &BTreeSet<String>,
    worktrees: &[GitWorktree],
    branches: &[String],
    mode: OrphanGcMode,
    repo_path: &Path,
    placement: &WorktreePlacement,
) -> Vec<OrphanGcTarget> {
    classify_orphans(
        claimed_paths,
        claimed_branches,
        worktrees,
        branches,
        mode,
        repo_path,
        placement,
    )
}

pub fn collect_orphan_gc_commands<R: Registry>(
    context: &CommandContext<R>,
    runner: &mut impl CommandRunner,
    mode: OrphanGcMode,
) -> Result<Vec<CommandSpec>, CommandError> {
    let (claimed_paths, claimed_branches) = claimed_registry_substrate(context);
    let placement = &context.runtime_paths.worktree_placement;
    let git = GitAdapter::new("git");
    let mut commands = Vec::new();

    for repo in &context.config.repos {
        let repo_path = repo.path.as_path();
        let repo_path_str = repo_path.display().to_string();
        let worktrees_output = run_successful_command(runner, &git.list_worktrees(&repo_path_str))?;
        let branches_output = run_successful_command(runner, &git.list_branches(&repo_path_str))?;
        let worktrees = GitAdapter::parse_worktrees(&worktrees_output);
        let branches = GitAdapter::parse_branches(&branches_output);
        let targets = plan_orphan_gc_for_repo(
            &claimed_paths,
            &claimed_branches,
            &worktrees,
            &branches,
            mode,
            repo_path,
            placement,
        );
        commands.extend(orphan_gc_commands(&repo_path_str, &targets));
    }

    Ok(commands)
}

pub fn append_orphan_gc_to_plan<R: Registry>(
    context: &CommandContext<R>,
    plan: &mut CommandPlan,
    runner: &mut impl CommandRunner,
    mode: OrphanGcMode,
) -> Result<(), CommandError> {
    let commands = collect_orphan_gc_commands(context, runner, mode)?;
    if !commands.is_empty() {
        plan.requires_confirmation = true;
        plan.commands.extend(commands);
    }
    Ok(())
}

fn claimed_registry_substrate<R: Registry>(
    context: &CommandContext<R>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut claimed_paths = BTreeSet::new();
    let mut claimed_branches = BTreeSet::new();
    for task in context.registry.list_tasks() {
        claimed_paths.insert(task.worktree_path.display().to_string());
        claimed_branches.insert(task.branch.clone());
    }
    (claimed_paths, claimed_branches)
}

fn run_successful_command(
    runner: &mut impl CommandRunner,
    command: &CommandSpec,
) -> Result<String, CommandError> {
    let output = runner.run(command).map_err(CommandError::CommandRun)?;
    if output.status_code != 0 {
        return Err(CommandError::CommandRun(CommandRunError::NonZeroExit {
            program: command.program.clone(),
            status_code: output.status_code,
            stderr: output.stderr,
            cwd: command.cwd.clone(),
        }));
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WorktreePlacement;

    const REPO_PATH: &str = "/repo/web";

    fn legacy_placement() -> WorktreePlacement {
        WorktreePlacement::LegacySibling
    }

    #[test]
    fn classify_orphans_lists_ajax_branch_not_claimed_by_registry() {
        let claimed_paths = BTreeSet::new();
        let claimed_branches = BTreeSet::new();
        let branches = vec!["ajax/hotbar".to_string(), "main".to_string()];

        let targets = classify_orphans(
            &claimed_paths,
            &claimed_branches,
            &[],
            &branches,
            OrphanGcMode::AjaxShaped,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );

        assert_eq!(
            targets,
            vec![OrphanGcTarget::Branch {
                repo_path: REPO_PATH.to_string(),
                branch: "ajax/hotbar".to_string(),
            }]
        );
    }

    #[test]
    fn classify_orphans_lists_ajax_worktree_path_not_claimed() {
        let worktree_path = "/repo/web__worktrees/ajax-xterm-implementation".to_string();
        let worktrees = vec![GitWorktree {
            path: worktree_path.clone(),
            branch: Some("ajax/xterm-implementation".to_string()),
        }];

        let targets = classify_orphans(
            &BTreeSet::new(),
            &BTreeSet::new(),
            &worktrees,
            &[],
            OrphanGcMode::AjaxShaped,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );
        assert_eq!(
            targets,
            vec![OrphanGcTarget::Worktree {
                repo_path: REPO_PATH.to_string(),
                worktree_path,
                branch: Some("ajax/xterm-implementation".to_string()),
            }]
        );

        let mut claimed_paths = BTreeSet::new();
        claimed_paths.insert("/repo/web__worktrees/ajax-xterm-implementation".to_string());
        let targets = classify_orphans(
            &claimed_paths,
            &BTreeSet::new(),
            &worktrees,
            &[],
            OrphanGcMode::AjaxShaped,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );
        assert!(targets.is_empty());
    }

    #[test]
    fn classify_orphans_skips_foreign_sibling_worktree() {
        let worktrees = vec![GitWorktree {
            path: "/repo/web__worktrees/fix-web-cf-shell-cache".to_string(),
            branch: Some("fix/web-cf-shell-cache".to_string()),
        }];

        let targets = classify_orphans(
            &BTreeSet::new(),
            &BTreeSet::new(),
            &worktrees,
            &[],
            OrphanGcMode::AjaxShaped,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );

        assert!(targets.is_empty());
    }

    #[test]
    fn classify_orphans_all_includes_foreign_sibling_worktree() {
        let worktree_path = "/repo/web__worktrees/fix-web-cf-shell-cache".to_string();
        let worktrees = vec![GitWorktree {
            path: worktree_path.clone(),
            branch: Some("fix/web-cf-shell-cache".to_string()),
        }];

        let targets = classify_orphans(
            &BTreeSet::new(),
            &BTreeSet::new(),
            &worktrees,
            &[],
            OrphanGcMode::All,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );

        assert_eq!(
            targets,
            vec![OrphanGcTarget::Worktree {
                repo_path: REPO_PATH.to_string(),
                worktree_path,
                branch: Some("fix/web-cf-shell-cache".to_string()),
            }]
        );
    }

    #[test]
    fn classify_orphans_all_skips_main_worktree() {
        let worktrees = vec![GitWorktree {
            path: "/repo/web__worktrees/main".to_string(),
            branch: Some("main".to_string()),
        }];

        let targets = classify_orphans(
            &BTreeSet::new(),
            &BTreeSet::new(),
            &worktrees,
            &[],
            OrphanGcMode::All,
            Path::new(REPO_PATH),
            &legacy_placement(),
        );

        assert!(targets.is_empty());
    }

    #[test]
    fn orphan_gc_commands_force_remove_worktree_then_delete_branch_d() {
        let targets = vec![OrphanGcTarget::Worktree {
            repo_path: REPO_PATH.to_string(),
            worktree_path: "/repo/web__worktrees/ajax-xterm-implementation".to_string(),
            branch: Some("ajax/xterm-implementation".to_string()),
        }];

        let commands = orphan_gc_commands(REPO_PATH, &targets);

        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].args,
            [
                "-C",
                REPO_PATH,
                "worktree",
                "remove",
                "--force",
                "/repo/web__worktrees/ajax-xterm-implementation"
            ]
        );
        assert_eq!(
            commands[1].args,
            ["-C", REPO_PATH, "branch", "-D", "ajax/xterm-implementation"]
        );
    }
}
