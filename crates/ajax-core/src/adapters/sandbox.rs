//! nono sandbox wrapping for agent processes.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROFILE_JSON: &str = include_str!("nono_agent_profile.json");
const PROFILE_FILE: &str = "ajax-agent.json";

/// Sandbox is on unless `AJAX_SANDBOX` is `off`, `0`, or `false`.
pub fn sandbox_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("off" | "0" | "false")
    )
}

/// Reads `AJAX_SANDBOX` from the environment.
pub fn sandbox_enabled() -> bool {
    sandbox_enabled_from(env::var("AJAX_SANDBOX").ok().as_deref())
}

/// Writes the embedded profile to `dir/ajax-agent.json`, overwriting any existing copy.
pub fn write_profile(dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(PROFILE_FILE);
    fs::write(&path, PROFILE_JSON)?;
    Ok(path)
}

/// Ajax-owned profile directory, outside any sandbox grant.
pub fn default_profile_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache/ajax/nono"))
}

/// Finds `nono` on `PATH`.
pub fn resolve_nono() -> Result<PathBuf, String> {
    env::var_os("PATH")
        .and_then(|paths| {
            env::split_paths(&paths)
                .map(|dir| dir.join("nono"))
                .find(|candidate| candidate.is_file())
        })
        .ok_or_else(|| {
            "agent sandbox is on but nono is not installed; run `brew install nono` or set AJAX_SANDBOX=off"
                .to_string()
        })
}

/// Absolute git common dir for a worktree, or `None` if git fails.
pub fn git_common_dir(worktree: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(worktree)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim_end();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

/// Builds the nono command line that runs `program args...` inside the sandbox.
// ponytail: codex-acp and npx-launched adapters are not rewritten; only a program named exactly `codex` gets the Seatbelt opt-out.
pub fn wrap_argv(
    nono: &Path,
    profile: &Path,
    worktree: &Path,
    read_write: &[PathBuf],
    program: &str,
    args: &[String],
) -> (String, Vec<String>) {
    let mut argv: Vec<String> = [
        "-s",
        "run",
        "--allow-cwd",
        "--no-rollback-prompt",
        "--profile",
    ]
    .map(String::from)
    .into();
    argv.push(profile.display().to_string());
    argv.push("--workdir".into());
    argv.push(worktree.display().to_string());
    for path in read_write {
        argv.push("--allow".into());
        argv.push(path.display().to_string());
    }
    argv.push("--".into());
    argv.push(program.into());
    if Path::new(program).file_name() == Some(OsStr::new("codex")) {
        argv.push("-c".into());
        argv.push("sandbox_mode=\"danger-full-access\"".into());
    }
    argv.extend(args.iter().cloned());
    (nono.display().to_string(), argv)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_enabled_from_table() {
        let cases = [
            (None, true),
            (Some(""), true),
            (Some("on"), true),
            (Some("1"), true),
            (Some("no"), true),
            (Some("off"), false),
            (Some(" OFF "), false),
            (Some("0"), false),
            (Some("False"), false),
        ];
        for (value, expected) in cases {
            assert_eq!(sandbox_enabled_from(value), expected, "{value:?}");
        }
    }

    #[test]
    fn wrap_argv_builds_exact_argv_with_multiple_grants() {
        let (program, argv) = wrap_argv(
            Path::new("/opt/nono"),
            Path::new("/cache/nono/ajax-agent.json"),
            Path::new("/work/task"),
            &[
                PathBuf::from("/repo/.git"),
                PathBuf::from("/cache/agent-events"),
            ],
            "claude",
            &["--resume".to_string()],
        );
        assert_eq!(program, "/opt/nono");
        assert_eq!(
            argv,
            [
                "-s",
                "run",
                "--allow-cwd",
                "--no-rollback-prompt",
                "--profile",
                "/cache/nono/ajax-agent.json",
                "--workdir",
                "/work/task",
                "--allow",
                "/repo/.git",
                "--allow",
                "/cache/agent-events",
                "--",
                "claude",
                "--resume",
            ]
        );
    }

    #[test]
    fn wrap_argv_disables_codex_inner_sandbox_for_bare_and_absolute_codex() {
        for program in ["codex", "/x/y/codex"] {
            let (_, argv) = wrap_argv(
                Path::new("/opt/nono"),
                Path::new("/p.json"),
                Path::new("/w"),
                &[],
                program,
                &["exec".to_string()],
            );
            assert_eq!(
                argv.iter().map(String::as_str).collect::<Vec<_>>(),
                [
                    "-s",
                    "run",
                    "--allow-cwd",
                    "--no-rollback-prompt",
                    "--profile",
                    "/p.json",
                    "--workdir",
                    "/w",
                    "--",
                    program,
                    "-c",
                    "sandbox_mode=\"danger-full-access\"",
                    "exec",
                ],
                "{program}"
            );
        }
    }

    #[test]
    fn wrap_argv_does_not_rewrite_other_programs() {
        for program in ["codex-acp", "claude"] {
            let (_, argv) = wrap_argv(
                Path::new("/opt/nono"),
                Path::new("/p.json"),
                Path::new("/w"),
                &[],
                program,
                &[],
            );
            assert!(!argv.iter().any(|arg| arg == "-c"), "{program}");
        }
    }

    #[test]
    fn write_profile_overwrites_tampered_file_with_valid_json() {
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sandbox-tests/write-profile");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(PROFILE_FILE), "tampered").unwrap();

        let path = write_profile(&dir).unwrap();

        assert_eq!(path, dir.join(PROFILE_FILE));
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, PROFILE_JSON);
        serde_json::from_str::<serde_json::Value>(&content).unwrap();
    }
}
