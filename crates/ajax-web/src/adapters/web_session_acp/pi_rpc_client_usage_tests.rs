use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::agent_event::{AgentEvent, NativeEvent};
use super::pi_rpc_client::PiRpcClient;

const TIMEOUT: Duration = Duration::from_secs(10);

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_pi_rpc.js")
}

fn spawn_client(extra_args: &[&str]) -> PiRpcClient {
    let args: Vec<String> = extra_args.iter().map(|arg| (*arg).to_string()).collect();
    let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
    PiRpcClient::spawn(&fixture_path(), &args, cwd, None, TIMEOUT)
        .expect("fake pi rpc fixture must spawn (is node installed?)")
}

fn text_blocks() -> String {
    "hello".to_string()
}

fn usage_update(event: &AgentEvent) -> Option<(u64, u64)> {
    match event {
        AgentEvent::Native(NativeEvent::Usage { used, size }) => Some((*used, *size)),
        _ => None,
    }
}

fn stop_reason(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::Native(NativeEvent::TurnFinished {
            result: Ok(stop), ..
        }) => Some(stop.as_str()),
        _ => None,
    }
}

fn is_agent_text(event: &AgentEvent) -> bool {
    matches!(event, AgentEvent::Native(NativeEvent::AgentText(_)))
}

fn collect_until_finished(client: &PiRpcClient) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = client
            .core()
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(&event, AgentEvent::Native(NativeEvent::TurnFinished { .. }));
        events.push(event);
        if finished {
            return events;
        }
    }
}

#[test]
fn prompt_flow_reports_usage_between_message_and_finish() {
    let client = spawn_client(&[]);
    let id = client
        .core()
        .begin_prompt(&text_blocks())
        .expect("begin_prompt");

    let events = collect_until_finished(&client);

    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );

    let usage_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| usage_update(event).map(|_| i))
        .collect();
    assert_eq!(
        usage_indices,
        vec![finish_index - 1],
        "expected exactly one Usage right before the finish: {events:?}"
    );
    assert_eq!(
        usage_update(&events[finish_index - 1]),
        Some((10362, 1000000)),
        "usage numbers must come from get_session_stats: {events:?}"
    );

    let chunk_index = events
        .iter()
        .position(is_agent_text)
        .expect("an AgentText chunk must precede the usage update");
    assert!(
        chunk_index < usage_indices[0],
        "message chunk must come before the usage update: {events:?}"
    );

    assert_eq!(stop_reason(&events[finish_index]), Some("end_turn"));
    assert!(matches!(
        &events[finish_index],
        AgentEvent::Native(NativeEvent::TurnFinished { request_id: finish_id, result: Ok(_) })
            if *finish_id == id
    ));
}

#[test]
fn silent_stats_still_finishes_without_usage() {
    let client = spawn_client(&["--silent-stats"]);
    let id = client
        .core()
        .begin_prompt(&text_blocks())
        .expect("begin_prompt");

    let events = collect_until_finished(&client);

    assert!(
        events.iter().all(|event| usage_update(event).is_none()),
        "no Usage may be reported when stats are silent: {events:?}"
    );
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must still arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );
    assert_eq!(stop_reason(&events[finish_index]), Some("end_turn"));
    assert!(matches!(
        &events[finish_index],
        AgentEvent::Native(NativeEvent::TurnFinished { request_id: finish_id, result: Ok(_) })
            if *finish_id == id
    ));
}

#[test]
fn aborted_run_reports_usage_before_cancelled_finish() {
    let client = spawn_client(&["--hold-run"]);
    let id = client
        .core()
        .begin_prompt(&text_blocks())
        .expect("begin_prompt");

    let deadline = Instant::now() + TIMEOUT;
    while !client.core().prompt_in_flight() {
        assert!(
            Instant::now() < deadline,
            "held run never became active before cancel"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    client.core().cancel().expect("cancel");

    let events = collect_until_finished(&client);

    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must arrive after cancel");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );

    let usage_indices: Vec<usize> = events
        .iter()
        .enumerate()
        .filter_map(|(i, event)| usage_update(event).map(|_| i))
        .collect();
    assert_eq!(
        usage_indices,
        vec![finish_index - 1],
        "an aborted run reports usage the same way, right before the finish: {events:?}"
    );
    assert_eq!(
        usage_update(&events[finish_index - 1]),
        Some((10362, 1000000)),
        "usage numbers must come from get_session_stats: {events:?}"
    );

    assert_eq!(stop_reason(&events[finish_index]), Some("cancelled"));
    assert!(matches!(
        &events[finish_index],
        AgentEvent::Native(NativeEvent::TurnFinished { request_id: finish_id, result: Ok(_) })
            if *finish_id == id
    ));
}

#[test]
fn rejected_prompt_finishes_without_usage() {
    let client = spawn_client(&["--reject-prompt"]);
    let id = client
        .core()
        .begin_prompt(&text_blocks())
        .expect("begin_prompt");

    let mut events = Vec::new();
    loop {
        let event = client
            .core()
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. }));
        events.push(event);
        if finished {
            break;
        }
    }

    assert!(
        events.iter().all(|event| usage_update(event).is_none()),
        "a rejected prompt must not request stats: {events:?}"
    );
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must arrive");
    assert_eq!(
        finish_index,
        events.len() - 1,
        "RequestFinished must be the last event: {events:?}"
    );
    assert!(matches!(
        &events[finish_index],
        AgentEvent::Native(NativeEvent::TurnFinished { request_id: finish_id, result: Err(_) })
            if *finish_id == id
    ));
}
