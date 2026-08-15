//! Resolve harness executables for a long-lived server process.
//!
//! `ajax-cli web` runs under tmux or a service manager, so its `PATH` is
//! whatever that supervisor had when it started. Version managers move
//! binaries — an nvm switch leaves `codex`, `pi`, and the ACP bridges under a
//! node version the daemon's `PATH` no longer names — and the operator then
//! sees a harness that simply "has no models". Fall back to the operator's own
//! shell, which is where those tools were installed to be visible.

use std::{
    collections::HashMap,
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
};

static RESOLVED: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);
static SHELL_PATH: Mutex<Option<Option<String>>> = Mutex::new(None);

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

    let resolved = resolve_on_path(program)
        .or_else(|| resolve_via_shell(program, &["-lc"]))
        // Version managers commonly set PATH in `.zshrc`, which a login shell
        // skips unless it is also interactive.
        .or_else(|| resolve_via_shell(program, &["-ilc"]));

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

/// Ask the operator's shell where the program is, with the given flags.
fn resolve_via_shell(program: &str, flags: &[&str]) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let output = Command::new(shell)
        .args(flags)
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

/// `PATH` as the operator's shell sees it, cached. Empty when the shell cannot
/// be asked.
fn shell_path() -> Option<String> {
    if let Ok(guard) = SHELL_PATH.lock() {
        if let Some(cached) = guard.as_ref() {
            return cached.clone();
        }
    }

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let resolved = ["-ilc", "-lc"].into_iter().find_map(|flags| {
        let output = Command::new(&shell)
            .arg(flags)
            .arg("printf %s \"$PATH\"")
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (output.status.success() && !path.is_empty()).then_some(path)
    });

    if let Ok(mut guard) = SHELL_PATH.lock() {
        *guard = Some(resolved.clone());
    }
    resolved
}

/// A `Command` for `program`, resolved outside the server's `PATH` when needed.
///
/// The child gets the operator's shell `PATH` as well: an ACP adapter spawns its
/// own harness (`claude-agent-acp` runs `claude`), so resolving only our direct
/// child would leave the adapter unable to find the CLI behind it.
pub fn harness_command(program: &str) -> Option<Command> {
    let mut command = Command::new(resolve_program(program)?);
    if let Some(shell_path) = shell_path() {
        let own = std::env::var("PATH").unwrap_or_default();
        let merged = if own.is_empty() {
            shell_path
        } else {
            format!("{shell_path}:{own}")
        };
        command.env("PATH", merged);
    }
    Some(command)
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

    // nvm and friends export PATH from `.zshrc`, which a plain login shell does
    // not read — the daemon then cannot see any harness the operator installed.
    // An ACP adapter spawns the harness CLI itself, so it needs a PATH that
    // names it — resolving only our own child is not enough.
    #[test]
    fn a_harness_command_carries_the_shell_path() {
        let command = harness_command("sh").expect("sh is installed");
        let path = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("PATH"))
            .and_then(|(_, value)| value)
            .map(|value| value.to_string_lossy().to_string());
        if shell_path().is_some() {
            assert!(path.is_some(), "harness children should carry a PATH");
        }
    }

    #[test]
    fn shell_resolution_falls_back_to_an_interactive_login_shell() {
        let source = include_str!("program.rs");
        let login_at = source
            .find(r#"resolve_via_shell(program, &["-lc"])"#)
            .expect("login shell attempt");
        let interactive_at = source
            .find(r#"resolve_via_shell(program, &["-ilc"])"#)
            .expect("interactive login shell attempt");
        assert!(
            login_at < interactive_at,
            "the cheaper login shell should be tried first"
        );
    }
}
