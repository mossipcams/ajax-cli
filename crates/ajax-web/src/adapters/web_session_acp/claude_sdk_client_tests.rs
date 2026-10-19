//! Tests for [`super::claude_sdk_client::ClaudeSdkClient`] running the real
//! `claude_sdk_sidecar.mjs` against the fake Claude Agent SDK fixture: the
//! handshake/config-option shape, the prompt and abort event flows, model and
//! effort round trips, shutdown semantics, launch-argument construction, and
//! the production embedded-script launch path (`node --input-type=module -e`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::agent_event::{AgentEvent, NativeCommand, NativeEvent};
use super::claude_sdk_client::{sidecar_launch_args, ClaudeSdkClient, EFFORT_LEVELS};

/// Generous per-step bound: node startup plus one scripted record burst.
const TIMEOUT: Duration = Duration::from_secs(10);

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// `(sidecar script, fake sdk module)` paths under the crate.
fn sidecar_and_fake_sdk() -> (PathBuf, PathBuf) {
    let dir = manifest_dir();
    (
        dir.join("sidecar/claude_sdk_sidecar.mjs"),
        dir.join("tests/fixtures/fake_claude_sdk.mjs"),
    )
}

/// Spawn the real sidecar script (`node <sidecar> --sdk-module <fake>`) wired
/// to the fake Claude Agent SDK. Fails loudly when spawn fails.
fn spawn_client(model: Option<&str>, resume: Option<&str>) -> ClaudeSdkClient {
    let (sidecar, fake_sdk) = sidecar_and_fake_sdk();
    let args = vec![
        sidecar.to_string_lossy().into_owned(),
        "--sdk-module".to_string(),
        fake_sdk.to_string_lossy().into_owned(),
    ];
    ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        model,
        resume,
        TIMEOUT,
    )
    .expect("claude sdk sidecar must spawn against the fake fixture (is node installed?)")
}

fn text_blocks(text: &str) -> String {
    text.to_string()
}

/// The streamed text of a native `AgentText` event, otherwise `None`.
fn event_text(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::Native(NativeEvent::AgentText(text)) => Some(text),
        _ => None,
    }
}

/// The `(used, size)` pair when `event` carries a native usage update,
/// otherwise `None`.
fn usage_update(event: &AgentEvent) -> Option<(u64, u64)> {
    match event {
        AgentEvent::Native(NativeEvent::Usage { used, size }) => Some((*used, *size)),
        _ => None,
    }
}

/// The stop reason when `event` is a successful native turn-finished event,
fn stop_reason(event: &AgentEvent) -> Option<&str> {
    match event {
        AgentEvent::Native(NativeEvent::TurnFinished {
            result: Ok(stop), ..
        }) => Some(stop.as_str()),
        _ => None,
    }
}

/// Collect events (with generous per-event waits) until a `RequestFinished`
/// arrives; it is kept as the last element.
fn collect_until_finished(client: &ClaudeSdkClient) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. }));
        events.push(event);
        if finished {
            return events;
        }
    }
}

/// The `(request_id, stop reason)` of a successful turn-finished event, used to
/// assert the identity and terminal shape of one prompt turn.
fn finished_fields(event: &AgentEvent) -> (u64, Option<&str>) {
    match event {
        AgentEvent::Native(NativeEvent::TurnFinished {
            request_id,
            result: Ok(stop),
        }) => (*request_id, Some(stop.as_str())),
        other => panic!("expected a TurnFinished, got {other:?}"),
    }
}

/// Owned snapshot of one select option, so callers avoid lifetimes into the
/// borrowed [`super::NeutralConfigOption`].
struct SelectView {
    current_value: String,
    values: Vec<String>,
    category: Option<super::NeutralConfigCategory>,
}

fn select_view(options: &[super::NeutralConfigOption], id: &str) -> SelectView {
    let option = options
        .iter()
        .find(|option| option.id == id)
        .unwrap_or_else(|| panic!("config option {id} must be advertised"));
    let super::NeutralConfigKind::Select {
        current_value,
        choices,
    } = &option.kind
    else {
        panic!("config option {id} must be a select");
    };
    SelectView {
        current_value: current_value.clone(),
        values: choices.iter().map(|choice| choice.value.clone()).collect(),
        category: option.category.clone(),
    }
}

#[test]
fn spawn_reports_generated_session_and_config_options() {
    let client = spawn_client(None, None);

    let session_id = client.session_id();
    assert_eq!(session_id.len(), 36, "uuid session id length");
    assert!(
        session_id.contains('-'),
        "uuid session id must contain a hyphen"
    );

    let options = client.config_options();
    assert_eq!(options.len(), 2);

    let model = select_view(&options, "model");
    assert!(
        matches!(model.category, Some(super::NeutralConfigCategory::Model)),
        "model option category: {:?}",
        model.category
    );
    assert_eq!(model.current_value, "default");
    assert_eq!(
        model.values,
        vec!["default".to_string(), "haiku".to_string()]
    );

    let effort = select_view(&options, "effort");
    assert!(
        matches!(
            effort.category,
            Some(super::NeutralConfigCategory::ThoughtLevel)
        ),
        "effort option category: {:?}",
        effort.category
    );
    assert_eq!(effort.current_value, "medium");
    assert_eq!(
        effort.values,
        EFFORT_LEVELS.map(|level| level.to_string()).to_vec()
    );
}

#[test]
fn spawn_with_model_applies_it() {
    let client = spawn_client(Some("haiku"), None);

    assert_eq!(client.applied_model(), "haiku");
    let model = select_view(&client.config_options(), "model");
    assert_eq!(model.current_value, "haiku");
}

#[test]
fn spawn_with_resume_reuses_the_session_id() {
    let client = spawn_client(None, Some("r-1"));

    assert_eq!(client.session_id(), "r-1");
}

#[test]
fn prompt_flow_streams_text_usage_and_finishes_end_turn() {
    let client = spawn_client(None, None);
    let id = client
        .begin_prompt(&text_blocks("hello"))
        .expect("begin_prompt");
    assert_eq!(id, 1);

    let events = collect_until_finished(&client);

    // Exactly one message chunk carrying "ok".
    let chunks: Vec<&str> = events.iter().filter_map(event_text).collect();
    assert_eq!(chunks, vec!["ok"], "events: {events:?}");

    // Exactly one UsageUpdate (size 200000, used > 0) before the finish.
    let usage_index = events
        .iter()
        .position(|event| usage_update(event).is_some())
        .expect("a UsageUpdate must arrive: {events:?}");
    assert!(events
        .iter()
        .skip(usage_index + 1)
        .all(|event| usage_update(event).is_none()));
    let (used, size) = usage_update(&events[usage_index]).unwrap();
    assert_eq!(size, 200000);
    assert!(used > 0, "usage.used must be non-zero: {used}");

    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must arrive");
    assert!(usage_index < finish_index);

    let (finished_id, _) = finished_fields(&events[finish_index]);
    assert_eq!(finished_id, 1);
    assert_eq!(stop_reason(&events[finish_index]), Some("end_turn"));

    assert!(!client.prompt_in_flight());
}

#[test]
fn finished_turn_emits_context_usage_from_sidecar_1334() {
    // #1334: the context meter comes from the sidecar's getContextUsage()
    // snapshot fetched by the run-finished hook, not from the result message.
    let client = spawn_client(None, None);
    client
        .begin_prompt(&text_blocks("hello"))
        .expect("begin_prompt");

    let events = collect_until_finished(&client);

    // The fake SDK fixture reports { totalTokens: 25730, maxTokens: 200000 }.
    let usage_index = events
        .iter()
        .position(|event| usage_update(event).is_some())
        .expect("a UsageUpdate must arrive before the finish: {events:?}");
    assert!(events
        .iter()
        .skip(usage_index + 1)
        .all(|event| usage_update(event).is_none()));
    let (used, size) = usage_update(&events[usage_index]).unwrap();
    assert_eq!((used, size), (25730, 200000));

    // The Usage event must precede the turn-finished event.
    let finish_index = events
        .iter()
        .rposition(|event| matches!(event, AgentEvent::Native(NativeEvent::TurnFinished { .. })))
        .expect("RequestFinished must arrive");
    assert!(usage_index < finish_index);
}

#[test]
fn abort_flow_cancels_and_reuses_the_same_host() {
    let client = spawn_client(None, None);
    let id = client
        .begin_prompt(&text_blocks("please SLOW down"))
        .expect("begin_prompt");
    assert_eq!(id, 1);

    // Wait for the first streamed chunk so the prompt is genuinely in flight.
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event during slow prompt");
        if matches!(&event, AgentEvent::Native(NativeEvent::AgentText(_))) {
            break;
        }
    }

    client.cancel().expect("cancel of an in-flight prompt");

    let events = collect_until_finished(&client);
    let last = &events[events.len() - 1];
    let (finished_id, reason) = finished_fields(last);
    assert_eq!(finished_id, 1);
    assert_eq!(reason, Some("cancelled"));

    // A second plain prompt on the same client finishes with id 2 (no respawn).
    let second_id = client
        .begin_prompt(&text_blocks("hello"))
        .expect("second begin_prompt");
    assert_eq!(second_id, 2);

    let events = collect_until_finished(&client);
    let last = &events[events.len() - 1];
    let (finished_id, reason) = finished_fields(last);
    assert_eq!(finished_id, 2);
    assert_eq!(reason, Some("end_turn"));

    assert!(!client.prompt_in_flight());
}

#[test]
fn set_model_and_set_effort_round_trip() {
    let client = spawn_client(None, None);

    client.set_model("haiku").expect("set_model");
    assert_eq!(client.applied_model(), "haiku");
    let model = select_view(&client.config_options(), "model");
    assert_eq!(model.current_value, "haiku");

    client.set_effort("high").expect("set_effort");
    let effort = select_view(&client.config_options(), "effort");
    assert_eq!(effort.current_value, "high");

    let err = client
        .set_effort("bogus")
        .expect_err("unknown effort level must fail");
    assert!(err.contains("unknown effort level"), "error text: {err}");
    let effort = select_view(&client.config_options(), "effort");
    assert_eq!(
        effort.current_value, "high",
        "failed set_effort must not change the option"
    );
}

#[test]
fn shutdown_returns_the_session_id_once_then_none() {
    let client = spawn_client(None, None);
    let session_id = client.session_id();

    assert_eq!(client.shutdown(), Some(session_id));
    assert_eq!(client.shutdown(), None);
    assert!(client.host_exited());
}

#[test]
fn busy_prompt_and_idle_cancel_are_rejected() {
    let client = spawn_client(None, None);
    let first_id = client
        .begin_prompt(&text_blocks("please SLOW down"))
        .expect("begin_prompt");
    assert_eq!(first_id, 1);

    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event during slow prompt");
        if matches!(&event, AgentEvent::Native(NativeEvent::AgentText(_))) {
            break;
        }
    }

    let err = client
        .begin_prompt(&text_blocks("again"))
        .expect_err("second prompt while one is in flight must fail");
    assert!(!err.is_empty());

    client.cancel().expect("cancel of the in-flight prompt");
    collect_until_finished(&client);

    let err = client
        .cancel()
        .expect_err("cancel with no active run must fail");
    assert!(!err.is_empty());
}

#[test]
fn sidecar_launch_args_embed_the_script_and_pass_the_sdk_module() {
    let args = sidecar_launch_args(None);
    assert_eq!(args.len(), 3);
    assert_eq!(args[0], "--input-type=module");
    assert_eq!(args[1], "-e");
    assert!(args[2].contains("resolveSdk"), "embedded script: {args:?}");

    let args = sidecar_launch_args(Some("/x"));
    assert_eq!(&args[args.len() - 3..], &["--", "--sdk-module", "/x"]);
}

#[test]
fn production_embedded_script_launch_path_prompts_to_end_turn() {
    // The production launch shape: `node <embedded script> -- --sdk-module <fake>`.
    let (_, fake_sdk) = sidecar_and_fake_sdk();
    let args = sidecar_launch_args(Some(&fake_sdk.to_string_lossy()));

    let client = ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        None,
        None,
        TIMEOUT,
    )
    .expect("sidecar must launch via the production embedded-script path (is node installed?)");

    let id = client
        .begin_prompt(&text_blocks("hello"))
        .expect("begin_prompt");

    let events = collect_until_finished(&client);
    let last = &events[events.len() - 1];
    let (finished_id, reason) = finished_fields(last);
    assert_eq!(finished_id, id);
    assert_eq!(reason, Some("end_turn"));

    assert!(!client.prompt_in_flight());
}

#[test]
fn spawn_exposes_the_advertised_command_list_once() {
    let client = spawn_client(None, None);

    assert_eq!(
        client.take_available_commands(),
        [NativeCommand {
            name: "compact".to_string(),
            description: "Compact the conversation".to_string(),
            input_hint: None,
        }],
        "the fake SDK fixture advertises exactly one command"
    );
    assert!(
        client.take_available_commands().is_empty(),
        "the second take must not re-deliver the handshake commands"
    );
}

/// Inline fake sidecar for #1275: acknowledges `init` and `prompt`, then ends
/// the turn with an API error result (`subtype: success`, `is_error: true`).
const ERROR_RESULT_SIDECAR: &str = r#"
const readline = require("node:readline");
const send = (record) => process.stdout.write(JSON.stringify(record) + "\n");
readline.createInterface({ input: process.stdin }).on("line", (line) => {
  const cmd = JSON.parse(line);
  if (cmd.type === "init") {
    send({ type: "response", id: cmd.id, success: true,
      data: { sessionId: "error-session", models: [], commands: [], initialization: {} } });
  } else if (cmd.type === "prompt") {
    send({ type: "response", id: cmd.id, success: true, data: { disposition: "started" } });
    send({ type: "result", subtype: "success", is_error: true,
      result: "API Error: overloaded", stop_reason: null });
  } else {
    send({ type: "response", id: cmd.id, success: true, data: null });
  }
});
"#;

#[test]
fn claude_error_result_finishes_prompt_as_failed_1275() {
    // #1275: a turn ended by an `is_error` result must settle the prompt as a
    // failure, not as a successful `end_turn`.
    let args = vec!["-e".to_string(), ERROR_RESULT_SIDECAR.to_string()];
    let client = ClaudeSdkClient::spawn(
        Path::new("node"),
        &args,
        manifest_dir(),
        None,
        None,
        TIMEOUT,
    )
    .expect("inline fake sidecar must spawn (is node installed?)");

    let id = client.begin_prompt("hello").expect("begin_prompt");

    let events = collect_until_finished(&client);
    let finished = events.last().expect("a turn-finished event");
    match finished {
        AgentEvent::Native(NativeEvent::TurnFinished {
            request_id,
            result: Err(message),
        }) => {
            assert_eq!(*request_id, id);
            assert!(
                message.contains("API Error: overloaded"),
                "failure must carry the provider text: {message}"
            );
        }
        other => panic!("error result must finish the prompt as failed, got {other:?}"),
    }

    assert!(!client.prompt_in_flight());
}
