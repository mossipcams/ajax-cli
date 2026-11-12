use std::collections::BTreeSet;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};

use nix::unistd::{getgid, getuid};

use super::docker_sandbox::{
    check_ready, docker_argv, ensure_task_volumes, image_tag, resolve_docker, task_key,
    DockerLaunch,
};
use super::trusted_git::{self, TrustedGit};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct GitGrants {
    pub read: Vec<PathBuf>,
    pub read_write: Vec<PathBuf>,
    pub read_write_files: Vec<PathBuf>,
}

pub fn sandbox_enabled_from(value: Option<&str>) -> bool {
    !matches!(
        value.map(|v| v.trim().to_ascii_lowercase()).as_deref(),
        Some("off" | "0" | "false")
    )
}

pub fn sandbox_enabled() -> bool {
    sandbox_enabled_from(env::var("AJAX_SANDBOX").ok().as_deref())
}

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

pub fn git_grants(worktree: &Path) -> GitGrants {
    linked_worktree(worktree).map_or_else(GitGrants::default, |linked| linked.grants())
}

fn linked_worktree(worktree: &Path) -> Option<LinkedWorktree> {
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

const HOME_UNSET: &str =
    "agent sandbox is on but HOME is unset, so the agent home cannot be mounted; set HOME or AJAX_SANDBOX=off";
const TRUST_STORE_UNSET: &str =
    "agent sandbox is on but HOME is unset, so no trusted-git store can be found; set HOME or AJAX_SANDBOX=off";
static LAUNCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PARENTS_REPAIRED: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
const HOME_REPAIR_FAILED: &str = "agent sandbox could not repair ownership of the ajax-agent-home volume; set AJAX_SANDBOX=off to run unsandboxed";

#[derive(Clone)]
pub struct DockerHost {
    pub resolve: fn() -> Result<PathBuf, String>,
    pub run: fn(&Path, &[&str]) -> io::Result<Output>,
    pub home: Option<PathBuf>,
    pub store: Option<PathBuf>,
    pub uid: u32,
    pub gid: u32,
    pub pass_gh_token: bool,
    pub boot_id: String,
    pub server: String,
}

impl DockerHost {
    pub fn process(boot_id: String, server: String) -> Self {
        Self {
            resolve: resolve_docker,
            run: run_docker,
            home: env::var_os("HOME").map(PathBuf::from),
            store: trusted_git::default_store(),
            uid: getuid().as_raw(),
            gid: getgid().as_raw(),
            pass_gh_token: env::var_os("GH_TOKEN").is_some(),
            boot_id,
            server,
        }
    }
}

fn run_docker(docker: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new(docker)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

pub fn docker_command(
    host: &DockerHost,
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
    extra_read_write: &[PathBuf],
    tty: bool,
) -> Result<(PathBuf, Vec<String>), String> {
    let docker = (host.resolve)()?;
    let image = image_tag();
    check_ready(&image, |docker_args| (host.run)(&docker, docker_args))?;
    let common = trust_linked_worktree(host.store.as_deref(), cwd)?;
    let home = host.home.as_deref().ok_or_else(|| HOME_UNSET.to_string())?;
    let env: Vec<(String, String)> = env
        .iter()
        .filter(|(key, _)| key.as_str() != "PATH")
        .cloned()
        .collect();
    let instance = launch_instance();
    let launch = DockerLaunch {
        home,
        uid: host.uid,
        gid: host.gid,
        cwd,
        git_common_dir: common.as_deref(),
        extra_read_write,
        boot_id: &host.boot_id,
        server: &host.server,
        instance: &instance,
        tty,
        pass_gh_token: host.pass_gh_token,
        image: &image,
        program,
        args,
        env: &env,
    };
    let argv = docker_argv(&launch, Path::exists);
    repair_home_once(host, &docker, &image, &argv, home)?;
    ensure_task_volumes(
        &docker,
        &image,
        &task_key(cwd),
        host.uid,
        host.gid,
        host.run,
    )?;
    Ok((docker, argv))
}

pub fn home_repair_argv(
    image: &str,
    home: &Path,
    uid: u32,
    gid: u32,
    parents: &[PathBuf],
) -> Vec<String> {
    let home = home.display();
    let script = format!(
        "home=\"$1\"; shift; for dir in \"$@\"; do mkdir -p \"$dir\" || exit 1; done && find \"$home\" -xdev -user 0 -exec chown {uid}:{gid} {{}} +"
    );
    let mut argv: Vec<String> = ["run", "--rm", "--user", "0", "-v"]
        .into_iter()
        .map(String::from)
        .collect();
    argv.push(format!("ajax-agent-home:{home}"));
    argv.extend(
        ["--entrypoint", "sh", image, "-c"]
            .into_iter()
            .map(String::from),
    );
    argv.push(script);
    argv.push("sh".to_string());
    argv.push(home.to_string());
    argv.extend(parents.iter().map(|parent| parent.display().to_string()));
    argv
}

fn home_parents(argv: &[String], image: &str, home: &Path) -> Vec<PathBuf> {
    let mounts = argv
        .iter()
        .position(|arg| arg == image)
        .unwrap_or(argv.len());
    let parents: BTreeSet<PathBuf> = argv[..mounts]
        .windows(2)
        .filter(|pair| pair[0] == "-v")
        .filter_map(|pair| pair[1].split(':').next())
        .map(Path::new)
        .filter(|source| source.starts_with(home))
        .filter_map(Path::parent)
        .map(Path::to_path_buf)
        .collect();
    parents.into_iter().collect()
}

fn repair_home_once(
    host: &DockerHost,
    docker: &Path,
    image: &str,
    argv: &[String],
    home: &Path,
) -> Result<(), String> {
    let parents = home_parents(argv, image, home);
    let mut repaired = PARENTS_REPAIRED
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if parents.iter().all(|parent| repaired.contains(parent)) {
        return Ok(());
    }
    let repair = home_repair_argv(image, home, host.uid, host.gid, &parents);
    let args: Vec<&str> = repair.iter().map(String::as_str).collect();
    if !(host.run)(docker, &args).is_ok_and(|output| output.status.success()) {
        return Err(HOME_REPAIR_FAILED.to_string());
    }
    repaired.extend(parents);
    Ok(())
}

fn launch_instance() -> String {
    let sequence = LAUNCH_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{sequence:x}", std::process::id())
}

fn trust_linked_worktree(store: Option<&Path>, cwd: &Path) -> Result<Option<PathBuf>, String> {
    let grants = git_grants(cwd);
    let (Some(common), Some(gitdir)) = (grants.read.first(), grants.read_write.first()) else {
        return Ok(None);
    };
    let store = store.ok_or_else(|| TRUST_STORE_UNSET.to_string())?;
    let trusted = TrustedGit {
        git_dir: gitdir.clone(),
        common_dir: common.clone(),
        work_tree: cwd.to_path_buf(),
    };
    trusted_git::record(store, &trusted).map_err(|err| {
        format!(
            "agent sandbox could not record trusted git metadata for {}: {err}; set AJAX_SANDBOX=off to run unsandboxed",
            cwd.display()
        )
    })?;
    Ok(Some(common.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

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

    fn docker_ok(_: &Path, _: &[&str]) -> io::Result<Output> {
        Ok(Output {
            status: std::os::unix::process::ExitStatusExt::from_raw(0),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    fn fake_docker_host(home: Option<PathBuf>) -> DockerHost {
        DockerHost {
            resolve: || Ok(PathBuf::from("/opt/docker")),
            run: docker_ok,
            home,
            store: None,
            uid: 501,
            gid: 20,
            pass_gh_token: false,
            boot_id: "boot-test".to_string(),
            server: "server-test".to_string(),
        }
    }

    #[test]
    fn docker_command_mounts_extra_read_write_paths_and_allocates_a_tty_when_asked() {
        let host = fake_docker_host(Some(PathBuf::from("/ajax-test-home")));
        let extra = [PathBuf::from("/var/folders/ajax/events")];

        let (program, argv) = docker_command(
            &host,
            "codex",
            &[],
            Path::new("/work/task"),
            &[],
            &extra,
            true,
        )
        .expect("launch wraps");

        assert_eq!(program, PathBuf::from("/opt/docker"));
        assert_eq!(argv[..4], ["run", "--rm", "-i", "-t"]);
        assert!(argv.windows(2).any(|pair| {
            pair[0] == "-v" && pair[1] == "/var/folders/ajax/events:/var/folders/ajax/events"
        }));
    }

    #[test]
    fn docker_command_without_home_fails_closed_and_mentions_override() {
        let host = fake_docker_host(None);

        let err =
            docker_command(&host, "claude", &[], Path::new("/w"), &[], &[], false).unwrap_err();

        assert!(err.contains("HOME"), "{err}");
        assert!(err.contains("AJAX_SANDBOX=off"), "{err}");
    }

    const REPAIR_SCRIPT: &str = "home=\"$1\"; shift; for dir in \"$@\"; do mkdir -p \"$dir\" || exit 1; done && find \"$home\" -xdev -user 0 -exec chown 501:20 {} +";

    #[test]
    fn home_repair_argv_is_exact() {
        let parents = [
            PathBuf::from("/Users/matt/.local/state"),
            PathBuf::from("/Users/matt/Projects"),
        ];

        assert_eq!(
            home_repair_argv(
                "ajax-agent:abc",
                Path::new("/Users/matt"),
                501,
                20,
                &parents
            ),
            [
                "run",
                "--rm",
                "--user",
                "0",
                "-v",
                "ajax-agent-home:/Users/matt",
                "--entrypoint",
                "sh",
                "ajax-agent:abc",
                "-c",
                REPAIR_SCRIPT,
                "sh",
                "/Users/matt",
                "/Users/matt/.local/state",
                "/Users/matt/Projects",
            ]
        );
    }

    thread_local! {
        static DOCKER_CALLS: RefCell<Vec<Vec<String>>> = const { RefCell::new(Vec::new()) };
    }

    fn recording_docker(_: &Path, args: &[&str]) -> io::Result<Output> {
        DOCKER_CALLS.with(|calls| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| (*arg).to_string()).collect());
        });
        docker_ok(Path::new("/opt/docker"), args)
    }

    fn docker_fails_repair(docker: &Path, args: &[&str]) -> io::Result<Output> {
        if args.contains(&"--entrypoint") {
            return Ok(Output {
                status: std::os::unix::process::ExitStatusExt::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: Vec::new(),
            });
        }
        docker_ok(docker, args)
    }

    fn is_home_repair(call: &[String]) -> bool {
        call.starts_with(&["run", "--rm", "--user", "0"].map(String::from))
    }

    #[test]
    fn home_repair_runs_only_for_parents_not_yet_repaired() {
        let home = PathBuf::from("/ajax-repair-parents-home");
        let host = DockerHost {
            run: recording_docker,
            ..fake_docker_host(Some(home.clone()))
        };
        let first = home.join("Projects/repo__worktrees/a");
        let second = home.join("Other/repo__worktrees/b");
        DOCKER_CALLS.with(|calls| calls.borrow_mut().clear());

        for cwd in [&first, &second, &first] {
            docker_command(&host, "claude", &[], cwd, &[], &[], false).expect("launch wraps");
        }

        let repairs = DOCKER_CALLS.with(|calls| {
            calls
                .borrow()
                .iter()
                .filter(|call| is_home_repair(call))
                .cloned()
                .collect::<Vec<_>>()
        });
        assert_eq!(
            repairs,
            [
                home_repair_argv(
                    &image_tag(),
                    &home,
                    501,
                    20,
                    &[home.join("Projects/repo__worktrees")]
                ),
                home_repair_argv(
                    &image_tag(),
                    &home,
                    501,
                    20,
                    &[home.join("Other/repo__worktrees")]
                ),
            ]
        );
    }

    #[test]
    fn a_failed_home_repair_refuses_the_launch() {
        let host = DockerHost {
            run: docker_fails_repair,
            ..fake_docker_host(Some(PathBuf::from("/ajax-repair-failed-home")))
        };

        let err = docker_command(
            &host,
            "claude",
            &[],
            Path::new("/ajax-repair-failed-home/w"),
            &[],
            &[],
            false,
        )
        .unwrap_err();

        assert!(err.contains("ajax-agent-home"), "{err}");
        assert!(err.contains("AJAX_SANDBOX=off"), "{err}");
    }
}
