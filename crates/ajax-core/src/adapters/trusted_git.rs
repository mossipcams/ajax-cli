//! Ajax-owned records of verified linked worktrees.
//!
//! A sandboxed agent can rewrite a worktree's `.git` file and the `gitdir` back-link in its
//! admin dir, so host-side git must not read repository location from those. The sandbox
//! records the worktree's verified admin and common dirs here at launch, in a store no sandbox
//! grant covers, and host-side git is pinned to those paths through the environment.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Verified repository locations for one linked worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedGit {
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    pub work_tree: PathBuf,
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a_64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

fn record_path(store: &Path, work_tree: &Path) -> PathBuf {
    let hash = fnv1a_64(work_tree.as_os_str().as_encoded_bytes());
    store.join(format!("{hash:016x}"))
}

/// Writes the record for `trusted.work_tree` into `store`, replacing any previous record.
///
/// Paths are canonicalized first. The record is written to a unique temp file and renamed
/// into place, so a concurrent lookup never reads a partial record.
pub fn record(store: &Path, trusted: &TrustedGit) -> io::Result<()> {
    let work_tree = fs::canonicalize(&trusted.work_tree)?;
    let git_dir = fs::canonicalize(&trusted.git_dir)?;
    let common_dir = fs::canonicalize(&trusted.common_dir)?;
    let paths = [&work_tree, &git_dir, &common_dir];
    if paths
        .iter()
        .any(|path| path.as_os_str().as_encoded_bytes().contains(&b'\n'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "trusted git paths must not contain newlines",
        ));
    }
    fs::create_dir_all(store)?;
    let path = record_path(store, &work_tree);
    let content = format!(
        "{}\n{}\n{}\n",
        work_tree.display(),
        git_dir.display(),
        common_dir.display()
    );
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let tmp = store.join(format!(".record.{}.{nanos}.tmp", std::process::id()));
    fs::write(&tmp, content)?;
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(())
}

/// Returns the record of the nearest recorded worktree containing `target`.
///
/// A record only counts when its stored work tree equals the matching ancestor exactly, so a
/// hash collision cannot pin git to another worktree's metadata. Once it matches it is always
/// returned, even when its admin dir is gone. The agent can delete that admin dir, so falling
/// back to "no record" would let host git read the agent-controlled `.git` file; pinned git
/// instead fails closed with "not a git repository". Dropped worktrees are unrecorded by
/// [`forget`], so a reused path is not pinned to a pruned admin dir.
pub fn lookup(store: &Path, target: &Path) -> Option<TrustedGit> {
    let target = fs::canonicalize(target).ok()?;
    target.ancestors().find_map(|dir| {
        let content = fs::read_to_string(record_path(store, dir)).ok()?;
        let mut lines = content.lines();
        let trusted = TrustedGit {
            work_tree: PathBuf::from(lines.next()?),
            git_dir: PathBuf::from(lines.next()?),
            common_dir: PathBuf::from(lines.next()?),
        };
        (trusted.work_tree == dir).then_some(trusted)
    })
}

/// Removes the record for `work_tree` from `store`. A missing record is not an error.
///
/// The record is looked up under the path as given and under its canonical form. Once the
/// worktree is gone it cannot be canonicalized, so the canonicalized parent joined with the file
/// name is used instead, which is how [`record`] keyed it while the worktree existed.
pub fn forget(store: &Path, work_tree: &Path) -> io::Result<()> {
    let mut keys = vec![work_tree.to_path_buf()];
    match fs::canonicalize(work_tree) {
        Ok(canonical) => keys.push(canonical),
        Err(_) => {
            if let (Some(parent), Some(name)) = (work_tree.parent(), work_tree.file_name()) {
                if let Ok(parent) = fs::canonicalize(parent) {
                    keys.push(parent.join(name));
                }
            }
        }
    }
    for key in keys {
        match fs::remove_file(record_path(store, &key)) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(())
}

/// Trusted-worktree store, `$HOME/.config/ajax/nono/trusted-worktrees`.
///
/// Sits under the Ajax profile dir and outside every sandbox grant, so the agent cannot rewrite
/// records.
pub fn default_store() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/ajax/nono/trusted-worktrees"))
}

/// Points `command` at the trusted git dirs, so git ignores the worktree's `.git` file.
///
/// Also disables fsmonitor at command-line config level (git >= 2.31), so repository-local
/// config an agent plants cannot run a hook program. Ajax never needs fsmonitor.
pub fn apply_env(command: &mut Command, trusted: &TrustedGit) {
    command
        .env("GIT_DIR", &trusted.git_dir)
        .env("GIT_COMMON_DIR", &trusted.common_dir)
        .env("GIT_WORK_TREE", &trusted.work_tree)
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_0", "false");
}

/// Applies [`apply_env`] when `target` lies in a recorded worktree; otherwise leaves `command`
/// unchanged.
pub fn harden(command: &mut Command, target: &Path) {
    if let Some(trusted) = default_store().and_then(|store| lookup(&store, target)) {
        apply_env(command, &trusted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/trusted-git-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.email=trusted@example.invalid",
                "-c",
                "user.name=trusted",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args);
        for (name, _) in env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        let status = command.status().unwrap();
        assert!(status.success(), "git {args:?} failed in {}", dir.display());
    }

    /// Creates `root/repo` with one committed file and a linked worktree at `root/wt`.
    fn linked_worktree(root: &Path) -> (PathBuf, TrustedGit) {
        let repo = root.join("repo");
        let worktree = root.join("wt");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("README.md"), "init\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                worktree.to_str().unwrap(),
            ],
        );
        let common = fs::canonicalize(repo.join(".git")).unwrap();
        let trusted = TrustedGit {
            git_dir: common.join("worktrees").join("wt"),
            common_dir: common,
            work_tree: fs::canonicalize(&worktree).unwrap(),
        };
        (worktree, trusted)
    }

    fn status_command(worktree: &Path) -> Command {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(worktree)
            .args(["status", "--porcelain=v1"]);
        for (name, _) in env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        command
    }

    #[test]
    fn record_and_lookup_round_trip_from_subdirectory() {
        let root = scratch("round-trip");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        let sub = worktree.join("sub");
        fs::create_dir_all(&sub).unwrap();

        record(&store, &trusted).unwrap();

        assert_eq!(lookup(&store, &worktree), Some(trusted.clone()));
        assert_eq!(lookup(&store, &sub), Some(trusted));
    }

    #[test]
    fn lookup_returns_none_for_unrecorded_directory() {
        let root = scratch("unrecorded");
        let (_, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();
        let other = root.join("other");
        fs::create_dir_all(&other).unwrap();

        assert_eq!(lookup(&store, &other), None);
        assert_eq!(
            lookup(&root.join("store-missing"), &trusted.work_tree),
            None
        );
    }

    #[test]
    fn lookup_ignores_record_whose_work_tree_does_not_match() {
        let root = scratch("collision");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        fs::create_dir_all(&store).unwrap();
        let victim = root.join("victim");
        fs::create_dir_all(&victim).unwrap();
        let forged = format!(
            "{}\n{}\n{}\n",
            worktree.display(),
            trusted.git_dir.display(),
            trusted.common_dir.display()
        );
        let victim_canonical = fs::canonicalize(&victim).unwrap();
        fs::write(record_path(&store, &victim_canonical), forged).unwrap();

        assert_eq!(lookup(&store, &victim), None);
    }

    #[test]
    fn lookup_pins_record_whose_admin_dir_was_removed() {
        let root = scratch("pruned-admin-dir");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();
        assert_eq!(lookup(&store, &worktree), Some(trusted.clone()));

        fs::remove_dir_all(&trusted.git_dir).unwrap();

        assert_eq!(lookup(&store, &worktree), Some(trusted));
    }

    #[test]
    fn forget_removes_record_and_tolerates_absence() {
        let root = scratch("forget");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();

        forget(&store, &worktree).unwrap();
        assert_eq!(lookup(&store, &worktree), None);
        forget(&store, &worktree).unwrap();
    }

    #[test]
    fn forget_removes_record_of_already_deleted_worktree() {
        let root = scratch("forget-deleted");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();

        fs::remove_dir_all(&worktree).unwrap();
        forget(&store, &worktree).unwrap();

        assert!(!record_path(&store, &trusted.work_tree).exists());
    }

    /// Issue #1387: an agent that deletes its admin dir and rewrites `.git` must not get host git
    /// to read its planted config. The pinned record fails closed instead.
    #[test]
    fn pinned_env_fails_closed_when_admin_dir_removed_and_git_file_forged() {
        let root = scratch("regression-1387-admin-removed");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();

        let evil = worktree.join("evil");
        fs::create_dir_all(&evil).unwrap();
        git(&evil, &["init", "-q"]);
        fs::write(evil.join("planted.txt"), "planted\n").unwrap();
        git(&evil, &["add", "planted.txt"]);
        git(&evil, &["commit", "-q", "-m", "planted"]);
        let marker = root.join("PWNED-admin-removed");
        git(
            &evil,
            &[
                "config",
                "core.fsmonitor",
                &format!("touch '{}'; false", marker.display()),
            ],
        );
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", evil.join(".git").display()),
        )
        .unwrap();
        fs::remove_dir_all(&trusted.git_dir).unwrap();

        let mut defended = status_command(&worktree);
        apply_env(&mut defended, &lookup(&store, &worktree).unwrap());
        let output = defended.output().unwrap();
        assert!(
            !output.status.success(),
            "pinned git read a deleted admin dir"
        );
        assert!(!marker.exists(), "pinned env still ran agent fsmonitor");

        let mut exploited = status_command(&worktree);
        exploited.output().unwrap();
        assert!(
            marker.exists(),
            "forged worktree link did not run fsmonitor; regression test does not exercise the exploit"
        );
    }

    #[test]
    fn record_rejects_missing_work_tree() {
        let root = scratch("missing-work-tree");
        let trusted = TrustedGit {
            git_dir: root.clone(),
            common_dir: root.clone(),
            work_tree: root.join("absent"),
        };

        assert!(record(&root.join("store"), &trusted).is_err());
    }

    /// Issue #1387: an agent that rewrites `.git` must not steer host-side git to its config.
    #[test]
    fn trusted_env_defeats_forged_worktree_link_that_runs_fsmonitor() {
        let root = scratch("regression-1387");
        let (worktree, trusted) = linked_worktree(&root);
        let store = root.join("store");
        record(&store, &trusted).unwrap();

        let evil = worktree.join("evil");
        fs::create_dir_all(&evil).unwrap();
        git(&evil, &["init", "-q"]);
        fs::write(evil.join("planted.txt"), "planted\n").unwrap();
        git(&evil, &["add", "planted.txt"]);
        git(&evil, &["commit", "-q", "-m", "planted"]);
        let marker = root.join("PWNED");
        git(
            &evil,
            &[
                "config",
                "core.fsmonitor",
                &format!("touch '{}'; false", marker.display()),
            ],
        );
        fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", evil.join(".git").display()),
        )
        .unwrap();

        let mut defended = status_command(&worktree);
        apply_env(&mut defended, &lookup(&store, &worktree).unwrap());
        defended.output().unwrap();
        assert!(!marker.exists(), "trusted env still ran agent fsmonitor");

        let mut exploited = status_command(&worktree);
        exploited.output().unwrap();
        assert!(
            marker.exists(),
            "forged worktree link did not run fsmonitor; regression test does not exercise the exploit"
        );
    }
}
