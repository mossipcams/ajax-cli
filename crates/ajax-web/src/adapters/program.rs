use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

use ajax_core::adapters::sandbox::{
    default_profile_dir, resolve_nono, sandbox_command, sandbox_enabled,
};

static RESOLVED: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);
static SHELL_PATH: Mutex<Option<Option<String>>> = Mutex::new(None);

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

pub(crate) fn global_npm_root() -> Option<PathBuf> {
    let npm = resolve_program("npm")?;
    let home = std::env::var_os("HOME")?;
    let home_path = Path::new(&home);
    if !home_path.is_dir() {
        return None;
    }
    let output = Command::new(npm)
        .args(["root", "-g"])
        .current_dir(home_path)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8(output.stdout).ok()?.trim().to_string();
    let path = PathBuf::from(root);
    path.is_absolute().then_some(path)
}

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

pub fn harness_command(program: &str) -> Option<Command> {
    let mut command = Command::new(resolve_program(program)?);
    apply_shell_path(&mut command);
    Some(command)
}

pub(crate) fn sandboxed_harness_command(
    program: &str,
    args: &[String],
    cwd: &Path,
) -> io::Result<Command> {
    let resolved = resolve_program(program).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{program} is not installed"),
        )
    })?;
    let (wrapped, wrapped_args) = sandboxed_program(&resolved, args, cwd)?;
    let mut command = Command::new(wrapped);
    command.args(wrapped_args);
    apply_shell_path(&mut command);
    Ok(command)
}

pub(crate) fn sandboxed_program(
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> io::Result<(PathBuf, Vec<String>)> {
    sandbox_launch(
        sandbox_enabled(),
        resolve_nono_for_launch,
        default_profile_dir(),
        program,
        args,
        cwd,
    )
    .map_err(io::Error::other)
}

fn resolve_nono_for_launch() -> Result<PathBuf, String> {
    resolve_nono().or_else(|error| resolve_program("nono").ok_or(error))
}

fn sandbox_launch(
    enabled: bool,
    nono: impl FnOnce() -> Result<PathBuf, String>,
    profile_dir: Option<PathBuf>,
    program: &Path,
    args: &[String],
    cwd: &Path,
) -> Result<(PathBuf, Vec<String>), String> {
    let (wrapped, wrapped_args) = sandbox_command(
        enabled,
        nono,
        profile_dir,
        cwd,
        &[],
        &program.display().to_string(),
        args,
    )?;
    Ok((PathBuf::from(wrapped), wrapped_args))
}

fn apply_shell_path(command: &mut Command) {
    if let Some(shell_path) = shell_path() {
        let own = std::env::var("PATH").unwrap_or_default();
        let merged = if own.is_empty() {
            shell_path
        } else {
            format!("{shell_path}:{own}")
        };
        command.env("PATH", merged);
    }
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

    #[test]
    fn a_sandboxed_harness_for_a_missing_program_is_not_found_so_callers_can_skip_it() {
        let error = sandboxed_harness_command(
            "ajax-not-a-real-program",
            &["exec".to_string()],
            Path::new(env!("CARGO_MANIFEST_DIR")),
        )
        .expect_err("a missing program has no launch");
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

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

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/sandbox-tests/program")
            .join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_nono() -> Result<PathBuf, String> {
        Ok(PathBuf::from("/opt/nono"))
    }

    #[test]
    fn an_enabled_launch_wraps_the_agent_in_nono_with_the_worktree_and_profile() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let profile_dir = scratch_dir("argv");
        let args = vec!["exec".to_string()];

        let (program, argv) = sandbox_launch(
            true,
            fake_nono,
            Some(profile_dir.clone()),
            Path::new("codex"),
            &args,
            cwd,
        )
        .expect("enabled launch wraps");

        assert_eq!(program, PathBuf::from("/opt/nono"));
        assert!(profile_dir.join("ajax-agent.json").is_file());
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--workdir" && pair[1] == cwd.display().to_string()));
        assert!(argv.ends_with(
            &[
                "--",
                "codex",
                "-c",
                "sandbox_mode=\"danger-full-access\"",
                "exec",
            ]
            .map(String::from)
        ));
    }

    #[test]
    fn a_disabled_launch_is_the_identity() {
        let args = vec!["--mode".to_string(), "rpc".to_string()];

        let launched = sandbox_launch(
            false,
            || panic!("nono must not be resolved when the sandbox is off"),
            None,
            Path::new("node"),
            &args,
            Path::new("/work/task"),
        )
        .expect("disabled launch is never an error");

        assert_eq!(launched, (PathBuf::from("node"), args));
    }

    #[test]
    fn an_enabled_launch_fails_closed_when_nono_is_missing() {
        let error = sandbox_launch(
            true,
            || Err("nono is not installed (AJAX_SANDBOX=off)".to_string()),
            Some(scratch_dir("missing-nono")),
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
        )
        .expect_err("a missing nono must refuse the launch");

        assert_eq!(error, "nono is not installed (AJAX_SANDBOX=off)");
    }

    #[test]
    fn an_enabled_launch_fails_closed_when_the_profile_cannot_be_written() {
        let blocker = scratch_dir("blocked").join("not-a-dir");
        std::fs::write(&blocker, "file").unwrap();

        let error = sandbox_launch(
            true,
            fake_nono,
            Some(blocker),
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
        )
        .expect_err("an unwritable profile must refuse the launch");

        assert!(error.contains("profile"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }

    #[test]
    fn an_enabled_launch_fails_closed_without_a_profile_directory() {
        let error = sandbox_launch(
            true,
            fake_nono,
            None,
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
        )
        .expect_err("no profile directory must refuse the launch");

        assert!(error.contains("HOME"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }
}
