use std::{
    collections::HashMap,
    env, fs, io,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
};

use ajax_core::adapters::{
    docker_sandbox::{check_ready, docker_argv, image_tag, resolve_docker, task_key, DockerLaunch},
    sandbox::{git_grants, sandbox_enabled},
    trusted_git::{self, TrustedGit},
};
use nix::unistd::{getgid, getuid};

use super::sandbox_image::ensure_task_volumes;

const HOME_UNSET: &str =
    "agent sandbox is on but HOME is unset, so the agent home cannot be mounted; set HOME or AJAX_SANDBOX=off";
const TRUST_STORE_UNSET: &str =
    "agent sandbox is on but HOME is unset, so no trusted-git store can be found; set HOME or AJAX_SANDBOX=off";

static SANDBOX_BOOT_ID: OnceLock<String> = OnceLock::new();
static SANDBOX_SERVER_ID: OnceLock<String> = OnceLock::new();

static RESOLVED: Mutex<Option<HashMap<String, Option<PathBuf>>>> = Mutex::new(None);
static SHELL_PATH: Mutex<Option<Option<String>>> = Mutex::new(None);
static LAUNCH_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const PROBE_DIR: &str = "ajax-sandbox-probe";

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

pub fn set_sandbox_boot_id(id: &str) {
    let _ = SANDBOX_BOOT_ID.set(id.to_string());
}

pub fn set_sandbox_server_id(id: &str) {
    let _ = SANDBOX_SERVER_ID.set(id.to_string());
}

pub(crate) fn sandboxed_harness_command(
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> io::Result<Command> {
    let (enabled, host) = sandbox_context();
    harness_launch(enabled, &host, program, args, cwd, env)
}

pub(crate) fn probe_command(program: &str, args: &[&str]) -> io::Result<Command> {
    let (enabled, host) = sandbox_context();
    let cwd = if enabled {
        probe_dir()?
    } else {
        PathBuf::new()
    };
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    harness_launch(enabled, &host, program, &args, &cwd, &[])
}

pub(crate) fn probe_dir() -> io::Result<PathBuf> {
    let dir = env::temp_dir().join(PROBE_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub(crate) fn sandbox_on() -> bool {
    sandbox_context().0
}

pub(crate) fn sandbox_docker_server() -> Option<String> {
    #[cfg(test)]
    if TEST_SANDBOX.with(|slot| slot.borrow().is_none()) {
        return None;
    }
    let (enabled, host) = sandbox_context();
    enabled.then_some(host.server)
}

pub(crate) fn sandbox_docker_run(args: &[&str]) -> io::Result<Output> {
    let (_, host) = sandbox_context();
    let docker = (host.resolve)().map_err(io::Error::other)?;
    (host.run)(&docker, args)
}

fn sandbox_context() -> (bool, DockerHost) {
    #[cfg(test)]
    if let Some(context) = TEST_SANDBOX.with(|slot| slot.borrow().clone()) {
        return context;
    }
    (sandbox_enabled(), DockerHost::process())
}

fn harness_launch(
    enabled: bool,
    host: &DockerHost,
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> io::Result<Command> {
    let program = if enabled {
        PathBuf::from(program)
    } else {
        resolve_program(program).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{program} is not installed"),
            )
        })?
    };
    let (wrapped, wrapped_args) =
        sandbox_launch(enabled, host, &program, args, cwd, env).map_err(io::Error::other)?;
    let mut command = Command::new(wrapped);
    command.args(wrapped_args);
    if !enabled {
        apply_shell_path(&mut command);
    }
    Ok(command)
}

pub(crate) fn sandboxed_program(
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> io::Result<(PathBuf, Vec<String>)> {
    let (enabled, host) = sandbox_context();
    sandbox_launch(enabled, &host, program, args, cwd, env).map_err(io::Error::other)
}

fn sandbox_launch(
    enabled: bool,
    host: &DockerHost,
    program: &Path,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> Result<(PathBuf, Vec<String>), String> {
    if !enabled {
        return Ok((program.to_path_buf(), args.to_vec()));
    }
    docker_launch(host, &program.display().to_string(), args, cwd, env)
}

#[derive(Clone)]
struct DockerHost {
    resolve: fn() -> Result<PathBuf, String>,
    run: fn(&Path, &[&str]) -> io::Result<Output>,
    home: Option<PathBuf>,
    store: Option<PathBuf>,
    uid: u32,
    gid: u32,
    pass_gh_token: bool,
    boot_id: String,
    server: String,
}

impl DockerHost {
    fn process() -> Self {
        Self {
            resolve: resolve_docker,
            run: run_docker,
            home: env::var_os("HOME").map(PathBuf::from),
            store: trusted_git::default_store(),
            uid: getuid().as_raw(),
            gid: getgid().as_raw(),
            pass_gh_token: env::var_os("GH_TOKEN").is_some(),
            boot_id: SANDBOX_BOOT_ID
                .get()
                .cloned()
                .unwrap_or_else(|| "unset".to_string()),
            server: SANDBOX_SERVER_ID
                .get()
                .cloned()
                .unwrap_or_else(|| "unset".to_string()),
        }
    }
}

fn run_docker(docker: &Path, args: &[&str]) -> io::Result<Output> {
    Command::new(docker)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

fn docker_launch(
    host: &DockerHost,
    program: &str,
    args: &[String],
    cwd: &Path,
    env: &[(String, String)],
) -> Result<(PathBuf, Vec<String>), String> {
    let docker = (host.resolve)()?;
    let image = image_tag();
    check_ready(&image, |docker_args| (host.run)(&docker, docker_args))?;
    ensure_task_volumes(
        &docker,
        &image,
        &task_key(cwd),
        host.uid,
        host.gid,
        host.run,
    )?;
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
        extra_read_write: &[],
        boot_id: &host.boot_id,
        server: &host.server,
        instance: &instance,
        tty: false,
        pass_gh_token: host.pass_gh_token,
        image: &image,
        program,
        args,
        env: &env,
    };
    let argv = docker_argv(&launch, Path::exists);
    Ok((docker, argv))
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
pub(crate) const TEST_DOCKER: &str = "/ajax-test-docker";

#[cfg(test)]
thread_local! {
    static TEST_SANDBOX: std::cell::RefCell<Option<(bool, DockerHost)>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_test_sandbox<R>(enabled: bool, run: impl FnOnce() -> R) -> R {
    let host = DockerHost {
        resolve: || Ok(PathBuf::from(TEST_DOCKER)),
        run: |_, _| {
            Ok(Output {
                status: std::os::unix::process::ExitStatusExt::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        },
        home: Some(PathBuf::from("/ajax-test-home")),
        store: None,
        uid: 501,
        gid: 20,
        pass_gh_token: false,
        boot_id: "boot-test".to_string(),
        server: "server-test".to_string(),
    };
    TEST_SANDBOX.with(|slot| *slot.borrow_mut() = Some((enabled, host)));
    let result = run();
    TEST_SANDBOX.with(|slot| *slot.borrow_mut() = None);
    result
}

#[cfg(test)]
pub(crate) fn assert_probe_docker_launch(command: &Command) {
    let argv: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let dir = probe_dir().expect("probe dir").display().to_string();
    assert_eq!(command.get_program(), std::ffi::OsStr::new(TEST_DOCKER));
    assert!(
        argv.windows(2)
            .any(|pair| pair[0] == "-w" && pair[1] == dir),
        "{argv:?}"
    );
    assert!(
        argv.windows(2)
            .any(|pair| pair[0] == "-v" && pair[1] == format!("{dir}:{dir}")),
        "{argv:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

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
            &[],
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
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn linked_worktree(name: &str) -> (PathBuf, PathBuf) {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/sandbox-tests/program")
            .join(name);
        let _ = fs::remove_dir_all(&root);
        let main = root.join("main");
        let linked = root.join("linked");
        fs::create_dir_all(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(&main, &["commit", "-q", "--allow-empty", "-m", "init"]);
        let linked_text = linked.display().to_string();
        git(
            &main,
            &["worktree", "add", "-q", "-b", "ajax-linked", &linked_text],
        );
        (
            fs::canonicalize(&linked).unwrap(),
            fs::canonicalize(main.join(".git")).unwrap(),
        )
    }

    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=ajax",
                "-c",
                "user.email=ajax@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args);
        for (name, _) in env::vars_os() {
            if name.to_string_lossy().starts_with("GIT_") {
                command.env_remove(name);
            }
        }
        assert!(
            command.status().expect("git runs").success(),
            "git {args:?} failed"
        );
    }

    fn output(code: i32) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    fn fake_docker() -> Result<PathBuf, String> {
        Ok(PathBuf::from("/opt/docker"))
    }

    fn docker_ready(_: &Path, _: &[&str]) -> io::Result<Output> {
        Ok(output(0))
    }

    fn docker_down(_: &Path, _: &[&str]) -> io::Result<Output> {
        Err(io::Error::other("no socket"))
    }

    fn image_not_built(_: &Path, args: &[&str]) -> io::Result<Output> {
        Ok(output(if args[0] == "info" { 0 } else { 1 }))
    }

    fn host(home: &Path, store: Option<PathBuf>) -> DockerHost {
        DockerHost {
            resolve: fake_docker,
            run: docker_ready,
            home: Some(home.to_path_buf()),
            store,
            uid: 501,
            gid: 20,
            pass_gh_token: false,
            boot_id: "boot-test".to_string(),
            server: "server-test".to_string(),
        }
    }

    thread_local! {
        static DOCKER_CALLS: std::cell::RefCell<Vec<Vec<String>>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    fn recording_docker(_: &Path, args: &[&str]) -> io::Result<Output> {
        DOCKER_CALLS.with(|calls| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| arg.to_string()).collect());
        });
        Ok(output(0))
    }

    fn has_pair(argv: &[String], flag: &str, value: &str) -> bool {
        argv.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    #[test]
    fn an_enabled_launch_wraps_the_agent_in_docker_with_the_worktree_and_image() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let args = vec!["exec".to_string()];

        let (program, argv) = sandbox_launch(
            true,
            &host(&scratch_dir("home-argv"), None),
            Path::new("codex"),
            &args,
            cwd,
            &[],
        )
        .expect("enabled launch wraps");

        assert_eq!(program, PathBuf::from("/opt/docker"));
        let cwd_text = cwd.display().to_string();
        assert!(has_pair(&argv, "--network", "host"));
        assert!(has_pair(&argv, "--user", "501:20"));
        assert!(has_pair(&argv, "-v", &format!("{cwd_text}:{cwd_text}")));
        let tail = [
            image_tag().as_str(),
            "codex",
            "-c",
            "sandbox_mode=\"danger-full-access\"",
            "exec",
        ]
        .map(String::from);
        assert!(argv.ends_with(&tail));
    }

    #[test]
    fn an_enabled_launch_ensures_its_task_volumes_after_the_image_is_ready() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let launch_host = DockerHost {
            run: recording_docker,
            ..host(&scratch_dir("home-volumes"), None)
        };

        sandbox_launch(true, &launch_host, Path::new("codex"), &[], cwd, &[])
            .expect("enabled launch wraps");

        let calls = DOCKER_CALLS.with(|calls| calls.take());
        let target = format!("ajax-target-{}", task_key(cwd));
        let ready = calls
            .iter()
            .position(|call| call[0] == "image")
            .expect("image is checked");
        let inspected = calls
            .iter()
            .position(|call| call == &["volume", "inspect", target.as_str()])
            .expect("target volume is inspected");
        assert!(ready < inspected, "{calls:?}");
    }

    #[test]
    fn an_enabled_launch_fails_closed_when_docker_is_missing() {
        let launch_host = DockerHost {
            resolve: || {
                Err("agent sandbox is on but docker is not installed; install OrbStack or set AJAX_SANDBOX=off".to_string())
            },
            run: |_, _| panic!("docker must not run without a client"),
            ..host(&scratch_dir("home-missing"), None)
        };

        let error = sandbox_launch(
            true,
            &launch_host,
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
            &[],
        )
        .expect_err("a missing docker must refuse the launch");

        assert!(error.contains("docker is not installed"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }

    #[test]
    fn an_enabled_launch_is_refused_when_the_docker_daemon_is_unreachable() {
        let launch_host = DockerHost {
            run: docker_down,
            ..host(&scratch_dir("home-down"), None)
        };

        let error = sandbox_launch(
            true,
            &launch_host,
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
            &[],
        )
        .expect_err("an unreachable daemon must refuse the launch");

        assert!(error.contains("Docker daemon is not reachable"), "{error}");
    }

    #[test]
    fn an_enabled_launch_is_refused_when_the_image_is_not_built() {
        let launch_host = DockerHost {
            run: image_not_built,
            ..host(&scratch_dir("home-unbuilt"), None)
        };

        let error = sandbox_launch(
            true,
            &launch_host,
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
            &[],
        )
        .expect_err("an unbuilt image must refuse the launch");

        assert!(error.contains("is not built yet"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }

    #[test]
    fn a_linked_worktree_mounts_its_common_dir_and_records_trusted_git() {
        let (linked, common) = linked_worktree("linked-argv");
        let store = scratch_dir("linked-argv-store");

        let (_, argv) = sandbox_launch(
            true,
            &host(&scratch_dir("home-linked"), Some(store.clone())),
            Path::new("codex"),
            &[],
            &linked,
            &[],
        )
        .expect("linked launch wraps");

        let common_text = common.display().to_string();
        assert!(has_pair(
            &argv,
            "-v",
            &format!("{common_text}:{common_text}")
        ));
        let trusted = trusted_git::lookup(&store, &linked).expect("trusted git is recorded");
        assert_eq!(trusted.common_dir, common);
        assert_eq!(trusted.work_tree, linked);
    }

    #[test]
    fn a_linked_launch_fails_closed_when_trusted_git_cannot_be_recorded() {
        let (linked, _) = linked_worktree("linked-blocked");
        let blocker = scratch_dir("linked-blocked-store").join("not-a-dir");
        fs::write(&blocker, "file").unwrap();

        let error = sandbox_launch(
            true,
            &host(&scratch_dir("home-blocked"), Some(blocker)),
            Path::new("codex"),
            &[],
            &linked,
            &[],
        )
        .expect_err("an unrecordable trusted git must refuse the launch");

        assert!(error.contains("trusted git metadata"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }

    #[test]
    fn an_enabled_launch_fails_closed_without_a_home_directory() {
        let launch_host = DockerHost {
            home: None,
            ..host(&scratch_dir("home-none"), None)
        };

        let error = sandbox_launch(
            true,
            &launch_host,
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
            &[],
        )
        .expect_err("no home must refuse the launch");

        assert!(error.contains("HOME"), "{error}");
        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
    }

    #[test]
    fn caller_env_reaches_the_container_but_the_host_path_does_not() {
        let env = [
            ("GIT_AUTHOR_NAME".to_string(), "ajax".to_string()),
            ("PATH".to_string(), "/host/bin".to_string()),
        ];

        let (_, argv) = sandbox_launch(
            true,
            &host(&scratch_dir("home-env"), None),
            Path::new("codex"),
            &[],
            Path::new("/work/task"),
            &env,
        )
        .expect("launch with env");

        assert!(has_pair(&argv, "-e", "GIT_AUTHOR_NAME=ajax"));
        assert!(!argv.iter().any(|arg| arg.starts_with("PATH=")));
    }

    #[test]
    fn a_docker_harness_is_the_bare_program_with_no_host_install_or_host_path() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let args = vec!["exec".to_string()];

        let command = harness_launch(
            true,
            &host(&scratch_dir("home-harness"), None),
            "ajax-not-a-real-program",
            &args,
            cwd,
            &[],
        )
        .expect("a docker launch needs no host install");

        assert_eq!(command.get_program(), OsStr::new("/opt/docker"));
        let argv: Vec<String> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(argv.ends_with(&["ajax-not-a-real-program".to_string(), "exec".to_string()]));
        assert!(command.get_envs().all(|(key, _)| key != OsStr::new("PATH")));
    }

    fn container_name(argv: &[String]) -> &str {
        argv.windows(2)
            .find(|pair| pair[0] == "--name")
            .map_or("", |pair| pair[1].as_str())
    }

    #[test]
    fn two_launches_for_one_worktree_get_distinct_container_names() {
        let cwd = Path::new("/work/task");
        let args = vec!["exec".to_string()];
        let home = scratch_dir("home-names");
        let launch_host = host(&home, None);

        let (_, first) = sandbox_launch(true, &launch_host, Path::new("codex"), &args, cwd, &[])
            .expect("first launch wraps");
        let (_, second) = sandbox_launch(true, &launch_host, Path::new("codex"), &args, cwd, &[])
            .expect("second launch wraps");

        assert_ne!(container_name(&first), container_name(&second));
        assert!(container_name(&first).starts_with("ajax-"));
        assert!(container_name(&second).starts_with("ajax-"));
    }

    #[test]
    fn a_sandboxed_probe_is_a_docker_launch_in_the_probe_directory_with_no_host_binary() {
        let command = with_test_sandbox(true, || {
            probe_command("ajax-not-a-real-program", &["auth", "status"])
        })
        .expect("a docker launch needs no host install");

        assert_probe_docker_launch(&command);
        assert!(command
            .get_args()
            .any(|arg| arg == "ajax-not-a-real-program"));
    }

    #[test]
    fn a_sandboxed_launch_labels_its_container_with_the_server_id() {
        let command = with_test_sandbox(true, || {
            probe_command("ajax-not-a-real-program", &["auth", "status"])
        })
        .expect("a docker launch needs no host install");

        assert!(command
            .get_args()
            .any(|arg| arg == "ajax.server=server-test"));
    }

    #[test]
    fn a_probe_without_the_sandbox_still_needs_the_host_binary() {
        let error = with_test_sandbox(false, || probe_command("ajax-not-a-real-program", &[]))
            .expect_err("a host probe needs the host binary");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_disabled_launch_is_the_identity() {
        let args = vec!["--mode".to_string(), "rpc".to_string()];

        let launched = sandbox_launch(
            false,
            &host(&scratch_dir("home-disabled"), None),
            Path::new("node"),
            &args,
            Path::new("/work/task"),
            &[],
        )
        .expect("disabled launch is never an error");

        assert_eq!(launched, (PathBuf::from("node"), args));
    }
}
