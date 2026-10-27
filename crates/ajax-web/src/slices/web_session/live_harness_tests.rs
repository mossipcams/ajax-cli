use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use ajax_core::models::AgentClient;

use crate::adapters::program::{global_npm_root, resolve_program};
use crate::adapters::web_session_acp::{
    with_test_claude_sdk_module, with_test_cursor_sdk_module, with_test_pi_program,
    NeutralConfigCategory, NeutralConfigKind, SessionClient, SessionConfigValue,
};

use super::acp_drain::{drain_acp_events, PromptTerminal, PromptTerminalOutcome};
use super::acp_usage::UsageDeduper;
use super::{test_support, SessionServerEvent};

const TURN_BUDGET: Duration = Duration::from_secs(240);
const CANCEL_AFTER: Duration = Duration::from_secs(6);
const SECRET_NOTES: &str = "the secret word is pineapple\n";
const TOOL_PROMPT: &str = "Using your tools, briefly: run the shell command `echo probe-ok`, \
    read notes.txt, and create out.txt containing 'written'. Then reply with the secret word \
    from notes.txt.";
const LONG_PROMPT: &str = "Write the numbers 1 to 400, one per line, no tools.";

#[derive(Clone, Copy, Debug)]
enum Harness {
    Claude,
    Pi,
    Cursor,
    Codex,
}

impl Harness {
    fn parse(name: &str) -> Harness {
        match name {
            "claude" => Harness::Claude,
            "pi" => Harness::Pi,
            "cursor" => Harness::Cursor,
            "codex" => Harness::Codex,
            other => panic!(
                "unknown AJAX_LIVE_HARNESS entry `{other}`; expected claude, pi, cursor or codex"
            ),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Harness::Claude => "claude",
            Harness::Pi => "pi",
            Harness::Cursor => "cursor",
            Harness::Codex => "codex",
        }
    }

    fn agent(self) -> AgentClient {
        match self {
            Harness::Claude => AgentClient::Claude,
            Harness::Pi => AgentClient::Pi,
            Harness::Cursor => AgentClient::Cursor,
            Harness::Codex => AgentClient::Codex,
        }
    }
}

fn selected_harnesses() -> Vec<Harness> {
    let spec = std::env::var("AJAX_LIVE_HARNESS").unwrap_or_else(|_| "claude,pi,codex".into());
    spec.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(Harness::parse)
        .collect()
}

fn npm_package(relative: &str) -> PathBuf {
    let root =
        global_npm_root().unwrap_or_else(|| panic!("global npm root not found; is npm installed?"));
    let path = root.join(relative);
    assert!(
        path.is_file(),
        "{} is missing; install the harness package globally",
        path.display()
    );
    path
}

fn with_real_harness<R>(harness: Harness, run: impl FnOnce() -> R) -> R {
    match harness {
        Harness::Claude => {
            with_test_claude_sdk_module(&npm_package("@anthropic-ai/claude-agent-sdk/sdk.mjs"), run)
        }
        Harness::Cursor => {
            with_test_cursor_sdk_module(&npm_package("@cursor/sdk/dist/esm/index.js"), run)
        }
        Harness::Pi => {
            let pi = resolve_program("pi").unwrap_or_else(|| panic!("`pi` is not on PATH"));
            with_test_pi_program(&pi, &[], run)
        }
        Harness::Codex => run(),
    }
}

struct Turn {
    events: Vec<SessionServerEvent>,
    terminals: Vec<PromptTerminal>,
}

impl Turn {
    fn all_events(&self) -> impl Iterator<Item = &SessionServerEvent> {
        self.events
            .iter()
            .chain(self.terminals.iter().flat_map(|t| t.events.iter()))
    }

    fn outcome(&self) -> PromptTerminalOutcome {
        self.terminals[0].outcome
    }

    fn stop_reason(&self) -> Option<&str> {
        self.all_events().find_map(|event| match event {
            SessionServerEvent::TurnEnd { stop_reason } => stop_reason.as_deref(),
            _ => None,
        })
    }

    fn has_tool_call(&self) -> bool {
        self.all_events()
            .any(|event| matches!(event, SessionServerEvent::ToolCall { .. }))
    }

    fn has_plan(&self) -> bool {
        self.all_events()
            .any(|event| matches!(event, SessionServerEvent::Plan { .. }))
    }

    fn has_agent_message(&self) -> bool {
        self.all_events().any(
            |event| matches!(event, SessionServerEvent::Message { role, .. } if role == "agent"),
        )
    }
}

fn run_turn(
    label: &str,
    client: &mut SessionClient,
    deduper: &mut UsageDeduper,
    text: &str,
    cancel_after: Option<Duration>,
) -> Turn {
    println!("\n=== {label}: {text}");
    let request_id = client
        .begin_prompt(&[ContentBlock::Text(TextContent::new(text))])
        .unwrap_or_else(|err| panic!("{label}: begin_prompt failed: {err}"));
    let started = Instant::now();
    let mut cancel_pending = cancel_after;
    let mut turn = Turn {
        events: Vec::new(),
        terminals: Vec::new(),
    };
    while turn.terminals.is_empty() {
        assert!(
            started.elapsed() <= TURN_BUDGET,
            "{label}: no prompt terminal within {TURN_BUDGET:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
        if cancel_pending.is_some_and(|after| started.elapsed() >= after) {
            cancel_pending = None;
            client
                .cancel()
                .unwrap_or_else(|err| panic!("{label}: cancel failed: {err}"));
        }
        let out = drain_acp_events(client, deduper);
        assert!(!out.host_exited, "{label}: host exited mid-turn");
        turn.events.extend(out.events);
        turn.terminals.extend(out.prompt_terminals);
    }
    assert_eq!(
        turn.terminals.len(),
        1,
        "{label}: exactly one prompt terminal expected"
    );
    assert_eq!(
        turn.terminals[0].request_id, request_id,
        "{label}: the terminal must belong to the submitted prompt"
    );
    print_transcript(&turn);
    println!("  terminal: {:?}", turn.outcome());
    turn
}

type MessageChunk = (String, Option<String>, String);

fn flush_chunk(open: &mut Option<MessageChunk>) {
    if let Some((role, _, text)) = open.take() {
        println!("  [{role}] {text}");
    }
}

fn print_transcript(turn: &Turn) {
    let mut open: Option<MessageChunk> = None;
    for event in turn.all_events() {
        if let SessionServerEvent::Message {
            role,
            text,
            message_id,
            ..
        } = event
        {
            let same = matches!(&open, Some((r, id, _)) if r == role && id == message_id);
            if same {
                if let Some((_, _, merged)) = open.as_mut() {
                    merged.push_str(text);
                }
            } else {
                flush_chunk(&mut open);
                open = Some((role.clone(), message_id.clone(), text.clone()));
            }
        } else {
            flush_chunk(&mut open);
            let json = serde_json::to_string(event).unwrap_or_default();
            println!("  {}", json.chars().take(240).collect::<String>());
        }
    }
    flush_chunk(&mut open);
}

fn assert_success(label: &str, turn: &Turn) {
    assert_eq!(
        turn.outcome(),
        PromptTerminalOutcome::Success,
        "{label}: the prompt must end in a Success terminal"
    );
}

fn check_harness(harness: Harness) {
    let label = harness.label();
    let agent = harness.agent();
    let dir = test_support::scratch_dir(&format!("live-harness-{label}"));
    std::fs::write(dir.join("notes.txt"), SECRET_NOTES).expect("write notes.txt");
    let git = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()
        .expect("run git init");
    assert!(git.success(), "{label}: git init failed");

    let (mut client, report) = SessionClient::spawn_with_operator_pin(agent, &dir, "default", None)
        .unwrap_or_else(|err| panic!("{label}: spawn failed: {err}"));
    assert!(
        !client.session_id().is_empty(),
        "{label}: session id must be non-empty after spawn"
    );
    let mut deduper = UsageDeduper::default();

    let tools = run_turn(label, &mut client, &mut deduper, TOOL_PROMPT, None);
    assert_success(label, &tools);
    assert!(
        tools.has_tool_call(),
        "{label}: tool turn must emit a tool_call event"
    );

    if matches!(harness, Harness::Claude) {
        let plan = run_turn(
            label,
            &mut client,
            &mut deduper,
            "Use the TaskCreate tool to create two tasks: 'read notes.txt' and 'write out.txt'. Then reply with just: tasks-created",
            None,
        );
        assert_success(label, &plan);
        assert!(
            plan.has_plan(),
            "{label}: TaskCreate turn must emit a plan event"
        );

        let context = run_turn(label, &mut client, &mut deduper, "/context", None);
        assert!(
            context.has_agent_message(),
            "{label}: /context must yield at least one agent message"
        );
    }

    let cancelled = run_turn(
        label,
        &mut client,
        &mut deduper,
        LONG_PROMPT,
        Some(CANCEL_AFTER),
    );
    let stopped_as_cancelled = cancelled.outcome() == PromptTerminalOutcome::Cancelled
        && cancelled.stop_reason() == Some("cancelled");
    assert!(
        stopped_as_cancelled || cancelled.outcome() == PromptTerminalOutcome::Success,
        "{label}: cancelled turn must end Cancelled with stopReason \"cancelled\" (or Success if the model finished first); got {:?} stopReason {:?}",
        cancelled.outcome(),
        cancelled.stop_reason()
    );
    let after_cancel = run_turn(
        label,
        &mut client,
        &mut deduper,
        "Reply with just: still-alive",
        None,
    );
    assert_success(label, &after_cancel);

    for option in report.config_options.unwrap_or_default() {
        if matches!(option.category, Some(NeutralConfigCategory::Model)) {
            continue;
        }
        let NeutralConfigKind::Select {
            current_value,
            choices,
        } = &option.kind
        else {
            continue;
        };
        let Some(other) = choices
            .iter()
            .map(|choice| choice.value.as_str())
            .find(|value| *value != current_value.as_str())
        else {
            continue;
        };
        let outcome = client
            .apply_config_option(&option.id, SessionConfigValue::Select(other.to_string()))
            .unwrap_or_else(|err| panic!("{label}: set {} failed: {err}", option.id));
        assert!(
            outcome.error.is_none(),
            "{label}: switching {} to {other} must not error: {:?}",
            option.id,
            outcome.error
        );
    }

    let session_id = client.session_id().to_string();
    client.shutdown();
    let (mut resumed, _) =
        SessionClient::spawn_with_operator_pin(agent, &dir, "default", Some(&session_id))
            .unwrap_or_else(|err| panic!("{label}: resume spawn failed: {err}"));
    assert_eq!(
        resumed.session_id(),
        session_id,
        "{label}: resume must keep the same session id"
    );
    let mut resumed_deduper = UsageDeduper::default();
    let resumed_turn = run_turn(
        label,
        &mut resumed,
        &mut resumed_deduper,
        "Reply with just: resumed",
        None,
    );
    assert_success(label, &resumed_turn);
    resumed.shutdown();

    let _ = std::fs::remove_dir_all(&dir);
}

fn panic_text(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_else(|| "panicked".to_string())
}

#[test]
#[ignore = "drives the real installed harnesses; see module docs for the run command"]
fn live_harness_parity() {
    let mut failures = Vec::new();
    for harness in selected_harnesses() {
        let run = AssertUnwindSafe(|| with_real_harness(harness, || check_harness(harness)));
        if let Err(payload) = catch_unwind(run) {
            failures.push(format!("{}: {}", harness.label(), panic_text(&*payload)));
        }
    }
    assert!(
        failures.is_empty(),
        "live harness parity failed:\n{}",
        failures.join("\n")
    );
}
