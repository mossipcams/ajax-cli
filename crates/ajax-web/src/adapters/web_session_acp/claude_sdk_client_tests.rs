//! Tests for [`super::claude_sdk_client::ClaudeSdkClient`] running the real
//! `claude_sdk_sidecar.mjs` against the fake Claude Agent SDK fixture: the
//! handshake/config-option shape, the prompt and abort event flows, model and
//! effort round trips, shutdown semantics, launch-argument construction, and
//! the production embedded-script launch path (`node --input-type=module -e`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionUpdate, TextContent,
};

use super::claude_sdk_client::{sidecar_launch_args, ClaudeSdkClient, EFFORT_LEVELS};
use super::client::AcpClientEvent;

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

fn text_blocks(text: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(text))]
}

/// The streamed text of an `AgentMessageChunk` update, otherwise `None`.
fn chunk_text(update: &SessionUpdate) -> Option<&str> {
    match update {
        SessionUpdate::AgentMessageChunk(chunk) => match &chunk.content {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// The `(used, size)` pair when `event` is a `SessionUpdate` carrying a
/// `UsageUpdate`, otherwise `None`.
fn usage_update(event: &AcpClientEvent) -> Option<(u64, u64)> {
    match event {
        AcpClientEvent::SessionUpdate(notification) => match &notification.update {
            SessionUpdate::UsageUpdate(update) => Some((update.used, update.size)),
            _ => None,
        },
        _ => None,
    }
}

/// The stop reason when `event` is a successful `RequestFinished`, otherwise
/// `None`.
fn stop_reason(event: &AcpClientEvent) -> Option<&str> {
    match event {
        AcpClientEvent::RequestFinished {
            result: Ok(value), ..
        } => value.get("stopReason").and_then(|s| s.as_str()),
        _ => None,
    }
}

/// Collect events (with generous per-event waits) until a `RequestFinished`
/// arrives; it is kept as the last element.
fn collect_until_finished(client: &ClaudeSdkClient) -> Vec<AcpClientEvent> {
    let mut events = Vec::new();
    loop {
        let event = client
            .wait_event(TIMEOUT)
            .expect("event before RequestFinished");
        let finished = matches!(event, AcpClientEvent::RequestFinished { .. });
        events.push(event);
        if finished {
            return events;
        }
    }
}

/// The `(id, method, stop reason)` of a successful `RequestFinished`, used to
/// assert the identity and terminal shape of one prompt turn.
fn finished_fields(event: &AcpClientEvent) -> (u64, &str, Option<&str>) {
    match event {
        AcpClientEvent::RequestFinished {
            id,
            method,
            result: _,
        } => (*id, method, stop_reason(event)),
        other => panic!("expected a RequestFinished, got {other:?}"),
    }
}

/// Owned snapshot of one select option, so callers avoid lifetimes into the
/// borrowed `SessionConfigOption`.
struct SelectView {
    current_value: String,
    values: Vec<String>,
    category: Option<SessionConfigOptionCategory>,
}

fn select_view(options: &[SessionConfigOption], id: &str) -> SelectView {
    let option = options
        .iter()
        .find(|option| option.id.0.as_ref() == id)
        .unwrap_or_else(|| panic!("config option {id} must be advertised"));
    let SessionConfigKind::Select(select) = &option.kind else {
        panic!("config option {id} must be a select");
    };
    let entries: &[agent_client_protocol::schema::v1::SessionConfigSelectOption] =
        match &select.options {
            SessionConfigSelectOptions::Ungrouped(entries) => entries,
            _ => panic!("config option {id} must be ungrouped"),
        };
    SelectView {
        current_value: select.current_value.0.to_string(),
        values: entries
            .iter()
            .map(|entry| entry.value.0.to_string())
            .collect(),
        category: option.category.clone(),
    }
}

#[test]
fn spawn_reports_generated_session_and_config_options() {
    let client = spawn_client(None, None);

    let session_id = client.session_id();
    assert_eq!(
        session_id.len(),
        36,
        "uuid session id shape: {session_id:?}"
    );
    assert!(
        session_id.contains('-'),
        "uuid session id shape: {session_id:?}"
    );

    let options = client.config_options();
    assert_eq!(options.len(), 2);

    let model = select_view(&options, "model");
    assert!(
        matches!(model.category, Some(SessionConfigOptionCategory::Model)),
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
            Some(SessionConfigOptionCategory::ThoughtLevel)
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
    let chunks: Vec<&str> = events
        .iter()
        .filter_map(|event| match event {
            AcpClientEvent::SessionUpdate(notification) => chunk_text(&notification.update),
            _ => None,
        })
        .collect();
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
        .rposition(|event| matches!(event, AcpClientEvent::RequestFinished { .. }))
        .expect("RequestFinished must arrive");
    assert!(usage_index < finish_index);

    let (finished_id, method, reason) = finished_fields(&events[finish_index]);
    assert_eq!(finished_id, 1);
    assert_eq!(method, "session/prompt");
    assert_eq!(reason, Some("end_turn"));

    assert!(!client.prompt_in_flight());
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
        if matches!(
            &event,
            AcpClientEvent::SessionUpdate(notification)
                if matches!(&notification.update, SessionUpdate::AgentMessageChunk(_))
        ) {
            break;
        }
    }

    client.cancel().expect("cancel of an in-flight prompt");

    let events = collect_until_finished(&client);
    let last = &events[events.len() - 1];
    let (finished_id, method, reason) = finished_fields(last);
    assert_eq!(finished_id, 1);
    assert_eq!(method, "session/prompt");
    assert_eq!(reason, Some("cancelled"));

    // A second plain prompt on the same client finishes with id 2 (no respawn).
    let second_id = client
        .begin_prompt(&text_blocks("hello"))
        .expect("second begin_prompt");
    assert_eq!(second_id, 2);

    let events = collect_until_finished(&client);
    let last = &events[events.len() - 1];
    let (finished_id, method, reason) = finished_fields(last);
    assert_eq!(finished_id, 2);
    assert_eq!(method, "session/prompt");
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
        if matches!(
            &event,
            AcpClientEvent::SessionUpdate(notification)
                if matches!(&notification.update, SessionUpdate::AgentMessageChunk(_))
        ) {
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
    let (finished_id, method, reason) = finished_fields(last);
    assert_eq!(finished_id, id);
    assert_eq!(method, "session/prompt");
    assert_eq!(reason, Some("end_turn"));

    assert!(!client.prompt_in_flight());
}
