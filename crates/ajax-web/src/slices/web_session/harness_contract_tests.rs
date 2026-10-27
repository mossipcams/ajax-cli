use std::path::Path;
use std::time::{Duration, Instant};

use agent_client_protocol::schema::v1::{ContentBlock, TextContent};
use ajax_core::models::AgentClient;

use crate::adapters::web_session_acp::{
    with_test_acp_program, with_test_claude_sdk_module, with_test_cursor_sdk_module,
    with_test_pi_program, SessionClient,
};

use super::acp_drain::{drain_acp_events, PromptTerminalOutcome};
use super::acp_usage::UsageDeduper;
use super::{test_support, SessionServerEvent};

type ContractInstaller = Box<dyn FnOnce(&dyn Fn())>;

struct ContractRow {
    name: &'static str,
    agent: AgentClient,
    install: ContractInstaller,
}

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn contract_rows() -> Vec<ContractRow> {
    vec![
        ContractRow {
            name: "acp",
            agent: AgentClient::Codex,
            install: Box::new(|f| with_test_acp_program(&test_support::fake_acp_fixture(), f)),
        },
        ContractRow {
            name: "pi-rpc",
            agent: AgentClient::Pi,
            install: Box::new(|f| {
                let fixture = manifest_dir().join("tests/fixtures/fake_pi_rpc.js");
                with_test_pi_program(&fixture, &[], f)
            }),
        },
        ContractRow {
            name: "claude-sdk",
            agent: AgentClient::Claude,
            install: Box::new(|f| {
                let fixture = manifest_dir().join("tests/fixtures/fake_claude_sdk.mjs");
                with_test_claude_sdk_module(&fixture, f)
            }),
        },
        ContractRow {
            name: "cursor-sdk",
            agent: AgentClient::Cursor,
            install: Box::new(|f| {
                let fixture = manifest_dir().join("tests/fixtures/fake_cursor_sdk.mjs");
                with_test_cursor_sdk_module(&fixture, f)
            }),
        },
    ]
}

fn drain_until_terminal(
    name: &str,
    client: &SessionClient,
    deduper: &mut UsageDeduper,
) -> (
    Vec<SessionServerEvent>,
    Vec<super::acp_drain::PromptTerminal>,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events: Vec<SessionServerEvent> = Vec::new();
    let mut terminals: Vec<super::acp_drain::PromptTerminal> = Vec::new();
    while terminals.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
        let outcome = drain_acp_events(client, deduper);
        if outcome.host_exited {
            break;
        }
        events.extend(outcome.events);
        terminals.extend(outcome.prompt_terminals);
    }
    if terminals.is_empty() {
        panic!(
            "{name}: no prompt terminal within 10s (events: {})",
            events.len()
        );
    }
    (events, terminals)
}

fn run_contract(name: &str, agent: AgentClient) {
    let work_dir = test_support::scratch_dir(&format!("harness-contract-{name}"));
    let (mut client, _report) =
        match SessionClient::spawn_with_operator_pin(agent, &work_dir, "default", None) {
            Ok(pair) => pair,
            Err(err) => panic!("{name}: spawn_with_operator_pin must succeed: {err}"),
        };

    assert!(
        !client.session_id().is_empty(),
        "{name}: session_id must be non-empty after spawn"
    );
    assert!(
        client.child_pid().is_some(),
        "{name}: child_pid must report the live host process"
    );

    let request_id = match client.begin_prompt(&[ContentBlock::Text(TextContent::new("hello"))]) {
        Ok(id) => id,
        Err(err) => panic!("{name}: begin_prompt must succeed: {err}"),
    };

    let mut deduper = UsageDeduper::default();
    let (events, terminals) = drain_until_terminal(name, &client, &mut deduper);

    assert_eq!(
        terminals.len(),
        1,
        "{name}: exactly one prompt terminal expected"
    );
    assert_eq!(
        terminals[0].request_id, request_id,
        "{name}: the terminal must belong to the submitted prompt"
    );
    assert_eq!(
        terminals[0].outcome,
        PromptTerminalOutcome::Success,
        "{name}: the prompt terminal must be Success"
    );
    assert!(
        events.iter().any(|event| {
            matches!(
                event,
                SessionServerEvent::Message { role, .. } if role == "agent"
            )
        }),
        "{name}: drain must yield at least one agent Message"
    );
    assert!(
        !client.prompt_in_flight(),
        "{name}: prompt_in_flight must be false after the terminal"
    );

    let cancel = match client.cancel() {
        Ok(outcome) => outcome,
        Err(err) => panic!("{name}: idle cancel must succeed: {err}"),
    };
    assert!(
        cancel.permissions.is_empty() && cancel.elicitations.is_empty(),
        "{name}: idle cancel must report no pending permissions or elicitations (issue 1272)"
    );

    assert!(
        client.shutdown().is_none(),
        "{name}: shutdown must return None with nothing left to surface"
    );
    assert!(
        client.detach().is_none(),
        "{name}: detach after shutdown must return None"
    );

    let _ = std::fs::remove_dir_all(&work_dir);
}

#[test]
fn harness_execution_contract_rows_follow_core_session_contract() {
    for row in contract_rows() {
        let name = row.name;
        let agent = row.agent;
        (row.install)(&|| run_contract(name, agent));
    }
}
