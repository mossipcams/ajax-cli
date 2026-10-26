//! nono sandbox wrapping for agent processes.

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use super::trusted_git::{self, TrustedGit};

const PROFILE_JSON: &str = include_str!("nono_agent_profile.json");
const PROFILE_FILE: &str = "ajax-agent.json";
const LOCAL_FILE: &str = "local.json";

/// Git paths a sandboxed linked worktree needs in order to commit.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitGrants {
    pub read: Vec<PathBuf>,
    pub read_write: Vec<PathBuf>,
    /// Single files, emitted as `--allow-file`.
    pub read_write_files: Vec<PathBuf>,
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

/// Writes the embedded profile to `dir/ajax-agent.json`, with `dir/local.json` merged in
/// when present.
///
/// `local.json` holds machine-specific grants and is user-owned: the profile denies agents
/// `dir`, so they cannot edit it. An identical existing profile is left alone.
/// Otherwise the profile is written to a unique temp file in `dir` and renamed over the
/// target, so a concurrent launch never reads a truncated profile.
pub fn write_profile(dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let content = profile_content(dir)?;
    let path = dir.join(PROFILE_FILE);
    if fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        return Ok(path);
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let tmp = dir.join(format!("{PROFILE_FILE}.{}.{nanos}.tmp", std::process::id()));
    fs::write(&tmp, content)?;
    if let Err(err) = fs::rename(&tmp, &path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    Ok(path)
}

fn profile_content(dir: &Path) -> std::io::Result<String> {
    let local_path = dir.join(LOCAL_FILE);
    let local = match fs::read_to_string(&local_path) {
        Ok(local) => local,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(PROFILE_JSON.to_string())
        }
        Err(err) => return Err(err),
    };
    let invalid = |err: serde_json::Error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{}: {err}", local_path.display()),
        )
    };
    let mut profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).map_err(invalid)?;
    merge_json(&mut profile, serde_json::from_str(&local).map_err(invalid)?);
    serde_json::to_string_pretty(&profile).map_err(invalid)
}

// ponytail: objects merge, arrays append, scalars replace; enough for adding grants.
fn merge_json(base: &mut serde_json::Value, extra: serde_json::Value) {
    use serde_json::Value;
    match (base, extra) {
        (Value::Object(base), Value::Object(extra)) => {
            for (key, value) in extra {
                merge_json(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (Value::Array(base), Value::Array(extra)) => base.extend(extra),
        (base, extra) => *base = extra,
    }
}

/// Ajax-owned profile directory, `$HOME/.config/ajax/nono`.
///
/// Not under `~/.cache`: that is writable inside the sandbox, so an agent could rewrite
/// the next launch's policy. The profile allows `$HOME` but denies `$HOME/.config/ajax/nono`.
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

/// A linked worktree whose `.git` file and admin dir were verified against each other.
#[derive(Debug, PartialEq, Eq)]
struct LinkedWorktree {
    gitdir: PathBuf,
    common: PathBuf,
}

impl LinkedWorktree {
    fn grants(&self) -> GitGrants {
        GitGrants {
            read: vec![self.common.clone()],
            read_write: vec![
                self.gitdir.clone(),
                self.common.join("objects"),
                self.common.join("refs"),
                self.common.join("logs"),
            ],
            read_write_files: vec![
                self.common.join("packed-refs"),
                self.common.join("packed-refs.lock"),
            ],
        }
    }
}

/// Git grants for a linked worktree, or empty grants if `worktree` cannot be verified as one.
// ponytail: the profile allows `$HOME`, so `.git` is writable under the denylist profile, including `.git/config` and hooks; these grants only add the linked worktree's git dirs and packed refs (#1409) to the explicit list. Branch upstream config is still recorded best-effort by scripts/gh-pr-create.
pub fn git_grants(worktree: &Path) -> GitGrants {
    linked_worktree(worktree).map_or_else(GitGrants::default, |linked| linked.grants())
}

fn linked_worktree(worktree: &Path) -> Option<LinkedWorktree> {
    // A symlinked `.git` could point at another worktree's admin dir, which passes the
    // back-link check below. Only a regular `.git` file is a linked worktree.
    let dot_git = worktree.join(".git");
    if !fs::symlink_metadata(&dot_git).ok()?.file_type().is_file() {
        return None;
    }
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
    if back_link != fs::canonicalize(worktree).ok()?.join(".git") {
        return None;
    }
    Some(LinkedWorktree { gitdir, common })
}

/// Builds the nono command line that runs `program args...` inside the sandbox.
// ponytail: codex-acp and npx-launched adapters are not rewritten; only a program named exactly `codex` gets the Seatbelt opt-out.
#[allow(clippy::too_many_arguments)]
pub fn wrap_argv(
    nono: &Path,
    profile: &Path,
    worktree: &Path,
    read: &[PathBuf],
    read_write: &[PathBuf],
    read_write_files: &[PathBuf],
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
    for path in read_write_files {
        argv.push("--allow-file".into());
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
    let linked = linked_worktree(cwd);
    if let Some(linked) = &linked {
        let store = trusted_git::default_store().ok_or_else(|| {
            "agent sandbox is on but HOME is unset, so no trusted-git store can be found; set HOME or AJAX_SANDBOX=off"
                .to_string()
        })?;
        let trusted = TrustedGit {
            git_dir: linked.gitdir.clone(),
            common_dir: linked.common.clone(),
            work_tree: cwd.to_path_buf(),
        };
        trusted_git::record(&store, &trusted).map_err(|err| {
            format!(
                "agent sandbox could not record trusted git metadata for {}: {err}; set AJAX_SANDBOX=off to run unsandboxed",
                cwd.display()
            )
        })?;
    }
    let mut grants = linked.map_or_else(GitGrants::default, |linked| linked.grants());
    grants.read_write.extend(extra_read_write.iter().cloned());
    Ok(wrap_argv(
        &nono,
        &profile,
        cwd,
        &grants.read,
        &grants.read_write,
        &grants.read_write_files,
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
    fn profile_denies_host_tmux_socket_issue_1396() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        for (path, platform) in [
            ("/private/tmp/tmux-$UID", "macos"),
            ("/tmp/tmux-$UID", "linux"),
        ] {
            assert!(
                deny.contains(&serde_json::json!({"path": path, "when": platform})),
                "agents must not reach the host tmux server at {path} on {platform}"
            );
        }
    }

    #[test]
    fn profile_denies_host_escape_sockets() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        for path in [
            "$HOME/.orbstack/run",
            "/private/var/run/docker.sock",
            "$HOME/.colima",
            "$HOME/.lima",
            "$HOME/.rd",
            "$HOME/.docker",
            "$HOME/.local/share/containers",
            "$HOME/Library/Containers/com.docker.docker",
            "$HOME/.config/colima",
            "/private/tmp/.s.PGSQL.5432",
        ] {
            assert!(
                deny.contains(&serde_json::json!({"path": path, "when": "macos"})),
                "agents must not reach the host socket at {path}: a reachable Docker API or Postgres superuser can run commands outside the sandbox"
            );
        }
    }

    #[test]
    fn profile_sets_allow_parent_of_protected() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        assert_eq!(
            profile["allow_parent_of_protected"],
            serde_json::json!(true),
            "nono refuses the $HOME grant because it contains ~/.nono, and every macOS launch fails without this flag"
        );
    }

    #[test]
    fn profile_omits_unlink_protection_group() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let include = profile["groups"]["include"].as_array().unwrap();
        assert!(
            !include.contains(&serde_json::json!("unlink_protection")),
            "unlink_protection's unlink overrides let agents delete denied Ajax state such as ~/.ajax-dev/ajax.db-wal"
        );
    }

    #[test]
    fn profile_denies_credential_files_outside_nono_defaults() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        for path in [
            "$HOME/.pgpass",
            "$HOME/.pypirc",
            "$HOME/.cargo/credentials",
            "$HOME/.cargo/credentials.toml",
        ] {
            assert!(
                deny.contains(&serde_json::json!(path)),
                "the $HOME grant exposes the credential file {path}, which nono's deny_credentials group does not cover"
            );
        }
    }

    #[test]
    fn profile_allows_home_and_denies_ajax_state() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let allow = profile["filesystem"]["allow"].as_array().unwrap();
        assert!(
            allow.contains(&serde_json::json!({"path": "$HOME", "when": "macos"})),
            "the denylist profile must grant $HOME on macOS"
        );
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        for path in [
            "$HOME/.config/ajax/nono",
            "$HOME/.config/ajax/config.toml",
            "$HOME/.local/state/ajax/ajax.db",
            "$HOME/.local/state/ajax/ajax.db-wal",
            "$HOME/.local/state/ajax/ajax.db-shm",
            "$HOME/.ajax-dev/config.toml",
            "$HOME/.ajax-dev/ajax.db",
            "$HOME/.ajax-dev/ajax.db-wal",
            "$HOME/.ajax-dev/ajax.db-shm",
        ] {
            assert!(
                deny.contains(&serde_json::json!(path)),
                "agents must not reach Ajax state at {path}"
            );
        }
    }

    #[test]
    fn profile_does_not_deny_whole_ajax_dirs() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        // dev worktrees, agent events, and the ssh agent socket live under these dirs
        for path in ["$HOME/.ajax-dev", "$HOME/.config/ajax"] {
            assert!(
                !deny.iter().any(|entry| {
                    entry.as_str() == Some(path)
                        || entry.get("path").and_then(|p| p.as_str()) == Some(path)
                }),
                "a whole-dir deny of {path} breaks dev worktrees, agent events and the ssh agent"
            );
        }
    }

    #[test]
    fn profile_lets_ssh_read_orbstack_include_but_not_its_keys_issue_1423() {
        let profile: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();
        // the `$HOME` grant covers the config; no explicit read_file entry is needed
        let allow = profile["filesystem"]["allow"].as_array().unwrap();
        assert!(
            allow.contains(&serde_json::json!({"path": "$HOME", "when": "macos"})),
            "ssh aborts on an unreadable Include (#1423)"
        );
        // only the config file: ~/.orbstack/ssh also holds OrbStack's private key,
        // which is denied explicitly below because the `$HOME` grant would expose it
        for key in ["read", "allow", "read_file", "allow_file"] {
            if let Some(entries) = profile["filesystem"][key].as_array() {
                for entry in entries {
                    let path = entry
                        .get("path")
                        .and_then(|p| p.as_str())
                        .or_else(|| entry.as_str())
                        .unwrap_or("");
                    assert!(
                        path == "$HOME/.orbstack/ssh/config"
                            || !path.starts_with("$HOME/.orbstack"),
                        "{key} grant {path} exposes ~/.orbstack beyond the ssh config"
                    );
                }
            }
        }
        let deny = profile["filesystem"]["deny"].as_array().unwrap();
        assert!(
            !deny.contains(&serde_json::json!({"path": "$HOME/.orbstack", "when": "macos"})),
            "a whole-dir ~/.orbstack deny breaks the ssh Include (#1423)"
        );
        assert!(
            deny.contains(
                &serde_json::json!({"path": "$HOME/.orbstack/ssh/id_ed25519", "when": "macos"})
            ),
            "the $HOME grant would otherwise expose OrbStack's private key"
        );
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
            &[PathBuf::from("/repo/.git-common/packed-refs")],
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
                "--allow-file",
                "/repo/.git-common/packed-refs",
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
    fn write_profile_merges_local_grants_into_embedded_profile() {
        let dir = scratch("write-profile-local");
        fs::write(
            dir.join(LOCAL_FILE),
            r#"{"filesystem": {"read": ["/opt/router"]}, "security": {"process_info_mode": "allow_same_sandbox"}}"#,
        )
        .unwrap();

        let profile: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(write_profile(&dir).unwrap()).unwrap())
                .unwrap();
        let embedded: serde_json::Value = serde_json::from_str(PROFILE_JSON).unwrap();

        let read = profile["filesystem"]["read"].as_array().unwrap();
        assert!(read.contains(&serde_json::json!("/opt/router")));
        for grant in embedded["filesystem"]["read"]
            .as_array()
            .map_or(&[][..], |v| v.as_slice())
        {
            assert!(read.contains(grant), "lost embedded grant {grant}");
        }
        assert_eq!(
            profile["filesystem"]["allow"],
            embedded["filesystem"]["allow"]
        );
        assert_eq!(
            profile["security"]["process_info_mode"],
            "allow_same_sandbox"
        );
        assert_eq!(profile["extends"], embedded["extends"]);
    }

    #[test]
    fn write_profile_fails_closed_on_invalid_local_json() {
        let dir = scratch("write-profile-local-invalid");
        fs::write(dir.join(LOCAL_FILE), "{not json").unwrap();

        let err = write_profile(&dir).unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("local.json"), "{err}");
        assert!(!dir.join(PROFILE_FILE).exists());
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
                read_write_files: vec![common.join("packed-refs"), common.join("packed-refs.lock"),],
            }
        );
        // `.git/config` and hooks run code (#1387), so no grant may cover them.
        let config = common.join("config");
        let hooks = common.join("hooks");
        for path in grants.read_write.iter().chain(&grants.read_write_files) {
            assert!(
                !path.starts_with(&config)
                    && !config.starts_with(path)
                    && !path.starts_with(&hooks)
                    && !hooks.starts_with(path),
                "grant {} covers .git/config or hooks",
                path.display()
            );
        }
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
    fn git_grants_return_default_when_dot_git_is_symlink_to_other_worktree() {
        let root = scratch("symlinked-worktree");
        init_repo(&root);
        let repo = root.join("repo");
        let linked = root.join("linked");
        let other = root.join("other");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "--detach",
                linked.to_str().unwrap(),
            ],
        );
        git(
            &repo,
            &["worktree", "add", "-q", "--detach", other.to_str().unwrap()],
        );
        fs::remove_file(linked.join(".git")).unwrap();
        std::os::unix::fs::symlink(other.join(".git"), linked.join(".git")).unwrap();

        assert_eq!(git_grants(&linked), GitGrants::default());
    }

    #[test]
    fn git_grants_return_default_for_non_git_dir() {
        let dir = scratch("not-git");

        assert_eq!(git_grants(&dir), GitGrants::default());
    }
}
