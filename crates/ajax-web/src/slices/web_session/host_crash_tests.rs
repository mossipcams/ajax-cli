//! Host-level crash/restart tests (durable execution stage 12): a chat host
//! process is SIGKILLed mid-turn with one prompt dispatching and one queued,
//! then a fresh host on the same registry recovers. Each row gets its own test
//! function so it runs in its own nextest process (the orphan sweep runs once
//! per process).

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ajax_core::models::AgentClient;
use ajax_core::registry::{OwnerClaim, SqliteRegistryStore};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, with_test_claude_sdk_module,
    with_test_cursor_sdk_module, with_test_pi_program,
};

use crate::adapters::web_session_store::prompt_ledger::{try_load, PromptPhase};

use super::test_support::{self, BlockingSessionDirectory};
use super::SessionServerEvent;

const HANDLE: &str = "web/crash";

/// One host-crash row. `install_child` configures the harness to hold the
/// first turn (so the host is killed mid-turn); `install_restart` installs a
/// plain fixture whose resumed session answers normally.
struct CrashRow {
    name: &'static str,
    agent: AgentClient,
    first_prompt: &'static str,
    install_child: fn(&dyn Fn()),
    install_restart: fn(&dyn Fn()),
    orphan_survives_host_death: bool,
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn row(name: &str) -> CrashRow {
    match name {
        "acp" => CrashRow {
            name: "acp",
            agent: AgentClient::Codex,
            first_prompt: "hold first",
            install_child: |f| {
                with_test_acp_program(&test_support::fake_acp_fixture(), || {
                    // --survive-stdin-eof keeps the orphan alive after the
                    // host's death closes its stdin (crash premise).
                    with_test_acp_extra_args(&["--hold-prompt", "--survive-stdin-eof"], f)
                })
            },
            // The fixture persists created session ids under FAKE_ACP_STATE_DIR
            // (set to the worktree dir by the spawner, shared across phases), so
            // a plain restarted fixture accepts the previous process's id.
            install_restart: |f| with_test_acp_program(&test_support::fake_acp_fixture(), f),
            // The --survive-stdin-eof fixture outlives the host's death.
            orphan_survives_host_death: true,
        },
        "pi-rpc" => CrashRow {
            name: "pi-rpc",
            agent: AgentClient::Pi,
            first_prompt: "hold first",
            install_child: |f| {
                with_test_pi_program(
                    &manifest_dir().join("tests/fixtures/fake_pi_rpc.js"),
                    // --survive-stdin-eof keeps the orphan alive after the
                    // host's death closes its stdin (crash premise).
                    &["--hold-run", "--survive-stdin-eof"],
                    f,
                )
            },
            // Pi resumes by exact session id; the client injects `--session-id
            // <id>` itself and the fixture echoes it back in get_state.
            install_restart: |f| {
                with_test_pi_program(
                    &manifest_dir().join("tests/fixtures/fake_pi_rpc.js"),
                    &[],
                    f,
                )
            },
            // The --survive-stdin-eof fixture outlives the host's death.
            orphan_survives_host_death: true,
        },
        "claude-sdk" => CrashRow {
            name: "claude-sdk",
            agent: AgentClient::Claude,
            first_prompt: "SLOW first",
            install_child: |f| {
                with_test_claude_sdk_module(
                    &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
                    f,
                )
            },
            install_restart: |f| {
                with_test_claude_sdk_module(
                    &manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs"),
                    f,
                )
            },
            // Real Claude sidecars exit on stdin EOF, so the orphan may be gone already.
            orphan_survives_host_death: false,
        },
        "cursor-sdk" => CrashRow {
            name: "cursor-sdk",
            agent: AgentClient::Cursor,
            first_prompt: "SLOW first",
            // `Agent.resume` in the fixture accepts any id it has not explicitly
            // flagged as missing, so a fresh process resumes the previous one's
            // agent id with the same installer.
            install_child: |f| {
                with_test_cursor_sdk_module(
                    &manifest_dir().join("tests/fixtures/fake_cursor_sdk.mjs"),
                    f,
                )
            },
            install_restart: |f| {
                with_test_cursor_sdk_module(
                    &manifest_dir().join("tests/fixtures/fake_cursor_sdk.mjs"),
                    f,
                )
            },
            // Real Cursor sidecars exit on stdin EOF, so the orphan may be gone already.
            orphan_survives_host_death: false,
        },
        other => panic!("unknown host-crash row: {other}"),
    }
}

fn pid_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn kill_pid_ignoring_errors(pid: i32) {
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status();
}

/// Child mode: spawn a session, get to mid-turn (p1 dispatching, p2 queued),
/// record the harness pid, then die by SIGKILL. Nothing may exit normally.
#[test]
fn host_crash_child() {
    let Some(row_name) = std::env::var("AJAX_HOST_CRASH_ROW").ok() else {
        return; // not in child mode: nothing to do.
    };
    let state_dir = match std::env::var("AJAX_HOST_CRASH_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => panic!("AJAX_HOST_CRASH_ROW set but AJAX_HOST_CRASH_DIR missing"),
    };
    let row = row(&row_name);
    (row.install_child)(&|| run_child_scenario(&row, &state_dir));
}

fn run_child_scenario(row: &CrashRow, state_dir: &Path) {
    let dir = BlockingSessionDirectory::new(state_dir.to_path_buf());
    let worktree = state_dir.join("wt");
    std::fs::create_dir_all(&worktree).expect("create worktree dir");
    dir.acquire(HANDLE, &worktree, "auto", row.agent)
        .expect("acquire must succeed in child");
    dir.submit_prompt_with_id(HANDLE, "p1".into(), row.first_prompt.to_string())
        .expect("submit p1 in child");
    dir.submit_prompt_with_id(HANDLE, "p2".into(), "second".into())
        .expect("submit p2 in child");

    // Wait until the transcript shows the user message for p1 and the harness
    // process is actually running (p1 dispatching, p2 queued).
    test_support::pump_until(&dir, HANDLE, Duration::from_secs(15), |events| {
        test_support::has_message(events, "user", row.first_prompt)
            && dir.child_id(HANDLE).is_some()
    });

    let harness_pid = dir.child_id(HANDLE).expect("harness pid must be known");
    std::fs::write(state_dir.join("harness.pid"), harness_pid.to_string())
        .expect("write harness.pid");

    // Die by SIGKILL; the sleep is a fallback so the child can never exit
    // normally and poison the "orphan" premise.
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &std::process::id().to_string()])
        .status();
    std::thread::sleep(Duration::from_secs(30));
}

fn crash_and_restart(row_name: &str) {
    let row = row(row_name);
    let state_dir = test_support::scratch_dir(&format!("host-crash-{}", row.name));

    // Phase 1: the old host runs to mid-turn and dies by SIGKILL.
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "slices::web_session::host_crash_tests::host_crash_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AJAX_HOST_CRASH_ROW", row.name)
        .env("AJAX_HOST_CRASH_DIR", &state_dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let status = command.status().expect("spawn child test process");
    assert!(!status.success(), "child must die, not exit normally");
    #[cfg(unix)]
    {
        assert_eq!(status.signal(), Some(9), "child must be killed by SIGKILL");
    }

    let harness_pid: i32 = std::fs::read_to_string(state_dir.join("harness.pid"))
        .expect("child wrote harness.pid")
        .trim()
        .parse()
        .expect("harness.pid is numeric");
    if row.orphan_survives_host_death {
        assert!(
            pid_alive(harness_pid),
            "orphan harness must be alive after host death"
        );
    }

    // Remember the native session id before restart; recovery must resume it.
    let store = SqliteRegistryStore::new(state_dir.join("ajax.db"));
    let old_session = store
        .latest_run_for_task(HANDLE)
        .expect("read run")
        .expect("a run exists for the crashed handle")
        .native_session_id
        .expect("native session id must be recorded before crash");

    // Phase 2: a fresh host on the same registry. The first lease claim in this
    // process runs the orphan sweep and startup recovery.
    let holder = std::cell::RefCell::new(None);
    (row.install_restart)(&|| {
        let dir = BlockingSessionDirectory::new(state_dir.clone());
        let worktree = state_dir.join("wt");
        std::fs::create_dir_all(&worktree).expect("create worktree dir");
        dir.acquire(HANDLE, &worktree, "auto", row.agent)
            .expect("restart acquire (orphan sweep + startup recovery) must succeed");

        // Pump until the queued p2 turn completes in the ledger.
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            dir.pump(HANDLE);
            match try_load(&store, &state_dir, HANDLE) {
                Ok(ledger) => {
                    if ledger
                        .entry("p2")
                        .is_some_and(|entry| entry.phase == PromptPhase::Completed)
                    {
                        break;
                    }
                }
                Err(err) => panic!("ledger load failed during restart: {err}"),
            }
            if Instant::now() >= deadline {
                let ledger = try_load(&store, &state_dir, HANDLE).expect("final ledger load");
                panic!(
                    "p2 never completed after restart; ledger={:?}",
                    ledger.entries
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        holder.borrow_mut().replace(dir);
    });
    let dir = holder.take().expect("restart closure ran");

    // (1) The sweep killed the orphan harness.
    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_alive(harness_pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !pid_alive(harness_pid),
        "orphan harness must be killed by the restart sweep"
    );

    // (2) Ledger: p1 Interrupted, p2 Completed.
    let ledger = try_load(&store, &state_dir, HANDLE).expect("final ledger load");
    assert_eq!(
        ledger.entry("p1").map(|entry| entry.phase),
        Some(PromptPhase::Interrupted),
        "p1 must be Interrupted"
    );
    assert_eq!(
        ledger.entry("p2").map(|entry| entry.phase),
        Some(PromptPhase::Completed),
        "p2 must be Completed"
    );

    // (3) Transcript: p2 ran once, exactly one restart-interruption error for
    // p1, and the first prompt was NOT re-run.
    let (events, _) = dir.read_from(HANDLE, 0);
    let user_second = events.iter().filter(|event| {
        matches!(event, SessionServerEvent::Message { role, text, .. } if role == "user" && text == "second")
    }).count();
    assert_eq!(user_second, 1, "exactly one user message for p2");

    const EXPECTED_ERROR: &str = "Prompt p1 was interrupted by host restart and was not retried.";
    let expected_error = EXPECTED_ERROR;
    let restart_errors = events
        .iter()
        .filter(|event| matches!(event, SessionServerEvent::Error { message } if message == expected_error))
        .count();
    assert_eq!(
        restart_errors, 1,
        "exactly one host-restart interruption error for p1"
    );

    let first_prompt_user = events.iter().filter(|event| {
        matches!(event, SessionServerEvent::Message { role, text, .. } if role == "user" && text == row.first_prompt)
    }).count();
    assert_eq!(
        first_prompt_user, 1,
        "the first prompt must not be re-run after restart"
    );

    let error_idx = events.iter().position(
        |event| matches!(event, SessionServerEvent::Error { message } if message == expected_error),
    );
    let agent_after_error = events
        .iter()
        .skip(error_idx.map_or(0, |idx| idx + 1))
        .filter(
            |event| matches!(event, SessionServerEvent::Message { role, .. } if role == "agent"),
        )
        .count();
    assert!(
        agent_after_error >= 1,
        "p2 must produce an agent message after the error note"
    );

    // (4) Same native session resumed, not a fresh one.
    let new_session = store
        .latest_run_for_task(HANDLE)
        .expect("read run after restart")
        .and_then(|run| run.native_session_id);
    assert_eq!(
        new_session.as_deref(),
        Some(old_session.as_str()),
        "recovery must resume the same native session"
    );

    // (5) The execution owner lease belongs to the new host: refresh it with
    // this process's boot id and expect Acquired.
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let claim = store.claim_execution_owner(
        &dir.inner().ownership_boot_id(),
        std::process::id() as i64,
        now_ms,
        30_000,
    );
    match claim {
        Ok(OwnerClaim::Acquired { .. }) => {}
        other => panic!("owner lease must be acquirable by the new host: {other:?}"),
    }

    // Cleanup: no leaked harness processes, no leftover scratch dir.
    kill_pid_ignoring_errors(harness_pid);
    let _ = std::fs::remove_dir_all(&state_dir);
}

#[test]
fn acp_host_crash_recovers() {
    crash_and_restart("acp");
}

#[test]
fn pi_rpc_host_crash_recovers() {
    crash_and_restart("pi-rpc");
}

#[test]
fn cursor_sdk_host_crash_recovers() {
    crash_and_restart("cursor-sdk");
}

#[test]
fn claude_sdk_host_crash_recovers() {
    crash_and_restart("claude-sdk");
}
