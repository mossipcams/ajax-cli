use super::task_session::agent_client_name;
use super::task_session_exit::ledger_phase;
use super::test_support::{fake_acp_fixture, pump_until, scratch_dir, BlockingSessionDirectory};
use super::SessionServerEvent;
use crate::adapters::web_session_acp::{
    with_test_acp_extra_args, with_test_acp_program, with_test_claude_sdk_module,
    with_test_cursor_sdk_module, with_test_pi_program,
};
use crate::adapters::web_session_store::{load, prompt_ledger, prompt_ledger::PromptPhase};
use ajax_core::models::AgentClient;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const HOST_SWITCH_NOTE: &str = "Client switched harness. Context reset.";

#[derive(Clone, Copy)]
enum Outcome {
    Ok,
    SpawnFails,
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn healthy_fixture(agent: AgentClient) -> PathBuf {
    match agent {
        AgentClient::Codex => fake_acp_fixture(),
        AgentClient::Claude => fixture("fake_claude_sdk.mjs"),
        AgentClient::Cursor => fixture("fake_cursor_sdk.mjs"),
        AgentClient::Pi => fixture("fake_pi_rpc.js"),
        AgentClient::Other => unreachable!("Other is not a swap target"),
    }
}

fn exit_before_handshake(dir: &Path, agent: AgentClient) -> PathBuf {
    let script = dir.join(format!(
        "{}-exit-before-handshake.mjs",
        agent_client_name(agent)
    ));
    std::fs::write(&script, "#!/usr/bin/env node\nprocess.exit(1);\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

fn with_harness<R>(
    agent: AgentClient,
    dir: &Path,
    spawn_fails: bool,
    body: impl FnOnce() -> R,
) -> R {
    let program = if spawn_fails {
        exit_before_handshake(dir, agent)
    } else {
        healthy_fixture(agent)
    };
    match agent {
        AgentClient::Codex => with_test_acp_program(&program, || {
            with_test_acp_extra_args(&["--hold-prompt"], body)
        }),
        AgentClient::Claude => with_test_claude_sdk_module(&program, body),
        AgentClient::Cursor => with_test_cursor_sdk_module(&program, body),
        AgentClient::Pi => with_test_pi_program(&program, &["--hold-run"], body),
        AgentClient::Other => unreachable!("Other is not a swap target"),
    }
}

fn resume_id(dir: &Path, handle: &str) -> Option<String> {
    load::<SessionServerEvent>(dir, handle).acp_session_id
}

fn queued_in_ledger(dir: &Path, handle: &str) -> usize {
    prompt_ledger::load(dir, handle)
        .expect("load prompt ledger")
        .queued_count()
}

fn is_switch_note(event: &SessionServerEvent) -> bool {
    matches!(
        event,
        SessionServerEvent::Message { text, .. } if text == HOST_SWITCH_NOTE
    )
}

fn assert_completed_swap(
    directory: &BlockingSessionDirectory,
    dir: &Path,
    handle: &str,
    to: AgentClient,
    from_id: Option<String>,
    swap: Result<(), String>,
) {
    swap.expect("swap to a healthy harness");
    let stored = load::<SessionServerEvent>(dir, handle);
    assert_eq!(
        stored.harness,
        agent_client_name(to),
        "session must move to the new harness"
    );
    assert!(
        directory.child_id(handle).is_some(),
        "new harness must be live after the swap"
    );
    if to == AgentClient::Claude {
        assert_eq!(
            stored.acp_session_id, None,
            "claude saves its resume id only after its first prompt"
        );
    } else {
        assert!(
            stored.acp_session_id.is_some(),
            "new harness must save its resume id at spawn"
        );
    }
    assert_ne!(
        stored.acp_session_id, from_id,
        "old harness resume id must be gone"
    );
    assert_eq!(
        queued_in_ledger(dir, handle),
        0,
        "queued prompts must be discarded in the ledger"
    );
    assert!(
        stored.events.iter().any(is_switch_note),
        "transcript must record the host switch note"
    );
    directory.pump(handle);
    let (events, _) = directory.read_from(handle, 0);
    let note = events
        .iter()
        .position(is_switch_note)
        .expect("transcript must record the host switch note");
    assert!(
        !events[note..]
            .iter()
            .any(|event| matches!(event, SessionServerEvent::TurnEnd { .. })),
        "discarded queued prompt must not start a turn on the new harness"
    );
    directory.detach_session(handle);
}

fn assert_failed_swap(
    directory: &BlockingSessionDirectory,
    dir: &Path,
    handle: &str,
    from: AgentClient,
    to: AgentClient,
    from_id: Option<String>,
    swap: Result<(), String>,
) {
    let error = swap.expect_err("a failed spawn must fail the swap");
    assert!(
        error.contains(agent_client_name(to)),
        "swap failure must name the target harness: {error}"
    );
    let stored = load::<SessionServerEvent>(dir, handle);
    assert_eq!(
        stored.harness,
        agent_client_name(from),
        "session must stay on the old harness"
    );
    let live = directory.child_id(handle).is_some();
    let resumable = stored.acp_session_id == from_id;
    assert!(
        live || resumable,
        "old harness must stay live or resumable after a failed spawn"
    );
    assert_eq!(
        stored.acp_session_id, from_id,
        "old harness resume id must survive a failed spawn"
    );
    assert_eq!(
        queued_in_ledger(dir, handle),
        1,
        "queued prompt must survive a failed spawn in the ledger"
    );
    assert!(
        stored.events.iter().any(|event| matches!(
            event,
            SessionServerEvent::Error { message } if message.contains(agent_client_name(to))
        )),
        "failed spawn must surface an Error event naming the target harness"
    );
    directory
        .cancel(handle, true)
        .expect("old harness must still accept cancel");
    pump_until(directory, handle, Duration::from_secs(5), |_| {
        matches!(
            ledger_phase(dir, handle, "queued-1"),
            Some(PromptPhase::Dispatching) | Some(PromptPhase::Completed)
        )
    });
    directory.detach_session(handle);
}

fn run_swap_case(from: AgentClient, to: AgentClient, outcome: Outcome) {
    let spawn_fails = matches!(outcome, Outcome::SpawnFails);
    let label = format!(
        "swap-{}-to-{}-{}",
        agent_client_name(from),
        agent_client_name(to),
        if spawn_fails { "spawn-fails" } else { "ok" }
    );
    let dir = scratch_dir(&label);
    let handle = format!("web/{label}");
    let directory = BlockingSessionDirectory::new(dir.clone());
    let hold_prompt = if matches!(from, AgentClient::Claude | AgentClient::Cursor) {
        "SLOW"
    } else {
        "hold"
    };

    with_harness(from, &dir, false, || {
        directory
            .acquire(&handle, &dir, "auto", from)
            .expect("acquire old harness");
        directory
            .submit_prompt_with_id(&handle, "active-1".into(), hold_prompt.into())
            .expect("active prompt");
        directory
            .submit_prompt_with_id(&handle, "queued-1".into(), "queued".into())
            .expect("queued prompt");
    });
    let from_id = resume_id(&dir, &handle);
    assert!(
        from_id.is_some(),
        "old harness must report a resume id before the swap"
    );
    assert_eq!(
        queued_in_ledger(&dir, &handle),
        1,
        "queued prompt must be durable before the swap"
    );

    let swap = with_harness(to, &dir, spawn_fails, || {
        directory.runtime_handle().block_on(
            directory
                .inner()
                .reset_harness_context(&handle, &dir, to, "auto"),
        )
    });
    match outcome {
        Outcome::Ok => assert_completed_swap(&directory, &dir, &handle, to, from_id, swap),
        Outcome::SpawnFails => {
            assert_failed_swap(&directory, &dir, &handle, from, to, from_id, swap)
        }
    }
    drop(directory);
    let _ = std::fs::remove_dir_all(dir);
}

macro_rules! swap_matrix {
    ($($ok:ident, $fails:ident: $from:ident => $to:ident;)*) => {
        $(
            #[test]
            fn $ok() {
                run_swap_case(AgentClient::$from, AgentClient::$to, Outcome::Ok);
            }

            #[test]
            fn $fails() {
                run_swap_case(AgentClient::$from, AgentClient::$to, Outcome::SpawnFails);
            }
        )*
    };
}

swap_matrix! {
    swap_claude_to_codex_ok, swap_claude_to_codex_spawn_fails: Claude => Codex;
    swap_claude_to_cursor_ok, swap_claude_to_cursor_spawn_fails: Claude => Cursor;
    swap_claude_to_pi_ok, swap_claude_to_pi_spawn_fails: Claude => Pi;
    swap_codex_to_claude_ok, swap_codex_to_claude_spawn_fails: Codex => Claude;
    swap_codex_to_cursor_ok, swap_codex_to_cursor_spawn_fails: Codex => Cursor;
    swap_codex_to_pi_ok, swap_codex_to_pi_spawn_fails: Codex => Pi;
    swap_cursor_to_claude_ok, swap_cursor_to_claude_spawn_fails: Cursor => Claude;
    swap_cursor_to_codex_ok, swap_cursor_to_codex_spawn_fails: Cursor => Codex;
    swap_cursor_to_pi_ok, swap_cursor_to_pi_spawn_fails: Cursor => Pi;
    swap_pi_to_claude_ok, swap_pi_to_claude_spawn_fails: Pi => Claude;
    swap_pi_to_codex_ok, swap_pi_to_codex_spawn_fails: Pi => Codex;
    swap_pi_to_cursor_ok, swap_pi_to_cursor_spawn_fails: Pi => Cursor;
}
