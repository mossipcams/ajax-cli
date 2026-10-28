use super::acp_adopted::{session_update_event, AcpAdoptedClient};
use super::client::AcpClientEvent;
use super::session_client::SessionClient;
use crate::adapters::session_shim::answered_request_ids;
use agent_client_protocol::schema::v1::SessionNotification;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct RecordingWriter(Arc<Mutex<Vec<u8>>>);

impl RecordingWriter {
    fn written(&self) -> Vec<u8> {
        self.0.lock().unwrap().clone()
    }
}

impl Write for RecordingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn lines(values: &[Value]) -> Vec<u8> {
    values
        .iter()
        .map(|value| format!("{value}\n"))
        .collect::<String>()
        .into_bytes()
}

fn adopt_from(bytes: Vec<u8>, prompt_request_id: u64) -> (AcpAdoptedClient, RecordingWriter) {
    adopt_with_answers(bytes, prompt_request_id, HashSet::new())
}

fn adopt_with_answers(
    bytes: Vec<u8>,
    prompt_request_id: u64,
    answered: HashSet<String>,
) -> (AcpAdoptedClient, RecordingWriter) {
    let writer = RecordingWriter::default();
    let client = AcpAdoptedClient::adopt(
        Box::new(Cursor::new(bytes)),
        Box::new(writer.clone()),
        4242,
        "session-1".to_string(),
        prompt_request_id,
        answered,
    );
    (client, writer)
}

fn permission_request(id: u64) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "session/request_permission",
        "params": {}
    })
}

fn collect_until_exit(client: &AcpAdoptedClient) -> Vec<AcpClientEvent> {
    let mut events = Vec::new();
    while let Some(event) = client.wait_event(WAIT) {
        let exited = matches!(event, AcpClientEvent::Exited);
        events.push(event);
        if exited {
            break;
        }
    }
    events
}

fn agent_message_chunk_params() -> Value {
    json!({
        "sessionId": "session-1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "hello" }
        }
    })
}

fn session_update_line(params: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": "session/update", "params": params })
}

fn stop_reason_response(id: u64) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": { "stopReason": "end_turn" } })
}

#[test]
fn replayed_agent_message_chunk_matches_live_mapping() {
    let params = agent_message_chunk_params();
    let live = session_update_event(
        serde_json::from_value::<SessionNotification>(params.clone())
            .expect("fixture is a valid session notification"),
    )
    .expect("message chunks always map to an event");

    let (client, _writer) = adopt_from(lines(&[session_update_line(&params)]), 7);
    let events = collect_until_exit(&client);

    assert_eq!(events.len(), 2);
    assert_eq!(format!("{:?}", events[0]), format!("{live:?}"));
    assert!(matches!(
        &events[0],
        AcpClientEvent::SessionUpdate(notification) if matches!(
            notification.update,
            agent_client_protocol::schema::v1::SessionUpdate::AgentMessageChunk(_)
        )
    ));
    assert!(matches!(events[1], AcpClientEvent::Exited));
}

#[test]
fn replayed_session_info_update_trims_title_like_live_mapping() {
    let params = json!({
        "sessionId": "session-1",
        "update": { "sessionUpdate": "session_info_update", "title": "  Hello  " }
    });
    let live = session_update_event(
        serde_json::from_value::<SessionNotification>(params.clone())
            .expect("fixture is a valid session notification"),
    )
    .expect("a non-empty title maps to an event");

    let (client, _writer) = adopt_from(lines(&[session_update_line(&params)]), 7);
    let events = collect_until_exit(&client);

    assert_eq!(format!("{:?}", events[0]), format!("{live:?}"));
    assert!(matches!(
        &events[0],
        AcpClientEvent::SessionInfoUpdated { title } if title.as_deref() == Some("Hello")
    ));
}

#[test]
fn undecodable_session_update_replays_as_unknown_update() {
    let params = json!({ "sessionId": "session-1", "update": { "sessionUpdate": "no_such_kind" } });
    let (client, _writer) = adopt_from(lines(&[session_update_line(&params)]), 7);
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::UnknownSessionUpdate(value) if *value == params
    ));
}

#[test]
fn stop_reason_response_emits_request_finished_and_clears_in_flight() {
    let (client, _writer) = adopt_from(lines(&[stop_reason_response(9)]), 9);
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::RequestFinished { id: 9, method: "session/prompt", result: Ok(result) }
            if result["stopReason"] == "end_turn"
    ));
    assert!(!client.prompt_in_flight());
}

#[test]
fn error_response_ends_the_adopted_turn_with_its_message() {
    let error = json!({
        "jsonrpc": "2.0",
        "id": 9,
        "error": { "code": -32000, "message": "rate limited" }
    });
    let (client, _writer) = adopt_from(lines(&[error]), 9);
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::RequestFinished { id: 9, method: "session/prompt", result: Err(message) }
            if message == "rate limited"
    ));
    assert!(!client.prompt_in_flight());
}

#[test]
fn prompt_stays_in_flight_without_a_turn_end_line() {
    let (client, _writer) = adopt_from(Vec::new(), 9);
    let events = collect_until_exit(&client);

    assert!(matches!(events.as_slice(), [AcpClientEvent::Exited]));
    assert!(client.prompt_in_flight());
    assert!(client.host_exited());
}

#[test]
fn response_with_a_different_request_id_still_ends_the_adopted_turn() {
    let (client, _writer) = adopt_from(lines(&[stop_reason_response(3)]), 9);
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::RequestFinished { id: 9, .. }
    ));
}

#[test]
fn answered_agent_request_is_skipped_and_the_turn_finishes_on_stop_reason() {
    let bytes = lines(&[
        permission_request(3),
        session_update_line(&agent_message_chunk_params()),
        stop_reason_response(9),
    ]);
    let (client, writer) = adopt_with_answers(bytes, 9, HashSet::from(["3".to_string()]));
    let events = collect_until_exit(&client);

    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        AcpClientEvent::SessionUpdate(notification) if matches!(
            notification.update,
            agent_client_protocol::schema::v1::SessionUpdate::AgentMessageChunk(_)
        )
    ));
    assert!(matches!(
        &events[1],
        AcpClientEvent::RequestFinished { id: 9, method: "session/prompt", result: Ok(result) }
            if result["stopReason"] == "end_turn"
    ));
    assert!(matches!(events[2], AcpClientEvent::Exited));
    assert!(writer.written().is_empty());
    assert!(!client.prompt_in_flight());
}

#[test]
fn answered_ids_read_from_the_shim_inbound_log_keep_replay_alive() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/acp-adopted-tests/answered-inbound");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let inbound = dir.join("session.shim.in.log");
    std::fs::write(
        &inbound,
        format!(
            "{}\n",
            json!({ "jsonrpc": "2.0", "id": 3, "result": { "outcome": "selected" } })
        ),
    )
    .expect("write inbound log");

    let answered: HashSet<String> = answered_request_ids(&inbound)
        .expect("read inbound log")
        .iter()
        .map(Value::to_string)
        .collect();
    let (client, writer) = adopt_with_answers(
        lines(&[permission_request(3), stop_reason_response(9)]),
        9,
        answered,
    );
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::RequestFinished {
            id: 9,
            method: "session/prompt",
            result: Ok(_)
        }
    ));
    assert!(writer.written().is_empty());
}

#[test]
fn unanswered_agent_request_still_shuts_down_when_other_ids_were_answered() {
    let bytes = lines(&[permission_request(3), stop_reason_response(9)]);
    let (client, writer) = adopt_with_answers(bytes, 9, HashSet::from(["4".to_string()]));
    let events = collect_until_exit(&client);

    assert!(matches!(
        &events[0],
        AcpClientEvent::Error(message) if message.contains("session/request_permission")
    ));
    assert_eq!(writer.written(), b"SHUTDOWN\n".to_vec());
    assert!(!client.prompt_in_flight());
    assert!(client.host_exited());
}

#[test]
fn agent_request_writes_shutdown_interrupts_the_turn_and_stops_reading() {
    let request = permission_request(3);
    let trailing = session_update_line(&agent_message_chunk_params());
    let (client, writer) = adopt_from(lines(&[request, trailing]), 9);
    let events = collect_until_exit(&client);

    assert_eq!(events.len(), 3);
    assert!(matches!(
        &events[0],
        AcpClientEvent::Error(message) if message.contains("session/request_permission")
    ));
    assert!(matches!(
        &events[1],
        AcpClientEvent::RequestFinished { id: 9, method: "session/prompt", result: Ok(result) }
            if result["stopReason"] == "cancelled"
    ));
    assert!(matches!(events[2], AcpClientEvent::Exited));
    assert_eq!(writer.written(), b"SHUTDOWN\n".to_vec());
    assert!(!client.prompt_in_flight());
    assert!(client.host_exited());
}

#[test]
fn adopted_client_never_writes_anything_but_shutdown() {
    let bytes = lines(&[
        session_update_line(&agent_message_chunk_params()),
        stop_reason_response(9),
    ]);
    let (mut client, writer) = adopt_from(bytes, 9);
    collect_until_exit(&client);
    assert!(writer.written().is_empty());

    client.shutdown();

    assert_eq!(writer.written(), b"SHUTDOWN\n".to_vec());
    assert!(client.host_exited());
}

#[test]
fn shutdown_reaches_the_shim_socket_as_a_single_shutdown_line() {
    let (web_end, shim_end) = UnixStream::pair().expect("socket pair");
    shim_end.set_read_timeout(Some(WAIT)).expect("read timeout");
    let mut shim_writer = shim_end.try_clone().expect("clone shim end");
    shim_writer
        .write_all(&lines(&[stop_reason_response(9)]))
        .expect("shim writes turn end");

    let reader = web_end.try_clone().expect("clone web end");
    let mut client = AcpAdoptedClient::adopt(
        Box::new(reader),
        Box::new(web_end),
        4242,
        "session-1".to_string(),
        9,
        HashSet::new(),
    );
    assert!(matches!(
        client.wait_event(WAIT),
        Some(AcpClientEvent::RequestFinished { id: 9, .. })
    ));

    client.shutdown();

    let mut received = String::new();
    BufReader::new(shim_end)
        .read_line(&mut received)
        .expect("shim reads shutdown");
    assert_eq!(received, "SHUTDOWN\n");
}

#[test]
fn session_client_adopted_arm_refuses_new_prompts_and_shuts_down() {
    let writer = RecordingWriter::default();
    let client = AcpAdoptedClient::adopt(
        Box::new(Cursor::new(Vec::new())),
        Box::new(writer.clone()),
        4242,
        "session-1".to_string(),
        9,
        HashSet::new(),
    );
    let mut session = SessionClient::AcpAdopted(client);

    assert_eq!(session.session_id(), "session-1");
    assert_eq!(session.child_pid(), Some(4242));
    assert!(session
        .begin_prompt(&[])
        .unwrap_err()
        .contains("finishing a turn"));
    assert!(session
        .respond_client_request(&json!(1), json!({}))
        .is_err());

    session.shutdown();

    assert_eq!(writer.written(), b"SHUTDOWN\n".to_vec());
    assert!(session.host_exited());
}
