//! Tells model-router harness children where to drop their ACP event stream.
//!
//! Ajax sets [`ENV_VAR`] on every spawned ACP child (and therefore on
//! `run-delegate`, which inherits the environment through Bash) to a
//! per-worktree directory under the web-session state root. Children write
//! their delegate JSONL into that directory; Ajax reads it back into Chat.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Web-session state root; `None` until the session layer wires it up.
static STATE_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Environment variable carrying the per-worktree events directory.
pub(crate) const ENV_VAR: &str = "AJAX_DELEGATE_EVENTS_DIR";

/// Remember the state root for later [`apply_env`] calls. The first call
/// wins; later calls are ignored so a restart cannot silently retarget an
/// in-flight session's children at a new location.
pub(crate) fn set_state_dir(dir: &Path) {
    let _ = STATE_DIR.set(dir.to_path_buf());
}

/// Per-worktree events directory, or `None` while no state dir is set so
/// callers leave the environment untouched (unchanged behavior).
pub(crate) fn events_dir_for(worktree: &Path) -> Option<PathBuf> {
    STATE_DIR
        .get()
        .map(|state_dir| events_dir_in(state_dir, worktree))
}

/// The pure path computation behind [`events_dir_for`], with the state dir
/// made explicit so tests never need the global [`STATE_DIR`].
fn events_dir_in(state_dir: &Path, worktree: &Path) -> PathBuf {
    state_dir
        .join("web-session")
        .join("delegates")
        .join(worktree_key(worktree))
}

/// Deterministic filesystem-safe encoding of a worktree path, done inline so
/// this adapter does not depend on the web-session store: it percent-escapes
/// `%` and `/`, so the result contains no path separators and is injective
/// (distinct paths give distinct keys), with no new dependencies.
fn worktree_key(worktree: &Path) -> String {
    worktree
        .to_string_lossy()
        .replace('%', "%25")
        .replace('/', "%2F")
}

/// Append the delegate events env var to `command` when a state dir is set:
/// creates `<state_dir>/web-session/delegates/<key>` (ignoring, with a log,
/// any failure) and sets [`ENV_VAR`] to it. A no-op while unset.
pub(crate) fn apply_env(command: &mut Command, worktree: &Path) {
    apply_env_to(command, events_dir_for(worktree));
}

/// The env-var wiring with the dir resolved by the caller, so tests never
/// need the global [`STATE_DIR`].
fn apply_env_to(command: &mut Command, dir: Option<PathBuf>) {
    let Some(dir) = dir else {
        return;
    };
    if let Err(error) = std::fs::create_dir_all(&dir) {
        tracing::warn!(?dir, %error, "failed to create delegate events dir");
    }
    command.env(ENV_VAR, &dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_key_is_deterministic_and_safe() {
        let a = worktree_key(Path::new("/tmp/wt/ajax-cli"));
        let b = worktree_key(Path::new("/tmp/wt/ajax-cli"));
        assert_eq!(a, b, "same path must encode the same");
        assert!(
            !a.contains('/'),
            "key must not contain a path separator, got {a}"
        );
    }

    #[test]
    fn worktree_key_distinct_for_confusable_paths() {
        assert_ne!(
            worktree_key(Path::new("/a/b")),
            worktree_key(Path::new("/a_b")),
            "separator and underscore must not collide"
        );
        assert_ne!(worktree_key(Path::new("x")), worktree_key(Path::new("y")));
    }

    #[test]
    fn events_dir_in_builds_expected_path() {
        let dir = events_dir_in(Path::new("/state"), Path::new("/wt/one"));
        assert_eq!(
            dir,
            PathBuf::from("/state/web-session/delegates").join(worktree_key(Path::new("/wt/one")))
        );
    }

    #[test]
    fn apply_env_to_is_noop_without_dir() {
        let mut command = Command::new("true");
        apply_env_to(&mut command, None);
        assert!(
            !command.get_envs().any(|(key, _)| key == ENV_VAR),
            "no {ENV_VAR} when there is no dir"
        );
    }

    #[test]
    fn apply_env_to_sets_var_and_creates_dir() {
        let state =
            std::env::temp_dir().join(format!("ajax-delegate-events-{}", std::process::id()));
        let dir = events_dir_in(&state, Path::new("/wt/two"));

        let mut command = Command::new("true");
        apply_env_to(&mut command, Some(dir.clone()));
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == ENV_VAR && value == Some(dir.as_os_str())),
            "apply_env_to must set {ENV_VAR}"
        );
        assert!(dir.is_dir(), "apply_env_to should create the dir");

        let _ = std::fs::remove_dir_all(&state);
    }
}
