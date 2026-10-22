//! nono sandbox wrapping for agent processes.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const PROFILE_JSON: &str = include_str!("nono_agent_profile.json");
const PROFILE_FILE: &str = "ajax-agent.json";

/// Git paths a sandboxed linked worktree needs in order to commit.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitGrants {
    pub read: Vec<PathBuf>,
    pub read_write: Vec<PathBuf>,
}

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

/// Writes the embedded profile to `dir/ajax-agent.json`.
///
/// An identical existing file is left alone. Otherwise the profile is written to a
/// unique temp file in `dir` and renamed over the target, so a concurrent launch
/// never reads a truncated profile.
pub fn write_profile(dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(PROFILE_FILE);
    if fs::read_to_string(&path).is_ok_and(|content| content == PROFILE_JSON) {
        return Ok(path);
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let tmp = dir.join(format!("{PROFILE_FILE}.{}.{nanos}.tmp", std::process::id()));
    fs::write(&tmp, PROFILE_JSON)?;
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(path)
}

/// Ajax-owned profile directory, `$HOME/.config/ajax/nono`.
///
/// Not under `~/.cache`: that is writable inside the sandbox, so an agent could rewrite
/// the next launch's policy. `~/.config/ajax` is verified ungranted.
pub fn default_profile_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/ajax/nono"))
}

/// Finds `nono` on `PATH`, then at fixed install locations, since launchd and tmux
/// often run with a minimal `PATH`.
pub fn resolve_nono() -> Result<PathBuf, String> {
    let on_path = env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join("nono"))
            .find(|candidate| candidate.is_file())
    });
    let home = env::var_os("HOME").map(PathBuf::from);
    let fixed =
        [
            PathBuf::from("/opt/homebrew/bin/nono"),
            PathBuf::from("/usr/local/bin/nono"),
        ]
        .into_iter()
        .chain(home.into_iter().flat_map(|home| {
            [".cargo/bin/nono", ".nix-profile/bin/nono"].map(|rel| home.join(rel))
        }));
    on_path
        .or_else(|| fixed.into_iter().find(|candidate| candidate.is_file()))
        .ok_or_else(|| {
            "agent sandbox is on but nono is not installed; run `brew install nono` or set AJAX_SANDBOX=off"
                .to_string()
        })
}

/// Git grants for a linked worktree, or empty grants if `worktree` cannot be verified as one.
// ponytail: main-checkout (non-linked) runs and packed-ref branch deletion get no .git write access; widen only with a hooks/config-safe grant.
pub fn git_grants(worktree: &Path) -> GitGrants {
    linked_git_grants(worktree).unwrap_or_default()
}

fn linked_git_grants(worktree: &Path) -> Option<GitGrants> {
    let mut command = Command::new("git");
    command.arg("-C").arg(worktree).args([
        "rev-parse",
        "--path-format=absolute",
        "--git-common-dir",
        "--absolute-git-dir",
    ]);
    for (name, _) in env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut lines = text.lines();
    let common = fs::canonicalize(lines.next()?).ok()?;
    let gitdir = fs::canonicalize(lines.next()?).ok()?;
    if common.file_name() != Some(OsStr::new(".git"))
        || gitdir.parent() != Some(common.join("worktrees").as_path())
    {
        return None;
    }
    let back_link = fs::read_to_string(gitdir.join("gitdir")).ok()?;
    let back_link = fs::canonicalize(PathBuf::from(back_link.trim())).ok()?;
    if back_link != fs::canonicalize(worktree.join(".git")).ok()? {
        return None;
    }
    Some(GitGrants {
        read: vec![common.clone()],
        read_write: vec![
            gitdir,
            common.join("objects"),
            common.join("refs"),
            common.join("logs"),
        ],
    })
}

/// Builds the nono command line that runs `program args...` inside the sandbox.
// ponytail: codex-acp and npx-launched adapters are not rewritten; only a program named exactly `codex` gets the Seatbelt opt-out.
pub fn wrap_argv(
    nono: &Path,
    profile: &Path,
    worktree: &Path,
    read: &[PathBuf],
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
    for path in read {
        argv.push("--read".into());
        argv.push(path.display().to_string());
    }
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

/// Wraps `program args...` in nono when enabled; otherwise returns it unchanged without
/// resolving nono.
pub fn sandbox_command(
    enabled: bool,
    nono: impl FnOnce() -> Result<PathBuf, String>,
    profile_dir: Option<PathBuf>,
    cwd: &Path,
    extra_read_write: &[PathBuf],
    program: &str,
    args: &[String],
) -> Result<(String, Vec<String>), String> {
    if !enabled {
        return Ok((program.to_string(), args.to_vec()));
    }
    let nono = nono()?;
    let dir = profile_dir.ok_or_else(|| {
        "agent sandbox is on but HOME is unset, so no profile directory can be found; set HOME or AJAX_SANDBOX=off"
            .to_string()
    })?;
    let profile = write_profile(&dir).map_err(|err| {
        format!(
            "agent sandbox could not write profile to {}: {err}; set AJAX_SANDBOX=off to run unsandboxed",
            dir.display()
        )
    })?;
    let mut grants = git_grants(cwd);
    grants.read_write.extend(extra_read_write.iter().cloned());
    Ok(wrap_argv(
        &nono,
        &profile,
        cwd,
        &grants.read,
        &grants.read_write,
        program,
        args,
    ))
}

/// [`sandbox_command`] with `AJAX_SANDBOX`, the `nono` lookup, and the profile dir taken
/// from the environment.
pub fn sandbox_command_from_env(
    cwd: &Path,
    extra_read_write: &[PathBuf],
    program: &str,
    args: &[String],
) -> Result<(String, Vec<String>), String> {
    sandbox_command(
        sandbox_enabled(),
        resolve_nono,
        default_profile_dir(),
        cwd,
        extra_read_write,
        program,
        args,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/sandbox-tests")
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
                "user.email=sandbox@example.invalid",
                "-c",
                "user.name=sandbox",
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

    /// Creates `root/repo` with one commit and returns its canonical common dir.
    fn init_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        fs::canonicalize(repo.join(".git")).unwrap()
    }

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
            &[PathBuf::from("/repo/.git-common")],
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
                "--read",
                "/repo/.git-common",
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

    #[test]
    fn write_profile_leaves_identical_file_untouched() {
        let dir = scratch("write-profile-identical");

        let first = write_profile(&dir).unwrap();
        let second = write_profile(&dir).unwrap();

        assert_eq!(first, second);
        assert_eq!(fs::read_to_string(&second).unwrap(), PROFILE_JSON);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn sandbox_command_disabled_returns_program_unchanged_without_resolving_nono() {
        let args = ["--resume".to_string()];
        let result = sandbox_command(
            false,
            || -> Result<PathBuf, String> { panic!("nono must not be resolved when disabled") },
            None,
            Path::new("/w"),
            &[],
            "claude",
            &args,
        );
        assert_eq!(result, Ok(("claude".to_string(), args.to_vec())));
    }

    #[test]
    fn sandbox_command_enabled_propagates_nono_error() {
        let result = sandbox_command(
            true,
            || Err("nono missing".to_string()),
            Some(PathBuf::from("/unused")),
            Path::new("/w"),
            &[],
            "claude",
            &[],
        );
        assert_eq!(result, Err("nono missing".to_string()));
    }

    #[test]
    fn sandbox_command_enabled_without_profile_dir_mentions_override() {
        let result = sandbox_command(
            true,
            || Ok(PathBuf::from("/opt/nono")),
            None,
            Path::new("/w"),
            &[],
            "claude",
            &[],
        );
        let err = result.unwrap_err();
        assert!(err.contains("HOME"), "{err}");
        assert!(err.contains("AJAX_SANDBOX=off"), "{err}");
    }

    #[test]
    fn git_grants_for_linked_worktree_cover_common_dir_and_worktree_metadata() {
        let root = scratch("linked-worktree");
        let common = init_repo(&root);
        let worktree = root.join("wt");
        git(
            &root.join("repo"),
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                worktree.to_str().unwrap(),
            ],
        );

        let grants = git_grants(&worktree);

        let gitdir = common.join("worktrees").join("wt");
        assert_eq!(
            grants,
            GitGrants {
                read: vec![common.clone()],
                read_write: vec![
                    gitdir,
                    common.join("objects"),
                    common.join("refs"),
                    common.join("logs"),
                ],
            }
        );
    }

    #[test]
    fn git_grants_return_default_when_worktree_link_is_forged() {
        let root = scratch("forged-worktree");
        let common = init_repo(&root);
        let forged = root.join("forged");
        let other = root.join("other");
        let repo = root.join("repo");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                forged.to_str().unwrap(),
            ],
        );
        git(
            &repo,
            &["worktree", "add", "-q", "--detach", other.to_str().unwrap()],
        );
        let other_gitdir = common.join("worktrees").join("other");
        fs::write(
            forged.join(".git"),
            format!("gitdir: {}\n", other_gitdir.display()),
        )
        .unwrap();

        assert_eq!(git_grants(&forged), GitGrants::default());
    }

    #[test]
    fn git_grants_return_default_for_non_git_dir() {
        let dir = scratch("not-git");

        assert_eq!(git_grants(&dir), GitGrants::default());
    }
}
