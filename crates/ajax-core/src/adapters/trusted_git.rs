use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

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

pub fn default_store() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/ajax/nono/trusted-worktrees"))
}

pub fn apply_env(command: &mut Command, trusted: &TrustedGit) {
    command
        .env("GIT_DIR", &trusted.git_dir)
        .env("GIT_COMMON_DIR", &trusted.common_dir)
        .env("GIT_WORK_TREE", &trusted.work_tree);
}

pub fn disable_hooks(command: &mut Command) {
    command
        .env("GIT_CONFIG_COUNT", "2")
        .env("GIT_CONFIG_KEY_0", "core.hooksPath")
        .env("GIT_CONFIG_VALUE_0", "/dev/null")
        .env("GIT_CONFIG_KEY_1", "core.fsmonitor")
        .env("GIT_CONFIG_VALUE_1", "false");
}

pub fn harden(command: &mut Command, target: &Path) {
    disable_hooks(command);
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

    #[test]
    fn disable_hooks_stops_planted_post_checkout_hook() {
        let root = scratch("planted-hook");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        fs::write(repo.join("README.md"), "init\n").unwrap();
        git(&repo, &["add", "README.md"]);
        git(&repo, &["commit", "-q", "-m", "init"]);

        let hooks = root.join("evil-hooks");
        fs::create_dir_all(&hooks).unwrap();
        let marker = root.join("PWNED-hook");
        let hook = hooks.join("post-checkout");
        fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        git(
            &repo,
            &["config", "core.hooksPath", hooks.to_str().unwrap()],
        );

        let switch = |branch: &str| {
            let mut command = Command::new("git");
            command
                .arg("-C")
                .arg(&repo)
                .args(["switch", "-q", "-c", branch]);
            for (name, _) in env::vars_os() {
                if name.to_string_lossy().starts_with("GIT_") {
                    command.env_remove(name);
                }
            }
            command
        };

        let mut exploited = switch("exploited");
        assert!(exploited.status().unwrap().success());
        assert!(
            marker.exists(),
            "planted post-checkout hook did not run; regression test does not exercise the exploit"
        );
        fs::remove_file(&marker).unwrap();

        let mut defended = switch("defended");
        disable_hooks(&mut defended);
        assert!(defended.status().unwrap().success());
        assert!(!marker.exists(), "disable_hooks still ran planted hook");
    }
}
