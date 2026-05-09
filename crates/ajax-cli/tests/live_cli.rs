use ajax_core::{
    models::{AgentClient, LifecycleStatus, SideFlag, Task, TaskId},
    registry::{InMemoryRegistry, Registry, RegistryStore, SqliteRegistryStore},
};
use serde_json::Value;
use std::{
    ffi::OsStr,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn ajax_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_ajax"))
}

struct IsolatedAjaxHome {
    root: PathBuf,
    config_file: PathBuf,
    state_file: PathBuf,
}

impl IsolatedAjaxHome {
    fn new(test_name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("ajax-live-cli-{test_name}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap_or_else(|error| {
                panic!("failed to remove old temp home {}: {error}", root.display())
            });
        }
        std::fs::create_dir_all(&root).unwrap_or_else(|error| {
            panic!("failed to create temp home {}: {error}", root.display())
        });

        Self {
            config_file: root.join("config.toml"),
            state_file: root.join("ajax.db"),
            root,
        }
    }

    fn ajax<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        Command::new(ajax_binary())
            .args(args)
            .env("HOME", &self.root)
            .env("AJAX_CONFIG", &self.config_file)
            .env("AJAX_STATE", &self.state_file)
            .output()
            .unwrap_or_else(|error| panic!("failed to run live ajax binary: {error}"))
    }

    fn ajax_with_fake_tools<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let path = format!(
            "{}:{}",
            self.fake_bin_dir().display(),
            std::env::var("PATH").unwrap_or_default()
        );

        Command::new(ajax_binary())
            .args(args)
            .env("HOME", &self.root)
            .env("AJAX_CONFIG", &self.config_file)
            .env("AJAX_STATE", &self.state_file)
            .env("AJAX_FAKE_WORKMUX_LOG", self.fake_workmux_log())
            .env("PATH", path)
            .output()
            .unwrap_or_else(|error| panic!("failed to run live ajax binary: {error}"))
    }

    fn create_managed_repo(&self, name: &str) -> PathBuf {
        let path = self.root.join("repos").join(name);
        fs::create_dir_all(&path).unwrap_or_else(|error| {
            panic!("failed to create managed repo {}: {error}", path.display())
        });
        path
    }

    fn write_config(&self, contents: &str) {
        fs::write(&self.config_file, contents).unwrap_or_else(|error| {
            panic!(
                "failed to write isolated config {}: {error}",
                self.config_file.display()
            )
        });
    }

    fn install_fake_workmux(&self) -> PathBuf {
        let fake_bin = self.fake_bin_dir();
        fs::create_dir_all(&fake_bin).unwrap_or_else(|error| {
            panic!(
                "failed to create fake tool directory {}: {error}",
                fake_bin.display()
            )
        });
        let script = fake_bin.join("workmux");
        fs::write(
            &script,
            r#"#!/bin/sh
{
  printf 'cwd=%s\n' "$PWD"
  printf 'args='
  first=1
  for arg in "$@"; do
    if [ "$first" = 1 ]; then
      first=0
      printf '%s' "$arg"
    else
      printf ' %s' "$arg"
    fi
  done
  printf '\n'
} >> "$AJAX_FAKE_WORKMUX_LOG"
printf 'fake workmux %s' "$1"
"#,
        )
        .unwrap_or_else(|error| {
            panic!("failed to write fake workmux {}: {error}", script.display())
        });
        let mut permissions = fs::metadata(&script)
            .unwrap_or_else(|error| panic!("failed to stat fake workmux: {error}"))
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions)
            .unwrap_or_else(|error| panic!("failed to make fake workmux executable: {error}"));

        self.fake_workmux_log()
    }

    fn seed_risky_reviewable_task(&self, repo: &str, repo_path: &Path) {
        let mut registry = InMemoryRegistry::default();
        let mut task = Task::new(
            TaskId::new(format!("{repo}/fix-login")),
            repo,
            "fix-login",
            "Fix login",
            "ajax/fix-login",
            "main",
            repo_path.join(".ajax-worktrees/fix-login"),
            "ajax-web-fix-login",
            "worktrunk",
            AgentClient::Codex,
        );
        task.lifecycle_status = LifecycleStatus::Reviewable;
        task.add_side_flag(SideFlag::NeedsInput);
        registry
            .create_task(task)
            .expect("fixture task should be inserted");
        SqliteRegistryStore::new(&self.state_file)
            .save(&registry)
            .expect("fixture task should be saved to SQLite state");
    }

    fn fake_bin_dir(&self) -> PathBuf {
        self.root.join("fake-bin")
    }

    fn fake_workmux_log(&self) -> PathBuf {
        self.root.join("workmux.log")
    }

    fn state_file(&self) -> &Path {
        &self.state_file
    }
}

impl Drop for IsolatedAjaxHome {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                panic!(
                    "failed to remove temp home {} during cleanup: {error}",
                    self.root.display()
                );
            }
        }
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("ajax stdout should be valid UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("ajax stderr should be valid UTF-8")
}

#[test]
fn live_help_exposes_the_scriptable_command_surface() {
    let home = IsolatedAjaxHome::new("help");

    let output = home.ajax(["--help"]);

    assert!(
        output.status.success(),
        "ajax --help should succeed, stderr:\n{}",
        stderr(&output)
    );
    assert_eq!(stderr(&output), "");
    let stdout = stdout(&output);
    assert!(stdout.contains("Usage: ajax [COMMAND]"), "{stdout}");
    for command in [
        "repos",
        "tasks",
        "inspect",
        "new",
        "open",
        "trunk",
        "check",
        "diff",
        "merge",
        "clean",
        "sweep",
        "repair",
        "next",
        "inbox",
        "review",
        "status",
        "doctor",
        "reconcile",
        "cockpit",
    ] {
        assert!(
            stdout.contains(command),
            "ajax --help should list `{command}` in:\n{stdout}"
        );
    }
}

#[test]
fn live_cockpit_json_uses_isolated_empty_context_without_creating_state() {
    let home = IsolatedAjaxHome::new("cockpit-json");

    let output = home.ajax(["cockpit", "--json"]);

    assert!(
        output.status.success(),
        "ajax cockpit --json should succeed, stderr:\n{}",
        stderr(&output)
    );
    assert_eq!(stderr(&output), "");
    let body: Value =
        serde_json::from_str(&stdout(&output)).expect("ajax cockpit --json should emit valid JSON");
    assert_eq!(body["repos"]["repos"], Value::Array(vec![]));
    assert_eq!(body["tasks"]["tasks"], Value::Array(vec![]));
    assert_eq!(body["review"]["tasks"], Value::Array(vec![]));
    assert_eq!(body["inbox"]["items"], Value::Array(vec![]));
    assert!(
        !home.state_file().exists(),
        "read-only cockpit JSON should not create isolated state at {}",
        home.state_file().display()
    );
}

#[test]
fn live_new_execute_records_task_and_persists_it_to_sqlite_state() {
    let home = IsolatedAjaxHome::new("new-execute");
    let repo_path = home.create_managed_repo("web");
    let workmux_log = home.install_fake_workmux();
    home.write_config(&format!(
        r#"
        [[repos]]
        name = "web"
        path = "{}"
        default_branch = "main"
        "#,
        repo_path.display()
    ));

    let output = home.ajax_with_fake_tools([
        "new",
        "--repo",
        "web",
        "--title",
        "Fix Login!",
        "--agent",
        "codex",
        "--execute",
    ]);

    assert!(
        output.status.success(),
        "ajax new --execute should succeed, stderr:\n{}",
        stderr(&output)
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        "exit:0\nstdout:fake workmux add\nstderr:\nrecorded task: web/fix-login\n"
    );
    assert_eq!(
        std::fs::read_to_string(&workmux_log).expect("fake workmux should record invocation"),
        format!(
            "cwd={}\nargs=add ajax/fix-login --prompt Fix Login! --agent codex\n",
            fs::canonicalize(&repo_path)
                .expect("managed repo path should canonicalize")
                .display()
        )
    );
    assert!(
        home.state_file().exists(),
        "ajax new --execute should create SQLite state at {}",
        home.state_file().display()
    );

    let tasks_output = home.ajax(["tasks", "--json"]);

    assert!(
        tasks_output.status.success(),
        "ajax tasks --json should load persisted state, stderr:\n{}",
        stderr(&tasks_output)
    );
    assert_eq!(stderr(&tasks_output), "");
    let body: Value = serde_json::from_str(&stdout(&tasks_output))
        .expect("ajax tasks --json should emit valid JSON");
    assert_eq!(
        body["tasks"],
        serde_json::json!([
            {
                "id": "web/fix-login",
                "qualified_handle": "web/fix-login",
                "title": "Fix Login!",
                "lifecycle_status": "Provisioning",
                "needs_attention": false
            }
        ])
    );
}

#[test]
fn live_new_execute_requires_title_before_workmux_can_run() {
    let home = IsolatedAjaxHome::new("new-execute-missing-title");
    let repo_path = home.create_managed_repo("web");
    let workmux_log = home.install_fake_workmux();
    home.write_config(&format!(
        r#"
        [[repos]]
        name = "web"
        path = "{}"
        default_branch = "main"
        "#,
        repo_path.display()
    ));

    let output = home.ajax_with_fake_tools(["new", "--repo", "web", "--execute"]);

    assert!(
        !output.status.success(),
        "ajax new --execute without a title should fail"
    );
    assert_eq!(stdout(&output), "");
    assert_eq!(
        stderr(&output),
        "CommandFailed(\"task title is required; pass --title\")\n"
    );
    assert!(
        !workmux_log.exists(),
        "workmux must not run until Ajax has a completed task title"
    );
    assert!(
        !home.state_file().exists(),
        "failed new task input should not create SQLite state at {}",
        home.state_file().display()
    );
}

#[test]
fn live_merge_execute_requires_yes_before_running_workmux_and_persists_success() {
    let home = IsolatedAjaxHome::new("merge-execute");
    let repo_path = home.create_managed_repo("web");
    let workmux_log = home.install_fake_workmux();
    home.write_config(&format!(
        r#"
        [[repos]]
        name = "web"
        path = "{}"
        default_branch = "main"
        "#,
        repo_path.display()
    ));
    home.seed_risky_reviewable_task("web", &repo_path);

    let rejected = home.ajax_with_fake_tools(["merge", "web/fix-login", "--execute"]);

    assert!(
        !rejected.status.success(),
        "ajax merge --execute should fail without --yes for risky task"
    );
    assert_eq!(stdout(&rejected), "");
    assert!(
        stderr(&rejected).contains("confirmation required; pass --yes"),
        "stderr should explain confirmation requirement:\n{}",
        stderr(&rejected)
    );
    assert!(
        !workmux_log.exists(),
        "workmux should not run before confirmation"
    );

    let merged = home.ajax_with_fake_tools(["merge", "web/fix-login", "--execute", "--yes"]);

    assert!(
        merged.status.success(),
        "ajax merge --execute --yes should succeed, stderr:\n{}",
        stderr(&merged)
    );
    assert_eq!(stderr(&merged), "");
    assert_eq!(
        stdout(&merged),
        "exit:0\nstdout:fake workmux merge\nstderr:\n"
    );
    assert_eq!(
        fs::read_to_string(&workmux_log).expect("fake workmux should record confirmed merge"),
        format!(
            "cwd={}\nargs=merge ajax/fix-login\n",
            fs::canonicalize(&repo_path)
                .expect("managed repo path should canonicalize")
                .display()
        )
    );

    let tasks_output = home.ajax(["tasks", "--json"]);

    assert!(
        tasks_output.status.success(),
        "ajax tasks --json should load merged state, stderr:\n{}",
        stderr(&tasks_output)
    );
    let body: Value = serde_json::from_str(&stdout(&tasks_output))
        .expect("ajax tasks --json should emit valid JSON");
    assert_eq!(body["tasks"][0]["qualified_handle"], "web/fix-login");
    assert_eq!(body["tasks"][0]["lifecycle_status"], "Merged");
    assert_eq!(body["tasks"][0]["needs_attention"], true);
}
