use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

pub const DOCKERFILE: &str = include_str!("agent.Dockerfile");

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;
const CODEX_SANDBOX_OVERRIDE: [&str; 2] = ["-c", "sandbox_mode=\"danger-full-access\""];
const SSH_AGENT_SOCK: &str = ".config/ajax/ssh/agent.sock";
const SHARED_HARNESS_HOMES: [&str; 3] = [".claude", ".codex", ".cursor"];
const GH_CONFIG: &str = ".config/gh";
const READ_ONLY_TOOLING: [&str; 8] = [
    "Projects/ajax-model-router",
    ".agents/skills",
    "Projects/AGENTS.md",
    "Projects/agents.md",
    "Projects/CLAUDE.md",
    "Projects/.claude",
    "Projects/.codex",
    "Projects/scripts",
];
const READ_WRITE_TOOLING: [&str; 5] = [
    ".ajax-router",
    ".pi/agent",
    ".local/share/opencode",
    ".config/opencode",
    SSH_AGENT_SOCK,
];
const DOCKER_MISSING: &str =
    "agent sandbox is on but docker is not installed; install OrbStack or set AJAX_SANDBOX=off";
const DAEMON_UNREACHABLE: &str =
    "agent sandbox is on but the Docker daemon is not reachable; start OrbStack or set AJAX_SANDBOX=off";
const AJAX_SANDBOX_OFF: &str = "set AJAX_SANDBOX=off to run unsandboxed";

pub fn stable_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

pub fn image_tag() -> String {
    format!("ajax-agent:{:016x}", stable_hash(DOCKERFILE.as_bytes()))
}

pub fn task_key(cwd: &Path) -> String {
    let name = cwd.file_name().map_or_else(String::new, |name| {
        name.to_string_lossy().to_ascii_lowercase()
    });
    let slug: String = name
        .chars()
        .take(40)
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() {
                c
            } else {
                '-'
            }
        })
        .collect();
    let digest = stable_hash(cwd.as_os_str().as_encoded_bytes()) & 0xffff_ffff;
    format!("{slug}-{digest:08x}")
}

pub fn task_volumes(key: &str) -> [String; 2] {
    [
        format!("ajax-target-{key}"),
        format!("ajax-node-modules-{key}"),
    ]
}

pub fn server_id(state_dir: &Path) -> String {
    format!(
        "{:016x}",
        stable_hash(state_dir.as_os_str().as_encoded_bytes())
    )
}

pub fn ps_argv(server: &str, key: &str) -> Vec<String> {
    let mut argv = strings(&["ps", "--no-trunc"]);
    argv.extend(pair("--filter", format!("label=ajax.server={server}")));
    argv.extend(pair("--filter", format!("label=ajax.task_key={key}")));
    argv.extend(pair(
        "--format",
        "{{.ID}} {{.Label \"ajax.boot\"}} {{.Label \"ajax.server\"}}",
    ));
    argv
}

pub fn top_argv(container: &str) -> Vec<String> {
    let mut argv = strings(&["top"]);
    argv.push(container.to_string());
    argv.extend(strings(&["-eo", "pid,ppid,comm"]));
    argv
}

pub fn rm_argv(container: &str) -> Vec<String> {
    rm_containers_argv(&[container.to_string()])
}

pub fn rm_containers_argv(containers: &[String]) -> Vec<String> {
    let mut argv = strings(&["rm", "-f"]);
    argv.extend(containers.iter().cloned());
    argv
}

pub fn task_containers_argv(key: &str) -> Vec<String> {
    let mut argv = strings(&["ps", "-aq"]);
    argv.extend(pair("--filter", format!("label=ajax.task_key={key}")));
    argv
}

pub fn volume_rm_argv(volumes: &[String]) -> Vec<String> {
    let mut argv = strings(&["volume", "rm", "-f"]);
    argv.extend(volumes.iter().cloned());
    argv
}

pub fn remove_task_resources(key: &str) {
    if cfg!(test) {
        return;
    }
    let Ok(docker) = resolve_docker() else {
        return;
    };
    let listing = run_quietly(&docker, &task_containers_argv(key)).unwrap_or_default();
    let containers: Vec<String> = listing.split_whitespace().map(String::from).collect();
    if !containers.is_empty() {
        run_quietly(&docker, &rm_containers_argv(&containers));
    }
    run_quietly(&docker, &volume_rm_argv(&task_volumes(key)));
}

fn run_quietly(docker: &Path, argv: &[String]) -> Option<String> {
    let output = Command::new(docker)
        .args(argv)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Debug, PartialEq, Eq)]
pub struct ContainerRow {
    pub id: String,
    pub boot_id: String,
    pub server: String,
}

pub fn parse_ps(output: &str) -> Vec<ContainerRow> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some(ContainerRow {
                id: fields.next()?.to_string(),
                boot_id: fields.next()?.to_string(),
                server: fields.next()?.to_string(),
            })
        })
        .collect()
}

pub struct DockerLaunch<'a> {
    pub home: &'a Path,
    pub uid: u32,
    pub gid: u32,
    pub cwd: &'a Path,
    pub git_common_dir: Option<&'a Path>,
    pub extra_read_write: &'a [PathBuf],
    pub boot_id: &'a str,
    pub server: &'a str,
    pub instance: &'a str,
    pub tty: bool,
    pub pass_gh_token: bool,
    pub image: &'a str,
    pub program: &'a str,
    pub args: &'a [String],
    pub env: &'a [(String, String)],
}

pub fn docker_argv(launch: &DockerLaunch<'_>, exists: impl Fn(&Path) -> bool) -> Vec<String> {
    let key = task_key(launch.cwd);
    let cwd = text(launch.cwd);
    let home = text(launch.home);
    let sock = launch.home.join(SSH_AGENT_SOCK);
    let mut argv = strings(&["run", "--rm", "-i"]);
    if launch.tty {
        argv.push("-t".to_string());
    }
    argv.extend(strings(&["--init", "--user"]));
    argv.push(format!("{}:{}", launch.uid, launch.gid));
    argv.extend(strings(&["--network", "host", "--name"]));
    argv.push(format!("ajax-{key}-{}", launch.instance));
    argv.extend(pair("--label", format!("ajax.boot={}", launch.boot_id)));
    argv.extend(pair("--label", format!("ajax.task_key={key}")));
    argv.extend(pair("--label", format!("ajax.server={}", launch.server)));
    argv.extend(pair("--label", format!("ajax.worktree={cwd}")));
    argv.extend(pair("-e", format!("HOME={home}")));
    if exists(&sock) {
        argv.extend(pair("-e", format!("SSH_AUTH_SOCK={}", text(&sock))));
    }
    if launch.pass_gh_token {
        argv.extend(pair("-e", "GH_TOKEN"));
    }
    for (key, value) in launch.env {
        argv.extend(pair("-e", format!("{key}={value}")));
    }
    argv.extend(pair("-w", cwd.as_str()));
    argv.extend(pair("-v", format!("ajax-agent-home:{home}")));
    argv.extend(pair("-v", "ajax-rustup:/opt/rustup"));
    argv.extend(pair("-v", "ajax-cargo-registry:/opt/cargo/registry"));
    argv.extend(pair("-v", "ajax-cargo-git:/opt/cargo/git"));
    argv.extend(pair("-v", same_path(launch.cwd)));
    if let Some(common) = launch.git_common_dir {
        argv.extend(pair("-v", same_path(common)));
    }
    for path in launch.extra_read_write {
        argv.extend(pair("-v", same_path(path)));
    }
    let [target, node_modules] = task_volumes(&key);
    argv.extend(pair("-v", format!("{target}:{cwd}/target")));
    argv.extend(pair("-v", format!("{node_modules}:{cwd}/node_modules")));
    for relative in SHARED_HARNESS_HOMES.iter() {
        let path = launch.home.join(relative);
        if exists(&path) {
            argv.extend(pair("-v", same_path(&path)));
        }
    }
    let gh = launch.home.join(GH_CONFIG);
    if exists(&gh) {
        argv.extend(pair("-v", format!("{0}:{0}:ro", text(&gh))));
    }
    let tooling = READ_ONLY_TOOLING
        .iter()
        .map(|relative| (relative, ":ro"))
        .chain(READ_WRITE_TOOLING.iter().map(|relative| (relative, "")));
    for (relative, mode) in tooling {
        let path = launch.home.join(relative);
        if exists(&path) {
            argv.extend(pair("-v", format!("{0}:{0}{mode}", text(&path))));
        }
    }
    let file_name = Path::new(launch.program).file_name();
    argv.push(launch.image.to_string());
    argv.push(file_name.map_or_else(String::new, |name| name.to_string_lossy().into_owned()));
    if file_name == Some(OsStr::new("codex")) {
        argv.extend(CODEX_SANDBOX_OVERRIDE.map(String::from));
    }
    argv.extend(launch.args.iter().cloned());
    argv
}

pub fn volume_init_argv(image: &str, volume: &str, uid: u32, gid: u32) -> Vec<String> {
    let mut argv = strings(&["run", "--rm", "--user", "0"]);
    argv.extend(pair("-v", format!("{volume}:/v")));
    argv.push(image.to_string());
    argv.push("chown".to_string());
    argv.push(format!("{uid}:{gid}"));
    argv.push("/v".to_string());
    argv
}

pub fn build_argv(tag: &str, uid: u32, gid: u32, home: &Path, context_dir: &Path) -> Vec<String> {
    let mut argv = strings(&["build"]);
    argv.extend(pair("--build-arg", format!("HOST_UID={uid}")));
    argv.extend(pair("--build-arg", format!("HOST_GID={gid}")));
    argv.extend(pair("--build-arg", format!("HOST_HOME={}", text(home))));
    argv.extend(pair("-t", tag));
    argv.extend(pair("-f", "-"));
    argv.push(text(context_dir));
    argv
}

pub fn resolve_docker() -> Result<PathBuf, String> {
    let path = env::var_os("PATH").unwrap_or_default();
    let orbstack = env::var_os("HOME").map(|home| PathBuf::from(home).join(".orbstack/bin/docker"));
    env::split_paths(&path)
        .map(|dir| dir.join("docker"))
        .chain(["/usr/local/bin/docker", "/opt/homebrew/bin/docker"].map(PathBuf::from))
        .chain(orbstack)
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| DOCKER_MISSING.to_string())
}

pub fn check_ready(tag: &str, run: impl Fn(&[&str]) -> io::Result<Output>) -> Result<(), String> {
    if !succeeded(run(&["info", "--format", "{{.ServerVersion}}"])) {
        return Err(DAEMON_UNREACHABLE.to_string());
    }
    if !succeeded(run(&["image", "inspect", "--format", "{{.Id}}", tag])) {
        return Err(format!(
            "agent sandbox image {tag} is not built yet; it builds when the Ajax web server starts. Retry shortly or set AJAX_SANDBOX=off"
        ));
    }
    Ok(())
}

pub fn ensure_task_volumes(
    docker: &Path,
    image: &str,
    key: &str,
    uid: u32,
    gid: u32,
    run: impl Fn(&Path, &[&str]) -> io::Result<Output>,
) -> Result<(), String> {
    for volume in task_volumes(key) {
        if succeeded(run(docker, &["volume", "inspect", volume.as_str()])) {
            continue;
        }
        if !succeeded(run(docker, &["volume", "create", volume.as_str()])) {
            return Err(volume_error("create", &volume));
        }
        let init = volume_init_argv(image, &volume, uid, gid);
        let init_args: Vec<&str> = init.iter().map(String::as_str).collect();
        if !succeeded(run(docker, &init_args)) {
            return Err(volume_error("chown", &volume));
        }
    }
    Ok(())
}

fn volume_error(step: &str, volume: &str) -> String {
    format!("agent sandbox could not {step} volume {volume}; {AJAX_SANDBOX_OFF}")
}

fn succeeded(result: io::Result<Output>) -> bool {
    result.is_ok_and(|output| output.status.success())
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_string()).collect()
}

fn pair(flag: &str, value: impl Into<String>) -> [String; 2] {
    [flag.to_string(), value.into()]
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

fn same_path(path: &Path) -> String {
    let path = text(path);
    format!("{path}:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::fs;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const UID: u32 = 501;
    const GID: u32 = 20;

    const HOME: &str = "/Users/matt";
    const CWD: &str = "/Users/matt/Projects/ajax-cli__worktrees/ajax-docker-sandboxes";
    const COMMON: &str = "/Users/matt/Projects/ajax-cli/.git";
    const KEY: &str = "ajax-docker-sandboxes-1576734c";
    const IMAGE: &str = "ajax-agent:test";
    const SOCK: &str = "/Users/matt/.config/ajax/ssh/agent.sock";
    const INSTANCE: &str = "1f-2";
    const SERVER: &str = "5a1c0e2f9b3d4e61";

    fn sample<'a>(home: &'a Path, cwd: &'a Path, args: &'a [String]) -> DockerLaunch<'a> {
        DockerLaunch {
            home,
            uid: 501,
            gid: 20,
            cwd,
            git_common_dir: Some(Path::new(COMMON)),
            extra_read_write: &[],
            boot_id: "boot-1",
            server: SERVER,
            instance: INSTANCE,
            tty: false,
            pass_gh_token: false,
            image: IMAGE,
            program: "claude",
            args,
            env: &[],
        }
    }

    fn words(line: &str) -> Vec<String> {
        line.split_whitespace().map(String::from).collect()
    }

    fn has_pair(argv: &[String], flag: &str, value: &str) -> bool {
        argv.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    fn output(code: i32) -> Output {
        Output {
            status: ExitStatus::from_raw(code << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn stable_hash_matches_fnv1a_vectors() {
        assert_eq!(stable_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(stable_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn image_tag_is_ajax_agent_with_sixteen_hex_digits() {
        let tag = image_tag();
        let digest = tag.strip_prefix("ajax-agent:").expect("ajax-agent prefix");
        assert_eq!(digest.len(), 16);
        assert!(digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    }

    #[test]
    fn task_key_is_stable_and_docker_safe() {
        assert_eq!(task_key(Path::new(CWD)), KEY);
        assert_eq!(task_key(Path::new(CWD)), task_key(Path::new(CWD)));

        let messy = task_key(Path::new("/x/My_Repo.v2"));
        assert!(messy.starts_with("my-repo-v2-"));
        assert!(messy
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'));

        let long = task_key(Path::new(&format!("/x/{}", "a".repeat(60))));
        assert_eq!(long.len(), 40 + 1 + 8);
        assert!(long.starts_with(&"a".repeat(40)));
        assert_ne!(
            task_key(Path::new("/x/repo")),
            task_key(Path::new("/y/repo"))
        );
    }

    #[test]
    fn linked_worktree_full_argv_in_order() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&["-p", "hi"]);
        let argv = docker_argv(&sample(home, cwd, &args), |_| true);
        let expected = words(&format!(
            "run --rm -i --init --user 501:20 --network host --name ajax-{KEY}-{INSTANCE} \
             --label ajax.boot=boot-1 --label ajax.task_key={KEY} --label ajax.server={SERVER} --label ajax.worktree={CWD} -e HOME={HOME} \
             -e SSH_AUTH_SOCK={SOCK} -w {CWD} -v ajax-agent-home:{HOME} \
             -v ajax-rustup:/opt/rustup -v ajax-cargo-registry:/opt/cargo/registry \
             -v ajax-cargo-git:/opt/cargo/git -v {CWD}:{CWD} -v {COMMON}:{COMMON} \
             -v ajax-target-{KEY}:{CWD}/target -v ajax-node-modules-{KEY}:{CWD}/node_modules \
             -v {HOME}/.claude:{HOME}/.claude -v {HOME}/.codex:{HOME}/.codex \
             -v {HOME}/.cursor:{HOME}/.cursor -v {HOME}/.config/gh:{HOME}/.config/gh:ro \
             -v {HOME}/Projects/ajax-model-router:{HOME}/Projects/ajax-model-router:ro \
             -v {HOME}/.agents/skills:{HOME}/.agents/skills:ro \
             -v {HOME}/Projects/AGENTS.md:{HOME}/Projects/AGENTS.md:ro \
             -v {HOME}/Projects/agents.md:{HOME}/Projects/agents.md:ro \
             -v {HOME}/Projects/CLAUDE.md:{HOME}/Projects/CLAUDE.md:ro \
             -v {HOME}/Projects/.claude:{HOME}/Projects/.claude:ro \
             -v {HOME}/Projects/.codex:{HOME}/Projects/.codex:ro \
             -v {HOME}/Projects/scripts:{HOME}/Projects/scripts:ro \
             -v {HOME}/.ajax-router:{HOME}/.ajax-router -v {HOME}/.pi/agent:{HOME}/.pi/agent \
             -v {HOME}/.local/share/opencode:{HOME}/.local/share/opencode \
             -v {HOME}/.config/opencode:{HOME}/.config/opencode \
             -v {SOCK}:{SOCK} {IMAGE} claude -p hi"
        ));
        assert_eq!(argv, expected);
    }

    #[test]
    fn plain_worktree_tty_with_extra_read_write_argv() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&["--print"]);
        let extra = [PathBuf::from("/var/folders/ajax/events")];
        let launch = DockerLaunch {
            tty: true,
            git_common_dir: None,
            extra_read_write: &extra,
            ..sample(home, cwd, &args)
        };
        let argv = docker_argv(&launch, |_| false);
        let expected = words(&format!(
            "run --rm -i -t --init --user 501:20 --network host --name ajax-{KEY}-{INSTANCE} \
             --label ajax.boot=boot-1 --label ajax.task_key={KEY} --label ajax.server={SERVER} --label ajax.worktree={CWD} -e HOME={HOME} -w {CWD} \
             -v ajax-agent-home:{HOME} -v ajax-rustup:/opt/rustup -v ajax-cargo-registry:/opt/cargo/registry \
             -v ajax-cargo-git:/opt/cargo/git -v {CWD}:{CWD} -v /var/folders/ajax/events:/var/folders/ajax/events \
             -v ajax-target-{KEY}:{CWD}/target -v ajax-node-modules-{KEY}:{CWD}/node_modules \
             {IMAGE} claude --print"
        ));
        assert_eq!(argv, expected);
    }

    fn container_name(argv: &[String]) -> &str {
        argv.windows(2)
            .find(|pair| pair[0] == "--name")
            .map_or("", |pair| pair[1].as_str())
    }

    fn volumes(argv: &[String]) -> Vec<&String> {
        argv.windows(2)
            .filter(|pair| pair[0] == "-v")
            .map(|pair| &pair[1])
            .collect()
    }

    #[test]
    fn two_launches_for_one_worktree_get_distinct_names_and_shared_volumes() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let first = docker_argv(&sample(home, cwd, &args), |_| false);
        let second_launch = DockerLaunch {
            instance: "1f-3",
            ..sample(home, cwd, &args)
        };
        let second = docker_argv(&second_launch, |_| false);

        assert_eq!(container_name(&first), format!("ajax-{KEY}-{INSTANCE}"));
        assert_eq!(container_name(&second), format!("ajax-{KEY}-1f-3"));
        assert!(!volumes(&first).is_empty());
        assert_eq!(volumes(&first), volumes(&second));
    }

    #[test]
    fn missing_tooling_paths_are_skipped() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let argv = docker_argv(&sample(home, cwd, &args), |_| false);
        assert!(!argv.iter().any(|a| a.starts_with("SSH_AUTH_SOCK")));
        assert!(!argv.iter().any(|a| a.contains("/.codex/skills")));

        let agents_skills = PathBuf::from(format!("{HOME}/.agents/skills"));
        let argv = docker_argv(&sample(home, cwd, &args), |p| p == agents_skills);
        assert!(has_pair(
            &argv,
            "-v",
            &format!("{HOME}/.agents/skills:{HOME}/.agents/skills:ro")
        ));
        assert!(!argv.iter().any(|a| a.contains("ajax-model-router")));
    }

    #[test]
    fn gh_token_and_ssh_socket_only_when_applicable() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let on = DockerLaunch {
            pass_gh_token: true,
            ..sample(home, cwd, &args)
        };
        let argv = docker_argv(&on, |p| p == Path::new(SOCK));
        assert!(has_pair(&argv, "-e", "GH_TOKEN"));
        assert!(has_pair(&argv, "-e", &format!("SSH_AUTH_SOCK={SOCK}")));

        let argv = docker_argv(&sample(home, cwd, &args), |_| false);
        assert!(!has_pair(&argv, "-e", "GH_TOKEN"));
        assert!(!argv.iter().any(|a| a.starts_with("SSH_AUTH_SOCK=")));
    }

    #[test]
    fn caller_env_follows_the_github_token_as_ordered_e_flags() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let env = [
            ("A".to_string(), "1".to_string()),
            ("B".to_string(), "two words".to_string()),
        ];
        let launch = DockerLaunch {
            pass_gh_token: true,
            env: &env,
            ..sample(home, cwd, &args)
        };
        let argv = docker_argv(&launch, |_| false);
        let expected = strings(&["-e", "GH_TOKEN", "-e", "A=1", "-e", "B=two words"]);
        assert!(argv
            .windows(expected.len())
            .any(|window| window == expected));
    }

    #[test]
    fn absolute_program_is_reduced_to_file_name() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&["-x"]);
        let launch = DockerLaunch {
            program: "/opt/homebrew/bin/claude",
            ..sample(home, cwd, &args)
        };
        let argv = docker_argv(&launch, |_| false);
        let at = argv
            .iter()
            .position(|a| a == IMAGE)
            .expect("image argument");
        assert_eq!(argv[at + 1], "claude");
        assert_eq!(argv[at + 2], "-x");
        assert!(!argv.iter().any(|a| a.contains("/opt/homebrew")));
    }

    #[test]
    fn codex_gets_sandbox_override_but_not_codex_acp() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&["exec"]);
        for program in ["codex", "/x/codex"] {
            let launch = DockerLaunch {
                program,
                ..sample(home, cwd, &args)
            };
            let argv = docker_argv(&launch, |_| false);
            let at = argv
                .iter()
                .position(|a| a == IMAGE)
                .expect("image argument");
            assert_eq!(
                argv[at + 1..],
                strings(&["codex", "-c", "sandbox_mode=\"danger-full-access\"", "exec"])
            );
        }
        for program in ["codex-acp", "claude"] {
            let launch = DockerLaunch {
                program,
                ..sample(home, cwd, &args)
            };
            let argv = docker_argv(&launch, |_| false);
            let at = argv
                .iter()
                .position(|a| a == IMAGE)
                .expect("image argument");
            assert_eq!(argv[at + 1..], strings(&[program, "exec"]));
        }
    }

    #[test]
    fn never_mounts_host_secrets_sockets_or_state() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let argv = docker_argv(&sample(home, cwd, &args), |_| true);
        let forbidden = [
            format!("{HOME}/.ssh"),
            format!("{HOME}/.orbstack"),
            format!("{HOME}/.docker"),
            "/var/run/docker.sock".to_string(),
            "/private/var/run/docker.sock".to_string(),
            "/private/tmp/tmux-".to_string(),
            "/tmp/tmux-".to_string(),
            format!("{HOME}/.local/state/ajax"),
            format!("{HOME}/.ajax-dev"),
            format!("{HOME}/.config/ajax"),
        ];
        let sources = argv
            .windows(2)
            .filter(|w| w[0] == "-v")
            .map(|w| w[1].split(':').next().unwrap_or_default());
        for source in sources {
            if source == SOCK {
                continue;
            }
            for prefix in &forbidden {
                assert!(
                    !source.starts_with(prefix.as_str()),
                    "{source} must not be mounted"
                );
            }
        }
    }

    #[test]
    fn shared_harness_homes_mount_read_write_and_gh_read_only() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let argv = docker_argv(&sample(home, cwd, &args), |_| true);
        for harness in [".claude", ".codex", ".cursor"] {
            assert!(has_pair(
                &argv,
                "-v",
                &format!("{HOME}/{harness}:{HOME}/{harness}")
            ));
        }
        assert!(has_pair(
            &argv,
            "-v",
            &format!("{HOME}/.config/gh:{HOME}/.config/gh:ro")
        ));
        for volume in volumes(&argv) {
            let source = volume.split(':').next().unwrap_or_default();
            assert!(
                !source.ends_with(".credentials.json") && !source.ends_with("auth.json"),
                "{source} must not be mounted"
            );
        }
    }

    #[test]
    fn volume_init_argv_is_exact() {
        assert_eq!(
            volume_init_argv(IMAGE, "ajax-target-x", 501, 20),
            strings(&[
                "run",
                "--rm",
                "--user",
                "0",
                "-v",
                "ajax-target-x:/v",
                IMAGE,
                "chown",
                "501:20",
                "/v",
            ])
        );
    }

    #[test]
    fn build_argv_is_exact() {
        assert_eq!(
            build_argv(IMAGE, 501, 20, Path::new(HOME), Path::new("/ctx")),
            strings(&[
                "build",
                "--build-arg",
                "HOST_UID=501",
                "--build-arg",
                "HOST_GID=20",
                "--build-arg",
                "HOST_HOME=/Users/matt",
                "-t",
                IMAGE,
                "-f",
                "-",
                "/ctx",
            ])
        );
    }

    #[test]
    fn check_ready_reports_each_outcome() {
        let daemon_down = check_ready(IMAGE, |args| {
            assert_eq!(args.join(" "), "info --format {{.ServerVersion}}");
            Err(io::Error::other("no socket"))
        });
        assert_eq!(daemon_down, Err(DAEMON_UNREACHABLE.to_string()));

        let daemon_failed = check_ready(IMAGE, |_| Ok(output(1)));
        assert_eq!(daemon_failed, Err(DAEMON_UNREACHABLE.to_string()));

        let image_missing = check_ready(IMAGE, |args| {
            if args[0] == "info" {
                return Ok(output(0));
            }
            assert_eq!(
                args.join(" "),
                "image inspect --format {{.Id}} ajax-agent:test"
            );
            Ok(output(1))
        });
        assert_eq!(
            image_missing,
            Err(format!(
                "agent sandbox image {IMAGE} is not built yet; it builds when the Ajax web server starts. Retry shortly or set AJAX_SANDBOX=off"
            ))
        );

        assert_eq!(check_ready(IMAGE, |_| Ok(output(0))), Ok(()));
    }

    #[test]
    fn server_id_is_a_stable_hex_digest_of_the_state_dir() {
        let id = server_id(Path::new("/Users/matt/.local/state/ajax"));

        assert_eq!(id, server_id(Path::new("/Users/matt/.local/state/ajax")));
        assert_eq!(id.len(), 16);
        assert!(id.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(id, server_id(Path::new("/Users/matt/.ajax-dev")));
    }

    #[test]
    fn ps_top_and_rm_argv_are_exact() {
        assert_eq!(
            ps_argv("srv", "key"),
            strings(&[
                "ps",
                "--no-trunc",
                "--filter",
                "label=ajax.server=srv",
                "--filter",
                "label=ajax.task_key=key",
                "--format",
                "{{.ID}} {{.Label \"ajax.boot\"}} {{.Label \"ajax.server\"}}",
            ])
        );
        assert_eq!(
            top_argv("abc"),
            strings(&["top", "abc", "-eo", "pid,ppid,comm"])
        );
        assert_eq!(rm_argv("abc"), strings(&["rm", "-f", "abc"]));
    }

    #[test]
    fn task_container_and_volume_removal_argv_are_exact() {
        assert_eq!(
            task_containers_argv("key"),
            strings(&["ps", "-aq", "--filter", "label=ajax.task_key=key"])
        );
        assert_eq!(
            rm_containers_argv(&strings(&["c1", "c2"])),
            strings(&["rm", "-f", "c1", "c2"])
        );
        assert_eq!(
            volume_rm_argv(&task_volumes("key")),
            strings(&[
                "volume",
                "rm",
                "-f",
                "ajax-target-key",
                "ajax-node-modules-key",
            ])
        );
    }

    #[test]
    fn parse_ps_reads_id_boot_and_server_and_skips_malformed_rows() {
        let rows = parse_ps("c1 boot-1 srv\nc2 boot-2 srv\n\nbroken\n");

        assert_eq!(
            rows,
            vec![
                ContainerRow {
                    id: "c1".to_string(),
                    boot_id: "boot-1".to_string(),
                    server: "srv".to_string(),
                },
                ContainerRow {
                    id: "c2".to_string(),
                    boot_id: "boot-2".to_string(),
                    server: "srv".to_string(),
                },
            ]
        );
    }

    fn playwright_base(text: &str) -> &str {
        let start = text
            .find("mcr.microsoft.com/playwright:")
            .expect("playwright image reference");
        let rest = &text[start..];
        let end = rest
            .find(|c: char| c == '@' || c.is_whitespace())
            .unwrap_or(rest.len());
        &rest[..end]
    }

    #[test]
    fn dockerfile_playwright_base_matches_ci_pin() {
        let ci = fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.github/workflows/ci.yml"
        ))
        .expect("read ci.yml");
        assert_eq!(playwright_base(DOCKERFILE), playwright_base(&ci));
    }

    type VolumeCalls = RefCell<Vec<Vec<String>>>;

    fn volume_recorder<'a>(
        calls: &'a VolumeCalls,
        respond: impl Fn(&[&str]) -> Output + 'a,
    ) -> impl Fn(&Path, &[&str]) -> io::Result<Output> + 'a {
        move |_, args| {
            calls
                .borrow_mut()
                .push(args.iter().map(|arg| arg.to_string()).collect());
            Ok(respond(args))
        }
    }

    fn ensure(run: impl Fn(&Path, &[&str]) -> io::Result<Output>) -> Result<(), String> {
        ensure_task_volumes(Path::new("/opt/docker"), IMAGE, KEY, UID, GID, run)
    }

    fn target() -> String {
        format!("ajax-target-{KEY}")
    }

    fn node_modules() -> String {
        format!("ajax-node-modules-{KEY}")
    }

    #[test]
    fn existing_task_volumes_are_left_alone() {
        let calls = VolumeCalls::default();

        ensure(volume_recorder(&calls, |_| output(0))).expect("existing volumes");

        assert_eq!(
            calls.into_inner(),
            vec![
                strings(&["volume", "inspect", &target()]),
                strings(&["volume", "inspect", &node_modules()]),
            ]
        );
    }

    #[test]
    fn missing_task_volumes_are_created_then_chowned_in_order() {
        let calls = VolumeCalls::default();

        ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "volume" && args[1] == "inspect" {
                1
            } else {
                0
            })
        }))
        .expect("missing volumes are created");

        let mut expected = Vec::new();
        for volume in [target(), node_modules()] {
            expected.push(strings(&["volume", "inspect", &volume]));
            expected.push(strings(&["volume", "create", &volume]));
            expected.push(volume_init_argv(IMAGE, &volume, UID, GID));
        }
        assert_eq!(calls.into_inner(), expected);
    }

    #[test]
    fn a_failed_volume_create_refuses_the_launch() {
        let calls = VolumeCalls::default();

        let error = ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "volume" { 1 } else { 0 })
        }))
        .expect_err("failed create refuses");

        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
        assert!(error.contains("create"), "{error}");
        assert_eq!(calls.into_inner().len(), 2);
    }

    #[test]
    fn a_failed_volume_chown_refuses_the_launch() {
        let calls = VolumeCalls::default();

        let error = ensure(volume_recorder(&calls, |args| {
            output(if args[0] == "run" || args[1] == "inspect" {
                1
            } else {
                0
            })
        }))
        .expect_err("failed chown refuses");

        assert!(error.contains("AJAX_SANDBOX=off"), "{error}");
        assert!(error.contains("chown"), "{error}");
        assert_eq!(calls.into_inner().len(), 3);
    }
}
