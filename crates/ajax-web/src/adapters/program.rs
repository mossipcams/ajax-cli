//! Resolve harness executables for a long-lived server process.
//!
//! `ajax-cli web` runs under tmux or a service manager, so its `PATH` is
//! whatever that supervisor had when it started. Version managers move
//! binaries — an nvm switch leaves `codex`, `pi`, and the ACP bridges under a
//! node version the daemon's `PATH` no longer names — and the operator then
//! sees a harness that simply "has no models". Fall back to the login shell,
//! which is where those tools were installed to be visible.

use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
};

static RESOLVED: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);

/// Absolute path for `program`, or `None` when it is not installed anywhere the
/// operator's shell can see. Cached: a miss costs a login-shell spawn.
pub fn resolve_program(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        return Some(PathBuf::from(program));
    }
    if let Ok(guard) = RESOLVED.lock() {
        if let Some(hit) = guard.as_ref().and_then(|entries| entries.get(program)) {
            return hit.clone();
        }
    }

    let resolved = resolve_on_path(program).or_else(|| resolve_via_login_shell(program));

    if let Ok(mut guard) = RESOLVED.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(program.to_string(), resolved.clone());
    }
    resolved
}

fn resolve_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}

/// Ask the operator's login shell where the program is. `-l` loads the profile
/// that a version manager writes its `PATH` into.
fn resolve_via_login_shell(program: &str) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let output = Command::new(shell)
        .arg("-lc")
        .arg(format!("command -v {program}"))
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    path.is_file().then_some(path)
}

/// A `Command` for `program`, resolved outside the server's `PATH` when needed.
pub fn harness_command(program: &str) -> Option<Command> {
    resolve_program(program).map(Command::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_program_is_used_as_written() {
        assert_eq!(
            resolve_program("/usr/bin/env"),
            Some(PathBuf::from("/usr/bin/env"))
        );
    }

    #[test]
    fn a_program_on_path_resolves_to_a_real_file() {
        let resolved = resolve_program("sh").expect("sh is installed");
        assert!(resolved.is_file(), "{resolved:?} should be a file");
    }

    #[test]
    fn a_program_that_does_not_exist_resolves_to_none() {
        assert_eq!(resolve_program("ajax-not-a-real-program"), None);
    }
}
