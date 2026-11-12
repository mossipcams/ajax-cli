use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Output;

pub const DOCKERFILE: &str = include_str!("agent.Dockerfile");

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;
const CODEX_SANDBOX_OVERRIDE: [&str; 2] = ["-c", "sandbox_mode=\"danger-full-access\""];
const SSH_AGENT_SOCK: &str = ".config/ajax/ssh/agent.sock";
const READ_ONLY_TOOLING: [&str; 15] = [
    "Projects/ajax-model-router",
    ".claude/skills",
    ".codex/skills",
    ".agents/skills",
    ".cursor/skills",
    ".claude/skills-src",
    "Projects/AGENTS.md",
    "Projects/agents.md",
    "Projects/CLAUDE.md",
    "Projects/.claude",
    "Projects/.codex",
    "Projects/scripts",
    ".claude/CLAUDE.md",
    ".claude/RTK.md",
    ".codex/AGENTS.md",
];
const READ_WRITE_TOOLING: [&str; 6] = [
    ".claude/projects",
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

pub struct DockerLaunch<'a> {
    pub home: &'a Path,
    pub uid: u32,
    pub gid: u32,
    pub cwd: &'a Path,
    pub git_common_dir: Option<&'a Path>,
    pub extra_read_write: &'a [PathBuf],
    pub boot_id: &'a str,
    pub tty: bool,
    pub pass_gh_token: bool,
    pub image: &'a str,
    pub program: &'a str,
    pub args: &'a [String],
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
    argv.push(format!("ajax-{key}"));
    argv.extend(pair("--label", format!("ajax.boot={}", launch.boot_id)));
    argv.extend(pair("--label", format!("ajax.worktree={cwd}")));
    argv.extend(pair("-e", format!("HOME={home}")));
    if exists(&sock) {
        argv.extend(pair("-e", format!("SSH_AUTH_SOCK={}", text(&sock))));
    }
    if launch.pass_gh_token {
        argv.extend(pair("-e", "GH_TOKEN"));
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
    argv.extend(pair("-v", format!("ajax-target-{key}:{cwd}/target")));
    argv.extend(pair(
        "-v",
        format!("ajax-node-modules-{key}:{cwd}/node_modules"),
    ));
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
    use std::fs;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    const HOME: &str = "/Users/matt";
    const CWD: &str = "/Users/matt/Projects/ajax-cli__worktrees/ajax-docker-sandboxes";
    const COMMON: &str = "/Users/matt/Projects/ajax-cli/.git";
    const KEY: &str = "ajax-docker-sandboxes-1576734c";
    const IMAGE: &str = "ajax-agent:test";
    const SOCK: &str = "/Users/matt/.config/ajax/ssh/agent.sock";

    fn sample<'a>(home: &'a Path, cwd: &'a Path, args: &'a [String]) -> DockerLaunch<'a> {
        DockerLaunch {
            home,
            uid: 501,
            gid: 20,
            cwd,
            git_common_dir: Some(Path::new(COMMON)),
            extra_read_write: &[],
            boot_id: "boot-1",
            tty: false,
            pass_gh_token: false,
            image: IMAGE,
            program: "claude",
            args,
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
            "run --rm -i --init --user 501:20 --network host --name ajax-{KEY} \
             --label ajax.boot=boot-1 --label ajax.worktree={CWD} -e HOME={HOME} \
             -e SSH_AUTH_SOCK={SOCK} -w {CWD} -v ajax-agent-home:{HOME} \
             -v ajax-rustup:/opt/rustup -v ajax-cargo-registry:/opt/cargo/registry \
             -v ajax-cargo-git:/opt/cargo/git -v {CWD}:{CWD} -v {COMMON}:{COMMON} \
             -v ajax-target-{KEY}:{CWD}/target -v ajax-node-modules-{KEY}:{CWD}/node_modules \
             -v {HOME}/Projects/ajax-model-router:{HOME}/Projects/ajax-model-router:ro \
             -v {HOME}/.claude/skills:{HOME}/.claude/skills:ro \
             -v {HOME}/.codex/skills:{HOME}/.codex/skills:ro \
             -v {HOME}/.agents/skills:{HOME}/.agents/skills:ro \
             -v {HOME}/.cursor/skills:{HOME}/.cursor/skills:ro \
             -v {HOME}/.claude/skills-src:{HOME}/.claude/skills-src:ro \
             -v {HOME}/Projects/AGENTS.md:{HOME}/Projects/AGENTS.md:ro \
             -v {HOME}/Projects/agents.md:{HOME}/Projects/agents.md:ro \
             -v {HOME}/Projects/CLAUDE.md:{HOME}/Projects/CLAUDE.md:ro \
             -v {HOME}/Projects/.claude:{HOME}/Projects/.claude:ro \
             -v {HOME}/Projects/.codex:{HOME}/Projects/.codex:ro \
             -v {HOME}/Projects/scripts:{HOME}/Projects/scripts:ro \
             -v {HOME}/.claude/CLAUDE.md:{HOME}/.claude/CLAUDE.md:ro \
             -v {HOME}/.claude/RTK.md:{HOME}/.claude/RTK.md:ro \
             -v {HOME}/.codex/AGENTS.md:{HOME}/.codex/AGENTS.md:ro \
             -v {HOME}/.claude/projects:{HOME}/.claude/projects \
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
            "run --rm -i -t --init --user 501:20 --network host --name ajax-{KEY} \
             --label ajax.boot=boot-1 --label ajax.worktree={CWD} -e HOME={HOME} -w {CWD} \
             -v ajax-agent-home:{HOME} -v ajax-rustup:/opt/rustup -v ajax-cargo-registry:/opt/cargo/registry \
             -v ajax-cargo-git:/opt/cargo/git -v {CWD}:{CWD} -v /var/folders/ajax/events:/var/folders/ajax/events \
             -v ajax-target-{KEY}:{CWD}/target -v ajax-node-modules-{KEY}:{CWD}/node_modules \
             {IMAGE} claude --print"
        ));
        assert_eq!(argv, expected);
    }

    #[test]
    fn missing_tooling_paths_are_skipped() {
        let home = Path::new(HOME);
        let cwd = Path::new(CWD);
        let args = strings(&[]);
        let argv = docker_argv(&sample(home, cwd, &args), |_| false);
        assert!(!argv.iter().any(|a| a.starts_with("SSH_AUTH_SOCK")));
        assert!(!argv.iter().any(|a| a.contains("/.codex/skills")));

        let codex_skills = PathBuf::from(format!("{HOME}/.codex/skills"));
        let argv = docker_argv(&sample(home, cwd, &args), |p| p == codex_skills);
        assert!(has_pair(
            &argv,
            "-v",
            &format!("{HOME}/.codex/skills:{HOME}/.codex/skills:ro")
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
}
