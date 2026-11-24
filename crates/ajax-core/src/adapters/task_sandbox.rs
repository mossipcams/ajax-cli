use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::docker_sandbox::task_key;
use super::sandbox::sandbox_enabled_from;

const UNSANDBOXED_DIR: &str = ".config/ajax/unsandboxed";
const HOME_UNSET: &str =
    "per-task sandbox setting needs HOME to store the opt-out; set HOME or AJAX_SANDBOX=off";

#[derive(Debug, PartialEq, Eq)]
pub struct TaskSandbox {
    pub enabled: bool,
    pub locked_by_env: bool,
}

fn marker_dir(home: &Path) -> PathBuf {
    home.join(UNSANDBOXED_DIR)
}

fn marker_path(home: &Path, worktree: &Path) -> PathBuf {
    marker_dir(home).join(task_key(worktree))
}

fn home_from_env() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

fn sandbox_enabled_for_from(home: Option<&Path>, env_value: Option<&str>, worktree: &Path) -> bool {
    sandbox_enabled_from(env_value)
        && !home.is_some_and(|home| marker_path(home, worktree).exists())
}

pub fn task_sandbox_from(
    home: Option<&Path>,
    env_value: Option<&str>,
    worktree: &Path,
) -> TaskSandbox {
    TaskSandbox {
        enabled: sandbox_enabled_for_from(home, env_value, worktree),
        locked_by_env: !sandbox_enabled_from(env_value),
    }
}

pub fn set_task_sandbox_from(
    home: Option<&Path>,
    worktree: &Path,
    enabled: bool,
) -> io::Result<()> {
    let home = home.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, HOME_UNSET))?;
    let marker = marker_path(home, worktree);
    if enabled {
        return match fs::remove_file(&marker) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        };
    }
    fs::create_dir_all(marker_dir(home))?;
    fs::write(marker, worktree.as_os_str().as_encoded_bytes())
}

pub fn sandbox_enabled_for(worktree: &Path) -> bool {
    sandbox_enabled_for_from(
        home_from_env().as_deref(),
        env::var("AJAX_SANDBOX").ok().as_deref(),
        worktree,
    )
}

pub fn task_sandbox(worktree: &Path) -> TaskSandbox {
    task_sandbox_from(
        home_from_env().as_deref(),
        env::var("AJAX_SANDBOX").ok().as_deref(),
        worktree,
    )
}

pub fn set_task_sandbox(worktree: &Path, enabled: bool) -> io::Result<()> {
    set_task_sandbox_from(home_from_env().as_deref(), worktree, enabled)
}

pub fn remove_task_sandbox_marker(worktree: &Path) {
    let _ = set_task_sandbox(worktree, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/task-sandbox-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const WORKTREE: &str = "/Users/matt/Projects/ajax-cli__worktrees/ajax-fix";

    #[test]
    fn sandbox_enabled_for_table_over_env_and_marker() {
        let home = scratch("table-home");
        let worktree = Path::new(WORKTREE);
        let cases = [
            (None, false, true),
            (Some("off"), false, false),
            (Some("on"), false, true),
            (None, true, false),
            (Some("off"), true, false),
            (Some("on"), true, false),
        ];
        for (env_value, marker_present, expected) in cases {
            let _ = fs::remove_dir_all(marker_dir(&home));
            if marker_present {
                set_task_sandbox_from(Some(&home), worktree, false).unwrap();
            }
            assert_eq!(
                sandbox_enabled_for_from(Some(&home), env_value, worktree),
                expected,
                "env={env_value:?} marker_present={marker_present}"
            );
        }
    }

    #[test]
    fn task_sandbox_reports_locked_by_env_only_when_env_turns_sandbox_off() {
        let home = scratch("task-sandbox-home");
        let worktree = Path::new(WORKTREE);
        assert_eq!(
            task_sandbox_from(Some(&home), None, worktree),
            TaskSandbox {
                enabled: true,
                locked_by_env: false
            }
        );
        assert_eq!(
            task_sandbox_from(Some(&home), Some("on"), worktree),
            TaskSandbox {
                enabled: true,
                locked_by_env: false
            }
        );
        assert_eq!(
            task_sandbox_from(Some(&home), Some("off"), worktree),
            TaskSandbox {
                enabled: false,
                locked_by_env: true
            }
        );
        set_task_sandbox_from(Some(&home), worktree, false).unwrap();
        assert_eq!(
            task_sandbox_from(Some(&home), Some("off"), worktree),
            TaskSandbox {
                enabled: false,
                locked_by_env: true
            }
        );
        assert_eq!(
            task_sandbox_from(Some(&home), None, worktree),
            TaskSandbox {
                enabled: false,
                locked_by_env: false
            }
        );
    }

    #[test]
    fn home_unset_fails_closed_to_sandboxed_and_cannot_store_opt_out() {
        let worktree = Path::new(WORKTREE);
        assert!(sandbox_enabled_for_from(None, None, worktree));
        assert_eq!(
            task_sandbox_from(None, None, worktree),
            TaskSandbox {
                enabled: true,
                locked_by_env: false
            }
        );
        let err = set_task_sandbox_from(None, worktree, false).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(set_task_sandbox_from(None, worktree, true).is_err());
    }

    #[test]
    fn set_task_sandbox_round_trip_writes_and_removes_marker() {
        let home = scratch("round-trip-home");
        let worktree = Path::new(WORKTREE);
        let marker = marker_path(&home, worktree);

        set_task_sandbox_from(Some(&home), worktree, false).unwrap();
        assert_eq!(
            fs::read(&marker).unwrap(),
            worktree.as_os_str().as_encoded_bytes()
        );
        assert!(!sandbox_enabled_for_from(Some(&home), None, worktree));

        set_task_sandbox_from(Some(&home), worktree, true).unwrap();
        assert!(!marker.exists());
        assert!(sandbox_enabled_for_from(Some(&home), None, worktree));

        set_task_sandbox_from(Some(&home), worktree, true).unwrap();
    }

    #[test]
    fn markers_for_different_worktrees_are_independent() {
        let home = scratch("independent-home");
        let first = Path::new("/Users/matt/Projects/ajax-cli__worktrees/ajax-one");
        let second = Path::new("/Users/matt/Projects/ajax-cli__worktrees/ajax-two");

        set_task_sandbox_from(Some(&home), first, false).unwrap();
        assert!(!sandbox_enabled_for_from(Some(&home), None, first));
        assert!(sandbox_enabled_for_from(Some(&home), None, second));

        set_task_sandbox_from(Some(&home), second, false).unwrap();
        set_task_sandbox_from(Some(&home), first, true).unwrap();
        assert!(sandbox_enabled_for_from(Some(&home), None, first));
        assert!(!sandbox_enabled_for_from(Some(&home), None, second));
    }
}
