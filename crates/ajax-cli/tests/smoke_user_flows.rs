use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_SANDBOX_ID: AtomicUsize = AtomicUsize::new(0);

fn ajax_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ajax"))
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
        let path = format!(
            "{}:{}",
            self.fake_bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut command = Command::new(ajax_binary());
        command
            .args(args)
            .env("HOME", &self.root)
            .env("AJAX_CONFIG", &self.config_file)
            .env("AJAX_STATE", &self.state_file)
            .env("AJAX_SMOKE_COMMAND_LOG", &self.command_log)
            .env("AJAX_SMOKE_SUBSTRATE_DIR", &self.substrate_dir)
            .env("PATH", path);
        for (key, value) in extra_env {
            command.env(key, value);
        }
        command
            .output()
            .unwrap_or_else(|error| panic!("failed to run ajax: {error}"))
    }

    fn install_fake_tools(&self) {
        fs::create_dir_all(&self.fake_bin)
            .unwrap_or_else(|error| panic!("failed to create fake bin: {error}"));
        self.write_executable("git", FAKE_GIT);
        self.write_executable("tmux", FAKE_TMUX);
        self.write_executable("codex", FAKE_CODEX);
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

const FAKE_GIT: &str = r#"#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >> "$AJAX_SMOKE_COMMAND_LOG"

slug_from_path() {
  local base
  base="$(basename "$1")"
  printf '%s' "${base#ajax-}"
}

case "$*" in
  *" worktree add "*)
    worktree="${7:-}"
    mkdir -p "$worktree"
    printf 'worktree\n' > "$worktree/.ajax-smoke-worktree"
    ;;
  *" worktree remove "*)
    target="${@: -1}"
    rm -rf "$target"
    ;;
  *" branch -d ajax/"*|*" branch -D ajax/"*)
    exit 0
    ;;
  *" switch main")
    exit 0
    ;;
  *" merge --ff-only ajax/"*)
    touch "$AJAX_SMOKE_SUBSTRATE_DIR/merged"
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
  attach-session|switch-client|select-window|send-keys)
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
      printf 'worktrunk\t%s\n' "$(cat "$sessions/$session")"
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
        "new",
        "--repo",
        repo,
        "--title",
        title,
        "--agent",
        "codex",
        "--execute",
    ]);
    assert_success(&output, "ajax new --execute");
}

fn create_failing_task(sandbox: &SmokeSandbox, repo: &str, title: &str) {
    let output = sandbox.ajax_with_env(
        [
            "new",
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
        "ajax new should fail for simulated partial creation"
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

#[test]
fn smoke_first_run_health_check() {
    let sandbox = SmokeSandbox::new("first-run-health-check");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    let doctor = assert_json(&sandbox.ajax(["doctor", "--json"]), "ajax doctor --json");
    assert!(doctor["checks"]
        .as_array()
        .expect("doctor checks should be an array")
        .iter()
        .all(|check| check["ok"].as_bool() == Some(true)));
    assert!(doctor["checks"]
        .as_array()
        .expect("doctor checks should be an array")
        .iter()
        .any(|check| check["name"] == "state:path" && check["ok"].as_bool() == Some(true)));

    let repos = assert_json(&sandbox.ajax(["repos", "--json"]), "ajax repos --json");
    assert_eq!(repos["repos"][0]["name"], "web");
    assert_eq!(
        repos["repos"][0]["path"],
        repo_path(&sandbox.root, "web").display().to_string()
    );

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));

    let status = assert_json(&sandbox.ajax(["status", "--json"]), "ajax status --json");
    assert_eq!(status["tasks"], Value::Array(vec![]));
}

#[test]
fn smoke_new_plan_has_no_side_effects() {
    let sandbox = SmokeSandbox::new("new-plan-no-side-effects");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    let plan = assert_json(
        &sandbox.ajax([
            "new",
            "--repo",
            "web",
            "--title",
            "fix login",
            "--agent",
            "codex",
            "--json",
        ]),
        "ajax new --json",
    );

    assert_eq!(plan["title"], "create task: fix login");
    assert_eq!(plan["requires_confirmation"], false);
    assert_eq!(plan["blocked_reasons"], Value::Array(vec![]));
    assert!(plan["commands"]
        .as_array()
        .expect("plan commands should be an array")
        .iter()
        .any(|command| command["program"] == "git"
            && command["args"]
                .as_array()
                .is_some_and(|args| args.iter().any(|arg| arg == "worktree"))));

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));
    assert_eq!(
        sandbox.command_log(),
        "",
        "plan-only new should not run fake lifecycle tools"
    );
    assert!(
        !sandbox.state_file.exists(),
        "plan-only new should not create durable state"
    );
}

#[test]
fn smoke_new_execute_creates_active_task_environment() {
    let sandbox = SmokeSandbox::new("new-execute-active");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = repo
        .parent()
        .expect("repo should have parent")
        .join("web__worktrees/ajax-fix-login");

    create_active_web_task(&sandbox);

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Active");
    assert_eq!(tasks["tasks"][0]["needs_attention"], false);

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["qualified_handle"], "web/fix-login");
    assert_eq!(inspect["task"]["lifecycle_status"], "Active");
    assert_eq!(inspect["branch"], "ajax/fix-login");
    assert_eq!(inspect["tmux_session"], "ajax-web-fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());
    assert!(inspect["worktree_path"]
        .as_str()
        .expect("worktree path should be a string")
        .contains("web__worktrees/ajax-fix-login"));

    let log = sandbox.command_log();
    assert!(
        log.contains(&format!(
            "git -C {} worktree add -b ajax/fix-login {} main",
            repo.display(),
            worktree.display()
        )),
        "fake git log should include worktree add:\n{log}"
    );
    assert!(
        log.contains(&format!(
            "tmux new-session -d -s ajax-web-fix-login -n worktrunk -c {}",
            worktree.display()
        )),
        "fake tmux log should include session creation:\n{log}"
    );
    assert!(
        log.contains(&format!(
            "tmux send-keys -t ajax-web-fix-login:worktrunk codex --cd {} 'fix login' Enter",
            worktree.display()
        )),
        "fake tmux log should include agent launch:\n{log}"
    );
}

#[test]
fn smoke_open_and_trunk_are_idempotent_repairs() {
    let sandbox = SmokeSandbox::new("open-trunk-idempotent");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = repo
        .parent()
        .expect("repo should have parent")
        .join("web__worktrees/ajax-fix-login");
    create_active_web_task(&sandbox);

    for command in [
        ["open", "web/fix-login", "--execute"],
        ["trunk", "web/fix-login", "--execute"],
        ["open", "web/fix-login", "--execute"],
        ["trunk", "web/fix-login", "--execute"],
    ] {
        let output = sandbox.ajax(command);
        assert_success(&output, &format!("ajax {}", command.join(" ")));
    }

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["tmux_session"], "ajax-web-fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());

    let log = sandbox.command_log();
    assert!(
        log.matches("tmux select-window -t ajax-web-fix-login:worktrunk")
            .count()
            >= 3,
        "open should select the worktrunk window each time:\n{log}"
    );
    assert!(
        log.matches("tmux select-window -t ajax-web-fix-login:worktrunk")
            .count()
            >= 5,
        "open and trunk should idempotently target the worktrunk window:\n{log}"
    );
    assert!(
        log.contains("tmux attach-session -t ajax-web-fix-login")
            || log.contains("tmux switch-client -t ajax-web-fix-login"),
        "open should attach or switch to the expected session:\n{log}"
    );
}

#[test]
fn smoke_supervise_completion_makes_task_reviewable() {
    let sandbox = SmokeSandbox::new("supervise-reviewable");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);

    let supervise = sandbox.ajax([
        "supervise",
        "--task",
        "web/fix-login",
        "--prompt",
        "finish task",
        "--json",
    ]);
    assert_success(&supervise, "ajax supervise --task --json");
    assert_eq!(stderr(&supervise), "");
    let events = stdout(&supervise)
        .lines()
        .map(|line| {
            serde_json::from_str::<Value>(line)
                .unwrap_or_else(|error| panic!("supervise event should be JSON: {error}: {line}"))
        })
        .collect::<Vec<_>>();
    assert!(events
        .iter()
        .any(|event| event["Agent"]["Started"]["agent"] == "codex"));
    assert!(events
        .iter()
        .any(|event| event["Agent"] == "Completed" || event["Agent"]["Completed"].is_object()));

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Reviewable");

    let review = assert_json(&sandbox.ajax(["review", "--json"]), "ajax review --json");
    assert_eq!(review["tasks"][0]["qualified_handle"], "web/fix-login");

    let next = assert_json(&sandbox.ajax(["next", "--json"]), "ajax next --json");
    assert_eq!(next["item"]["task_handle"], "web/fix-login");

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    assert!(inbox["items"]
        .as_array()
        .expect("inbox items should be an array")
        .iter()
        .any(|item| item["task_handle"] == "web/fix-login"));

    let log = sandbox.command_log();
    assert!(
        log.contains("codex"),
        "fake codex should be launched by supervise:\n{log}"
    );
}

#[test]
fn smoke_merge_and_clean_completed_task() {
    let sandbox = SmokeSandbox::new("merge-clean");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = repo
        .parent()
        .expect("repo should have parent")
        .join("web__worktrees/ajax-fix-login");
    complete_web_task_to_reviewable(&sandbox);

    let check = sandbox.ajax(["check", "web/fix-login", "--execute"]);
    assert_success(&check, "ajax check --execute");
    assert!(
        sandbox.command_log().contains("checked"),
        "check should run the configured test command"
    );

    let diff = sandbox.ajax(["diff", "web/fix-login", "--execute"]);
    assert_success(&diff, "ajax diff --execute");
    assert!(
        stdout(&diff).contains("smoke.rs | 1 +"),
        "diff should render fake git diff output:\n{}",
        stdout(&diff)
    );

    let merge_plan = assert_json(
        &sandbox.ajax(["merge", "web/fix-login", "--json"]),
        "ajax merge --json",
    );
    assert_eq!(merge_plan["title"], "merge task: web/fix-login");
    assert!(merge_plan["commands"]
        .as_array()
        .expect("merge plan commands should be an array")
        .iter()
        .any(|command| command["program"] == "git"));
    let log_before_merge = sandbox.command_log();

    let merge = sandbox.ajax(["merge", "web/fix-login", "--execute", "--yes"]);
    assert_success(&merge, "ajax merge --execute --yes");
    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Merged");

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["lifecycle_status"], "Merged");
    let log_after_merge = sandbox.command_log();
    assert_eq!(
        log_before_merge.matches("git -C").count() + 3,
        log_after_merge.matches("git -C").count(),
        "merge execution should add status, switch, and merge git calls only after execute"
    );
    assert!(log_after_merge.contains(&format!("git -C {} switch main", repo.display())));
    assert!(log_after_merge.contains(&format!(
        "git -C {} merge --ff-only ajax/fix-login",
        repo.display()
    )));

    let clean_plan = sandbox.ajax(["clean", "web/fix-login"]);
    assert_success(&clean_plan, "ajax clean plan");
    assert!(
        stdout(&clean_plan).contains("clean task: web/fix-login"),
        "clean should return a cleanup plan before execution"
    );
    let log_before_clean = sandbox.command_log();

    let clean = sandbox.ajax(["clean", "web/fix-login", "--execute", "--yes"]);
    assert_success(&clean, "ajax clean --execute --yes");
    let log_after_clean = sandbox.command_log();
    assert_ne!(
        log_before_clean, log_after_clean,
        "confirmed clean should run external cleanup commands"
    );
    assert!(
        log_after_clean.contains("tmux kill-session -t ajax-web-fix-login"),
        "clean should kill the task session:\n{log_after_clean}"
    );
    assert!(
        log_after_clean.contains(&format!(
            "git -C {} worktree remove {}",
            repo.display(),
            worktree.display()
        )),
        "clean should remove the worktree:\n{log_after_clean}"
    );
    assert!(
        log_after_clean.contains(&format!(
            "git -C {} branch -d ajax/fix-login",
            repo.display()
        )),
        "clean should delete the merged task branch:\n{log_after_clean}"
    );

    let tasks_after_clean = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks_after_clean["tasks"], Value::Array(vec![]));
    let cockpit = assert_json(&sandbox.ajax(["cockpit", "--json"]), "ajax cockpit --json");
    assert_eq!(cockpit["summary"]["active_tasks"], 0);
    assert_eq!(cockpit["tasks"]["tasks"], Value::Array(vec![]));
}

#[test]
fn smoke_partial_new_failure_remains_visible_and_recoverable() {
    let sandbox = SmokeSandbox::new("partial-new-failure");
    let repo = sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    let worktree = repo
        .parent()
        .expect("repo should have parent")
        .join("web__worktrees/ajax-fix-login");

    let failed = sandbox.ajax_with_env(
        [
            "new",
            "--repo",
            "web",
            "--title",
            "fix login",
            "--agent",
            "codex",
            "--execute",
        ],
        [("AJAX_SMOKE_FAIL_AFTER_WORKTREE", "1")],
    );
    assert!(
        !failed.status.success(),
        "ajax new should fail when tmux provisioning fails"
    );
    assert!(
        stderr(&failed).contains("simulated tmux startup failure"),
        "failure should preserve tmux stderr:\n{}",
        stderr(&failed)
    );

    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(tasks["tasks"][0]["lifecycle_status"], "Error");
    assert_eq!(tasks["tasks"][0]["needs_attention"], true);

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    assert!(inbox["items"]
        .as_array()
        .expect("inbox items should be an array")
        .iter()
        .any(|item| item["task_handle"] == "web/fix-login"));

    let inspect = assert_json(
        &sandbox.ajax(["inspect", "web/fix-login", "--json"]),
        "ajax inspect --json",
    );
    assert_eq!(inspect["task"]["lifecycle_status"], "Error");
    assert_eq!(inspect["branch"], "ajax/fix-login");
    assert_eq!(inspect["worktree_path"], worktree.display().to_string());

    let log = sandbox.command_log();
    assert!(log.contains(&format!(
        "git -C {} worktree add -b ajax/fix-login {} main",
        repo.display(),
        worktree.display()
    )));
    assert!(log.contains(&format!(
        "tmux new-session -d -s ajax-web-fix-login -n worktrunk -c {}",
        worktree.display()
    )));
    assert!(
        !log.contains("tmux send-keys -t ajax-web-fix-login:worktrunk"),
        "agent launch should not run after tmux session creation fails:\n{log}"
    );
}

#[test]
fn smoke_state_export_writes_json_and_refuses_overwrite() {
    let sandbox = SmokeSandbox::new("state-export");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    create_active_web_task(&sandbox);
    let backup = sandbox.root.join("ajax-state-backup.json");

    let export = sandbox.ajax([
        "state",
        "export",
        "--output",
        backup.to_str().expect("backup path should be UTF-8"),
    ]);
    assert_success(&export, "ajax state export");
    let exported =
        fs::read_to_string(&backup).expect("state export should create a readable backup file");
    assert!(
        !exported.is_empty(),
        "state export should create a non-empty JSON file"
    );
    let snapshot: Value =
        serde_json::from_str(&exported).expect("state export file should parse as JSON");
    assert_eq!(snapshot["repos"][0]["name"], "web");
    assert_eq!(snapshot["tasks"][0]["repo"], "web");
    assert_eq!(snapshot["tasks"][0]["handle"], "fix-login");
    assert_eq!(snapshot["metadata"]["repo_count"], 1);
    assert_eq!(snapshot["metadata"]["task_count"], 1);
    assert!(snapshot["metadata"]["event_count"]
        .as_u64()
        .is_some_and(|count| count > 0));

    let duplicate = sandbox.ajax([
        "state",
        "export",
        "--output",
        backup.to_str().expect("backup path should be UTF-8"),
    ]);
    assert!(
        !duplicate.status.success(),
        "duplicate state export should fail rather than overwrite"
    );
    assert!(
        stderr(&duplicate).contains("state export target already exists"),
        "duplicate export failure should explain the existing target:\n{}",
        stderr(&duplicate)
    );
}

#[test]
fn smoke_cockpit_tracks_cli_workflow() {
    let sandbox = SmokeSandbox::new("cockpit-parity");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);

    assert_cockpit_matches_tasks(&sandbox, None);

    create_active_web_task(&sandbox);
    assert_cockpit_matches_tasks(&sandbox, Some("Active"));

    let supervise = sandbox.ajax([
        "supervise",
        "--task",
        "web/fix-login",
        "--prompt",
        "finish task",
        "--json",
    ]);
    assert_success(&supervise, "ajax supervise --task --json");
    assert_cockpit_matches_tasks(&sandbox, Some("Reviewable"));

    let merge = sandbox.ajax(["merge", "web/fix-login", "--execute", "--yes"]);
    assert_success(&merge, "ajax merge --execute --yes");
    assert_cockpit_matches_tasks(&sandbox, Some("Merged"));

    let clean = sandbox.ajax(["clean", "web/fix-login", "--execute", "--yes"]);
    assert_success(&clean, "ajax clean --execute --yes");
    assert_cockpit_matches_tasks(&sandbox, None);
}

#[test]
fn smoke_multi_repo_attention_routing() {
    let sandbox = SmokeSandbox::new("multi-repo-attention");
    sandbox.create_repo("web");
    sandbox.create_repo("api");
    sandbox.write_config(&["web", "api"]);

    create_task(&sandbox, "web", "fix login");
    create_task(&sandbox, "api", "add search");
    supervise_task(&sandbox, "api/add-search");
    create_failing_task(&sandbox, "api", "break cache");

    let all_tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(all_tasks["tasks"].as_array().unwrap().len(), 3);

    let web_tasks = assert_json(
        &sandbox.ajax(["tasks", "--repo", "web", "--json"]),
        "ajax tasks --repo web --json",
    );
    assert_eq!(web_tasks["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(web_tasks["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(web_tasks["tasks"][0]["lifecycle_status"], "Active");

    let api_tasks = assert_json(
        &sandbox.ajax(["tasks", "--repo", "api", "--json"]),
        "ajax tasks --repo api --json",
    );
    assert_eq!(api_tasks["tasks"].as_array().unwrap().len(), 2);
    assert!(api_tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["qualified_handle"] == "api/add-search"
            && task["lifecycle_status"] == "Reviewable"));
    assert!(api_tasks["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["qualified_handle"] == "api/break-cache"
            && task["lifecycle_status"] == "Error"));

    let review = assert_json(&sandbox.ajax(["review", "--json"]), "ajax review --json");
    assert_eq!(review["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(review["tasks"][0]["qualified_handle"], "api/add-search");

    let inbox = assert_json(&sandbox.ajax(["inbox", "--json"]), "ajax inbox --json");
    let inbox_handles = inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["task_handle"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(inbox_handles.contains(&"api/add-search"));
    assert!(inbox_handles.contains(&"api/break-cache"));

    let next = assert_json(&sandbox.ajax(["next", "--json"]), "ajax next --json");
    assert_eq!(next["item"]["task_handle"], "api/break-cache");

    let status = assert_json(&sandbox.ajax(["status", "--json"]), "ajax status --json");
    assert_eq!(status["tasks"].as_array().unwrap().len(), 3);
    let cockpit = assert_json(&sandbox.ajax(["cockpit", "--json"]), "ajax cockpit --json");
    assert_eq!(cockpit["summary"]["repos"], 2);
    assert_eq!(cockpit["summary"]["tasks"], 3);
    assert_eq!(cockpit["summary"]["reviewable_tasks"], 1);
    assert!(cockpit["inbox"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["task_handle"] == "api/break-cache"));
}

#[test]
fn smoke_destructive_commands_require_confirmation() {
    let sandbox = SmokeSandbox::new("destructive-safety");
    sandbox.create_repo("web");
    sandbox.write_config(&["web"]);
    complete_web_task_to_reviewable(&sandbox);

    let merge_without_yes = sandbox.ajax(["merge", "web/fix-login", "--execute"]);
    assert_success(
        &merge_without_yes,
        "safe ajax merge --execute without explicit --yes",
    );

    let clean_without_yes = sandbox.ajax(["clean", "web/fix-login", "--execute"]);
    assert_success(
        &clean_without_yes,
        "safe ajax clean --execute without explicit --yes",
    );

    create_active_web_task(&sandbox);
    let before_remove = sandbox.command_log();
    let remove_without_yes = sandbox.ajax(["remove", "web/fix-login", "--execute"]);
    assert!(
        !remove_without_yes.status.success(),
        "remove --execute should require explicit --yes"
    );
    assert!(
        stderr(&remove_without_yes).contains("confirmation required; pass --yes"),
        "remove failure should explain confirmation:\n{}",
        stderr(&remove_without_yes)
    );
    assert_eq!(
        before_remove,
        sandbox.command_log(),
        "remove without --yes must not run external commands"
    );

    let remove = sandbox.ajax(["remove", "web/fix-login", "--execute", "--yes"]);
    assert_success(&remove, "ajax remove --execute --yes");
    let tasks = assert_json(&sandbox.ajax(["tasks", "--json"]), "ajax tasks --json");
    assert_eq!(tasks["tasks"], Value::Array(vec![]));
}
