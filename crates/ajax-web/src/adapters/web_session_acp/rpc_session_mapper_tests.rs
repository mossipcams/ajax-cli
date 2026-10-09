//! Checks that the session driver delegates record meaning to its mapper.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::jsonl_process::JsonlProcess;
use super::pi_rpc_handshake::handshake;
use super::rpc_handshake::RpcHandshake;
use super::rpc_session::{RecordMapper, RpcMapping, RpcSession, RpcStep};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Spawn the fake pi RPC child under `node`, passing `extra_flags` to the
/// fixture script. Fails loudly (panics) when `node` is missing.
fn spawn_fake_pi(extra_flags: &[&str]) -> JsonlProcess {
    let mut args: Vec<String> = vec![Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fake_pi_rpc.js")
        .to_string_lossy()
        .into_owned()];
    args.extend(extra_flags.iter().copied().map(str::to_owned));
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    JsonlProcess::spawn(Path::new("node"), &args, cwd)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

/// Spawn the fake and run the real four-command handshake.
fn handshaken(extra_flags: &[&str]) -> (JsonlProcess, RpcHandshake) {
    let mut process = spawn_fake_pi(extra_flags);
    let handshake = handshake(&mut process, HANDSHAKE_TIMEOUT).expect("handshake");
    (process, handshake)
}

fn finish_on_agent_start(record: &Value, _id: &str) -> RpcMapping {
    RpcMapping {
        events: Vec::new(),
        finished: record["type"] == "agent_start",
        ended_aborted: false,
    }
}

fn ignore_record(_record: &Value, _id: &str) -> RpcMapping {
    RpcMapping {
        events: Vec::new(),
        finished: false,
        ended_aborted: false,
    }
}

#[test]
fn injected_mapper_finishes_on_agent_start() {
    let (process, handshake) = handshaken(&[]);
    let mut session = RpcSession::new(process, handshake, finish_on_agent_start);
    session.begin_prompt("hello").expect("prompt");

    assert!(matches!(
        session.next_step(Duration::from_secs(10)),
        RpcStep::RunFinished { aborted: false }
    ));
    assert!(!session.run_active());
}

#[test]
fn empty_mapper_keeps_run_active() {
    let (process, handshake) = handshaken(&[]);
    let mut session = RpcSession::new(process, handshake, ignore_record);
    session.begin_prompt("hello").expect("prompt");

    assert!(matches!(
        session.next_step(Duration::from_secs(1)),
        RpcStep::Idle
    ));
    assert!(session.run_active());
}

#[test]
fn provider_mappers_match_record_mapper_type() {
    let _: RecordMapper = super::claude_sdk_map::map_sdk_message;
    let _: RecordMapper = super::pi_rpc_map::map_record;
}
