use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    io::{Read, Write},
    os::fd::AsFd,
    os::unix::fs::PermissionsExt,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use nix::{
    poll::{poll, PollFd, PollFlags, PollTimeout},
    pty::{forkpty, ForkptyResult, Winsize},
    sys::wait::{waitpid, WaitPidFlag, WaitStatus},
};

static NEXT_SANDBOX_ID: AtomicUsize = AtomicUsize::new(0);

fn ajax_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ajax-cli"))
}

struct SmokeSandbox {
    root: PathBuf,
    config_file: PathBuf,
    state_file: PathBuf,
    fake_bin: PathBuf,
    command_log: PathBuf,
    substrate_dir: PathBuf,
}

impl SmokeSandbox {
    fn new(test_name: &str) -> Self {
        let id = NEXT_SANDBOX_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test clock should be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ajax-smoke-{test_name}-{}-{id}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("failed to create {}: {error}", root.display()));

        let sandbox = Self {
            config_file: root.join("config.toml"),
            state_file: root.join("state").join("ajax.db"),
            fake_bin: root.join("fake-bin"),
            command_log: root.join("commands.log"),
            substrate_dir: root.join("substrate"),
            root,
        };
        fs::create_dir_all(
            sandbox
                .state_file
                .parent()
                .expect("state should have parent"),
        )
        .unwrap_or_else(|error| panic!("failed to create state directory: {error}"));
        fs::create_dir_all(&sandbox.substrate_dir)
            .unwrap_or_else(|error| panic!("failed to create substrate directory: {error}"));
        fs::write(&sandbox.command_log, "")
            .unwrap_or_else(|error| panic!("failed to create command log: {error}"));
        sandbox.install_fake_tools();
        sandbox
    }

    fn create_repo(&self, name: &str) -> PathBuf {
        let repo = self.root.join("repos").join(name);
        fs::create_dir_all(&repo)
            .unwrap_or_else(|error| panic!("failed to create repo {}: {error}", repo.display()));
        repo
    }

    fn write_config(&self, repos: &[&str]) {
        let mut config = String::new();
        for repo in repos {
            let repo_path = self.root.join("repos").join(repo);
            config.push_str(&format!(
                r#"
[[repos]]
name = "{repo}"
path = "{}"
default_branch = "main"

"#,
                repo_path.display()
            ));
        }
        config.push_str(
            r#"
[[test_commands]]
repo = "web"
command = 'printf checked >> "$AJAX_SMOKE_COMMAND_LOG"'

[[test_commands]]
repo = "api"
command = 'printf checked-api >> "$AJAX_SMOKE_COMMAND_LOG"'
"#,
        );
        fs::write(&self.config_file, config)
            .unwrap_or_else(|error| panic!("failed to write config: {error}"));
    }

    fn ajax<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.ajax_with_env(args, Vec::<(&str, &str)>::new())
    }

    fn ajax_with_env<I, S, E, K, V>(&self, args: I, extra_env: E) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let mut command = self.ajax_command(args, extra_env);
        command
            .output()
            .unwrap_or_else(|error| panic!("failed to run ajax: {error}"))
    }

    fn ajax_command<I, S, E, K, V>(&self, args: I, extra_env: E) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        E: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        let path = format!(
            "{}:{}",
            self.fake_bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(ajax_binary());
        command
            .args(args)
            .env_remove("AJAX_PROFILE")
            .env_remove("AJAX_HOME")
            .env_remove("AJAX_WORKTREE_ROOT")
            .env("HOME", &self.root)
            .env("AJAX_PROFILE", "dev")
            .env("AJAX_CONFIG", &self.config_file)
            .env("AJAX_STATE", &self.state_file)
            .env("AJAX_PROFILE", "dev")
            .env("AJAX_HOME", &self.root)
            .env_remove("AJAX_WORKTREE_ROOT")
            .env("AJAX_SMOKE_COMMAND_LOG", &self.command_log)
            .env("AJAX_SMOKE_SUBSTRATE_DIR", &self.substrate_dir)
            .env("PATH", path);
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command
    }

    fn repo_path(&self, name: &str) -> PathBuf {
        self.root.join("repos").join(name)
    }

    fn expected_worktree_path(&self, repo_name: &str, handle: &str) -> PathBuf {
        let repo_path = self.repo_path(repo_name);
        self.root
            .join("worktrees")
            .join(rooted_repo_dir(repo_name, &repo_path))
            .join(handle)
    }

    fn install_fake_tools(&self) {
        fs::create_dir_all(&self.fake_bin)
            .unwrap_or_else(|error| panic!("failed to create fake bin: {error}"));
        self.write_executable("git", FAKE_GIT);
        self.write_executable("tmux", FAKE_TMUX);
        self.write_executable("codex", FAKE_CODEX);
        self.write_executable("codex-acp", FAKE_CODEX);
        self.write_executable("claude-agent-acp", FAKE_CODEX);
        self.write_executable("pi-acp", FAKE_CODEX);
    }

    fn write_executable(&self, name: &str, contents: &str) {
        let path = self.fake_bin.join(name);
        fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
        let mut permissions = fs::metadata(&path)
            .unwrap_or_else(|error| panic!("failed to stat {}: {error}", path.display()))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .unwrap_or_else(|error| panic!("failed to chmod {}: {error}", path.display()));
    }

    fn command_log(&self) -> String {
        fs::read_to_string(&self.command_log)
            .unwrap_or_else(|error| panic!("failed to read command log: {error}"))
    }

    fn tmux_session_path(&self, session: &str) -> PathBuf {
        self.substrate_dir.join("sessions").join(session)
    }
}

impl Drop for SmokeSandbox {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                panic!("failed to remove {}: {error}", self.root.display());
            }
        }
    }
}

struct PtyAjaxOutput {
    stdout: String,
    status: WaitStatus,
}

fn run_ajax_cockpit_ctrl_q_flow(sandbox: &SmokeSandbox) -> PtyAjaxOutput {
    run_ajax_cockpit_ctrl_q_flow_with_env(sandbox, &[])
}

fn run_ajax_cockpit_ctrl_q_flow_with_env(
    sandbox: &SmokeSandbox,
    extra_env: &[(&str, &str)],
) -> PtyAjaxOutput {
    let winsize = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let fork = unsafe { forkpty(Some(&winsize), None) }
        .unwrap_or_else(|error| panic!("failed to fork cockpit PTY: {error}"));

    match fork {
        ForkptyResult::Child => {
            let mut env = vec![("AJAX_SMOKE_TMUX_ATTACH_BLOCK", "1"), ("TERM", "xterm")];
            env.extend_from_slice(extra_env);
            let error = sandbox
                .ajax_command(["cockpit", "--interval-ms", "10000"], env)
                .exec();
            eprintln!("failed to exec ajax cockpit: {error}");
            std::process::exit(127);
        }
        ForkptyResult::Parent { child, master } => {
            let mut master = fs::File::from(master);
            let mut stdout = Vec::new();

            wait_for_pty_output(
                &mut master,
                &mut stdout,
                "web/fix-login",
                "cockpit task list",
            );
            master
                .write_all(b"\x1b[B\r")
                .expect("failed to select resume action from cockpit PTY");
            wait_for_pty_resume_confirmation(
                &mut master,
                &mut stdout,
                "resume confirmation prompt",
            );
            master
                .write_all(b"\r")
                .expect("failed to confirm resume action from cockpit PTY");
            wait_for_pty_output(
                &mut master,
                &mut stdout,
                "attached ajax-web-fix-login",
                "task attach output",
            );
            let attach_output_len = stdout.len();
            master.write_all(b"\x11").expect("failed to send Ctrl-Q");
            wait_for_pty_output_after(
                &mut master,
                &mut stdout,
                attach_output_len,
                "Ajax",
                "cockpit redraw after Ctrl-Q",
            );
            master
                .write_all(b"q")
                .expect("failed to quit cockpit after Ctrl-Q");

            let status = wait_for_child_exit(child, &mut master, &mut stdout);
            PtyAjaxOutput {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                status,
            }
        }
    }
}

fn wait_for_pty_output(master: &mut fs::File, stdout: &mut Vec<u8>, expected: &str, context: &str) {
    wait_until_pty(context, master, stdout, |stdout| {
        String::from_utf8_lossy(stdout).contains(expected)
    });
}

fn wait_for_pty_resume_confirmation(master: &mut fs::File, stdout: &mut Vec<u8>, context: &str) {
    wait_until_pty(context, master, stdout, |stdout| {
        strip_ansi_escapes(&String::from_utf8_lossy(stdout)).contains(">> resume")
    });
}

fn strip_ansi_escapes(text: &str) -> String {
    let mut stripped = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(next) = chars.next() {
                if next.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        stripped.push(ch);
    }
    stripped
}

fn wait_for_pty_output_after(
    master: &mut fs::File,
    stdout: &mut Vec<u8>,
    after: usize,
    expected: &str,
    context: &str,
) {
    wait_until_pty(context, master, stdout, |stdout| {
        String::from_utf8_lossy(stdout.get(after..).unwrap_or_default()).contains(expected)
    });
}

fn wait_until_pty(
    context: &str,
    master: &mut fs::File,
    stdout: &mut Vec<u8>,
    mut done: impl FnMut(&[u8]) -> bool,
) {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        read_pty_available(master, stdout);
        if done(stdout) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "timed out waiting for {context}\nstdout:\n{}",
        String::from_utf8_lossy(stdout)
    );
}

fn wait_for_child_exit(
    child: nix::unistd::Pid,
    master: &mut fs::File,
    stdout: &mut Vec<u8>,
) -> WaitStatus {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(5) {
        read_pty_available(master, stdout);
        match waitpid(child, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {}
            Ok(status) => return status,
            Err(error) => panic!("failed to wait for ajax cockpit: {error}"),
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "timed out waiting for ajax cockpit to exit\nstdout:\n{}",
        String::from_utf8_lossy(stdout)
    );
}

fn read_pty_available(master: &mut fs::File, stdout: &mut Vec<u8>) {
    loop {
        let mut poll_fds = [PollFd::new(master.as_fd(), PollFlags::POLLIN)];
        poll(&mut poll_fds, PollTimeout::ZERO).expect("failed to poll cockpit PTY");
        if !poll_fds[0]
            .revents()
            .unwrap_or_else(PollFlags::empty)
            .contains(PollFlags::POLLIN)
        {
            return;
        }
        let mut buf = [0_u8; 8192];
        match master.read(&mut buf) {
            Ok(0) => return,
            Ok(count) => stdout.extend_from_slice(&buf[..count]),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(error) => panic!("failed to read cockpit PTY: {error}"),
        }
    }
}

fn rooted_repo_dir(repo_name: &str, repo_path: &Path) -> String {
    let slug = repo_name.to_ascii_lowercase();
    format!("{slug}-{:08x}", short_path_hash(repo_path))
}

fn short_path_hash(path: &Path) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

const FAKE_GIT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"

slug_from_path() {
  local base
  base="$(basename "$1")"
  printf '%s' "${base#ajax-}"
}

case "$*" in
  *" fetch origin "*)
    exit 0
    ;;
  *" show-ref --verify --quiet refs/heads/"*)
    ref="${@: -1}"
    branch="${ref#refs/heads/}"
    created="$AJAX_SMOKE_SUBSTRATE_DIR/created-branches"
    deleted="$AJAX_SMOKE_SUBSTRATE_DIR/deleted-branches"
    if [[ -f "$created" ]] && grep -qx "$branch" "$created"; then
      if [[ -f "$deleted" ]] && grep -qx "$branch" "$deleted"; then
        exit 1
      fi
      exit 0
    fi
    exit 1
    ;;
  *" show-ref --verify --quiet "*)
    # Start planning probes whether ajax/<handle> already exists; absent ⇒ exit 1.
    exit 1
    ;;
  *" worktree add "*)
    worktree="${7:-}"
    branch="${6:-}"
    if [[ "$branch" == ajax/* ]]; then
      printf '%s\n' "$branch" >> "$AJAX_SMOKE_SUBSTRATE_DIR/created-branches"
    fi
    mkdir -p "$worktree"
    printf 'worktree\n' > "$worktree/.ajax-smoke-worktree"
    ;;
  *" worktree remove "*)
    target="${@: -1}"
    rm -rf "$target"
    ;;
  *" branch -d ajax/"*|*" branch -D ajax/"*)
    branch="${@: -1}"
    printf '%s\n' "$branch" >> "$AJAX_SMOKE_SUBSTRATE_DIR/deleted-branches"
    exit 0
    ;;
  *" push origin --delete ajax/"*)
    branch="${@: -1}"
    printf '%s\n' "$branch" >> "$AJAX_SMOKE_SUBSTRATE_DIR/deleted-branches"
    exit 0
    ;;
  *" switch main")
    exit 0
    ;;
  *" merge --ff-only ajax/"*)
    touch "$AJAX_SMOKE_SUBSTRATE_DIR/merged"
    ;;
  *" worktree list --porcelain"*)
    repo="${2:-}"
    repo_slug="$(basename "$repo")"
    printf 'worktree %s\nHEAD 1111111\nbranch refs/heads/main\n\n' "$repo"
    worktrees_root="$(dirname "$repo")/$(basename "$repo")__worktrees"
    if [[ -d "$worktrees_root" ]]; then
      for dir in "$worktrees_root"/ajax-*; do
        [[ -d "$dir" ]] || continue
        branch_suffix="${dir##*/ajax-}"
        printf 'worktree %s\nHEAD 2222222\nbranch refs/heads/ajax/%s\n\n' "$dir" "$branch_suffix"
      done
    fi
    if [[ -d "$HOME/worktrees" ]]; then
      for repo_dir in "$HOME/worktrees"/"$repo_slug"-*; do
        [[ -d "$repo_dir" ]] || continue
        for dir in "$repo_dir"/*; do
          [[ -d "$dir" ]] || continue
          handle="$(basename "$dir")"
          printf 'worktree %s\nHEAD 2222222\nbranch refs/heads/ajax/%s\n\n' "$dir" "$handle"
        done
      done
    fi
    ;;
  *" branch --format=%(refname:short)"*|*" branch -r --format=%(refname:short)"*)
    remote=0
    if [[ "$*" == *" branch -r "* ]]; then
      remote=1
    fi
    repo="${2:-}"
    repo_slug="$(basename "$repo")"
    deleted="$AJAX_SMOKE_SUBSTRATE_DIR/deleted-branches"
    branch_is_deleted() {
      local name="$1"
      [[ -f "$deleted" ]] && grep -qx "$name" "$deleted"
    }
    emit_branch() {
      local name="$1"
      branch_is_deleted "$name" && return 0
      if (( remote == 1 )); then
        printf 'origin/%s\n' "$name"
      else
        printf '%s\n' "$name"
      fi
    }
    emit_branch "main"
    worktrees_root="$(dirname "$repo")/$(basename "$repo")__worktrees"
    if [[ -d "$worktrees_root" ]]; then
      for dir in "$worktrees_root"/ajax-*; do
        [[ -d "$dir" ]] || continue
        branch_suffix="${dir##*/ajax-}"
        emit_branch "ajax/$branch_suffix"
      done
    fi
    if [[ -d "$HOME/worktrees" ]]; then
      for repo_dir in "$HOME/worktrees"/"$repo_slug"-*; do
        [[ -d "$repo_dir" ]] || continue
        for dir in "$repo_dir"/*; do
          [[ -d "$dir" ]] || continue
          emit_branch "ajax/$(basename "$dir")"
        done
      done
    fi
    ;;
  *" status --porcelain=v1 --branch"*)
    cwd="${2:-}"
    if [[ ! -d "$cwd" ]]; then
      echo "fatal: not a git repository: $cwd" >&2
      exit 128
    fi
    slug="$(slug_from_path "$cwd")"
    printf '## ajax/%s\n' "$slug"
    ;;
  *" merge-base --is-ancestor "*)
    if [[ -f "$AJAX_SMOKE_SUBSTRATE_DIR/merged" ]]; then
      exit 0
    fi
    exit 1
    ;;
  "diff --stat "*)
    printf ' smoke.rs | 1 +\n'
    ;;
  *)
    echo "unexpected git command: $*" >&2
    exit 2
    ;;
esac
"#;

const FAKE_TMUX: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'tmux %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"
sessions="$AJAX_SMOKE_SUBSTRATE_DIR/sessions"
mkdir -p "$sessions"

case "${1:-}" in
  new-session)
    if [[ -n "${AJAX_SMOKE_FAIL_AFTER_WORKTREE:-}" ]]; then
      echo "simulated tmux startup failure" >&2
      exit 42
    fi
    session="${4:-}"
    worktree="${8:-}"
    printf '%s\n' "$worktree" > "$sessions/$session"
    ;;
  new-window)
    session="${3:-}"
    worktree="${7:-}"
    printf '%s\n' "$worktree" > "$sessions/$session"
    ;;
  kill-window)
    session_window="${3:-}"
    session="${session_window%%:*}"
    rm -f "$sessions/$session"
    ;;
  kill-session)
    session="${3:-}"
    rm -f "$sessions/$session"
    ;;
  attach-session)
    session="${3:-}"
    if [[ -n "${AJAX_SMOKE_TMUX_ATTACH_EINTR_ONCE:-}" ]]; then
      interrupted_marker="$AJAX_SMOKE_SUBSTRATE_DIR/attach-eintr-once"
      if [[ ! -f "$interrupted_marker" ]]; then
        touch "$interrupted_marker"
        printf 'tmux: EINTR service interrupted call\n'
        exit 1
      fi
    fi
    if [[ -n "${AJAX_SMOKE_TMUX_ATTACH_BLOCK:-}" ]]; then
      printf 'attached %s\n' "$session"
      trap 'exit 0' HUP TERM INT
      while true; do
        sleep 1
      done
    fi
    exit 0
    ;;
  switch-client|select-window|send-keys)
    exit 0
    ;;
  list-sessions)
    for file in "$sessions"/*; do
      [[ -e "$file" ]] || exit 0
      basename "$file"
    done
    ;;
  list-windows)
    session="${3:-}"
    if [[ -f "$sessions/$session" ]]; then
      printf 'task\t%s\n' "$(cat "$sessions/$session")"
    fi
    ;;
  capture-pane)
    printf 'idle\n'
    ;;
  *)
    echo "unexpected tmux command: $*" >&2
    exit 2
    ;;
esac
"#;

const FAKE_CODEX: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'codex %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"
printf '{"type":"started"}\n'
printf '{"type":"completed"}\n'
"#;

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout should be UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr should be UTF-8")
}

fn assert_success(output: &Output, command: &str) {
    assert!(
        output.status.success(),
        "{command} should succeed\nstdout:\n{}\nstderr:\n{}",
        stdout(output),
        stderr(output)
    );
}

fn assert_json(output: &Output, command: &str) -> Value {
    assert_success(output, command);
    assert_eq!(stderr(output), "", "{command} should not write stderr");
    serde_json::from_str(&stdout(output))
        .unwrap_or_else(|error| panic!("{command} should emit JSON: {error}\n{}", stdout(output)))
}

fn repo_path(root: &Path, name: &str) -> PathBuf {
    root.join("repos").join(name)
}

fn create_active_web_task(sandbox: &SmokeSandbox) {
    create_task(sandbox, "web", "fix login");
}

fn create_task(sandbox: &SmokeSandbox, repo: &str, title: &str) {
    let output = sandbox.ajax([
        "start",
        "--repo",
        repo,
        "--title",
        title,
        "--agent",
        "codex",
        "--execute",
    ]);
    assert_success(&output, "ajax start --execute");
}

fn create_failing_task(sandbox: &SmokeSandbox, repo: &str, title: &str) {
    let output = sandbox.ajax_with_env(
        [
            "start",
            "--repo",
            repo,
            "--title",
            title,
            "--agent",
            "codex",
            "--execute",
        ],
        [("AJAX_SMOKE_FAIL_AFTER_WORKTREE", "1")],
    );
    assert!(
        !output.status.success(),
        "ajax start should fail for simulated partial creation"
    );
}

fn supervise_task(sandbox: &SmokeSandbox, task: &str) {
    let output = sandbox.ajax([
        "supervise",
        "--task",
        task,
        "--prompt",
        "finish task",
        "--json",
    ]);
    assert_success(&output, "ajax supervise --task --json");
}

fn complete_web_task_to_reviewable(sandbox: &SmokeSandbox) {
    create_active_web_task(sandbox);
    supervise_task(sandbox, "web/fix-login");
}

fn assert_cockpit_matches_tasks(sandbox: &SmokeSandbox, expected_lifecycle: Option<&str>) {
    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    let cockpit = assert_json(&sandbox.ajax(["cockpit", "--json"]), "ajax cockpit --json");
    let task_count = tasks["tasks"]
        .as_array()
        .expect("tasks should be an array")
        .len();
    assert_eq!(cockpit["summary"]["tasks"], task_count);
    assert_eq!(
        cockpit["tasks"]["tasks"]
            .as_array()
            .expect("cockpit tasks should be an array")
            .len(),
        task_count
    );
    if let Some(lifecycle) = expected_lifecycle {
        assert_eq!(tasks["tasks"][0]["lifecycle_status"], lifecycle);
        assert_eq!(cockpit["tasks"]["tasks"][0]["lifecycle_status"], lifecycle);
        assert_eq!(
            cockpit["tasks"]["tasks"][0]["needs_attention"],
            tasks["tasks"][0]["needs_attention"]
        );
        assert_eq!(
            cockpit["next"]["item"]["task_handle"],
            if tasks["tasks"][0]["needs_attention"] == true {
                Value::String("web/fix-login".to_string())
            } else {
                Value::Null
            }
        );
    }
}

include!("suite_1.rs");
include!("suite_2.rs");
