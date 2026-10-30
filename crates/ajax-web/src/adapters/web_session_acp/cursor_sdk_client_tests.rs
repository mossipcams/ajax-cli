use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::agent_event::{AgentEvent, NativeEvent};
use super::cursor_sdk_client::{sidecar_launch_args, CursorSdkClient};

const TIMEOUT: Duration = Duration::from_secs(10);

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ajax-cursor-sdk-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn spawn_in(
    cwd: &Path,
    model: Option<&str>,
    resume: Option<&str>,
) -> Result<CursorSdkClient, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let args = vec![
        dir.join("sidecar/cursor_sdk_sidecar.mjs")
            .to_string_lossy()
            .into_owned(),
        "--sdk-module".to_string(),
        dir.join("tests/fixtures/fake_cursor_sdk.mjs")
            .to_string_lossy()
            .into_owned(),
    ];
    CursorSdkClient::spawn(Path::new("node"), &args, cwd, model, resume, TIMEOUT)
}

fn drain_turn(client: &CursorSdkClient) -> Vec<NativeEvent> {
    let started = Instant::now();
    let mut events = Vec::new();
    while started.elapsed() < TIMEOUT {
        match client.core().wait_event(Duration::from_millis(50)) {
            Some(AgentEvent::Native(event)) => {
                let done = matches!(event, NativeEvent::TurnFinished { .. });
                events.push(event);
                if done {
                    break;
                }
            }
            Some(_) => {}
            None => {}
        }
    }
    events
}

#[test]
fn spawn_reports_session_id_and_default_model() {
    let cwd = scratch("default");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    assert_eq!(client.core().session_id(), "agent-fake-1");
    assert_eq!(client.applied_model(), "default");
    client.core().shutdown();
}

#[test]
fn spawn_with_encoded_model_reaches_the_sdk() {
    let cwd = scratch("encoded-model");
    let client =
        spawn_in(&cwd, Some("grok-4.6|effort=low"), None).expect("spawn with encoded model");
    assert_eq!(client.applied_model(), "grok-4.6|effort=low");
    client.core().begin_prompt("hi").expect("begin prompt");
    let events = drain_turn(&client);
    let joined = format!("{events:?}");
    assert!(joined.contains("echo[grok-4.6]: hi"), "events: {joined}");
    client.core().shutdown();
}

#[test]
fn prompt_streams_thought_text_and_tool_call_then_finishes() {
    let cwd = scratch("stream");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client.core().begin_prompt("hi").expect("begin prompt");
    let events = drain_turn(&client);

    assert!(
        events
            .iter()
            .any(|event| matches!(event, NativeEvent::AgentThought { .. })),
        "no AgentThought in {events:?}"
    );
    let joined = format!("{events:?}");
    assert!(joined.contains("echo[default]: hi"), "events: {joined}");
    assert!(
        joined.contains("call-1\\nfc_1"),
        "tool call id missing in {joined}"
    );
    assert!(
        joined.contains("\"completed\""),
        "no completed tool update in {joined}"
    );

    match events.last().expect("drained at least one event") {
        NativeEvent::TurnFinished { result, .. } => {
            assert!(result.is_ok(), "turn finished with error: {result:?}");
        }
        other => panic!("last event is not TurnFinished: {other:?}"),
    }
    assert!(!client.core().prompt_in_flight());
    client.core().shutdown();
}

#[test]
fn failing_prompt_surfaces_the_error() {
    let cwd = scratch("failing");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client
        .core()
        .begin_prompt("FAIL please")
        .expect("begin prompt");
    let events = drain_turn(&client);

    let surfaced = events.iter().any(|event| match event {
        NativeEvent::Error(text) => text.contains("out of usage"),
        NativeEvent::TurnFinished { result, .. } => {
            matches!(result, Err(text) if text.contains("out of usage"))
        }
        _ => false,
    });
    assert!(surfaced, "no out-of-usage error in {events:?}");
    assert!(!client.core().prompt_in_flight());
    client.core().shutdown();
}

#[test]
fn cancel_settles_the_turn_and_the_next_prompt_works() {
    let cwd = scratch("cancel");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client.core().begin_prompt("SLOW").expect("begin prompt");
    std::thread::sleep(Duration::from_millis(200));
    client.core().cancel().expect("cancel prompt");
    let events = drain_turn(&client);
    assert!(
        matches!(events.last(), Some(NativeEvent::TurnFinished { .. })),
        "no TurnFinished after cancel: {events:?}"
    );

    client
        .core()
        .begin_prompt("again")
        .expect("begin second prompt");
    let events = drain_turn(&client);
    let joined = format!("{events:?}");
    assert!(joined.contains("again"), "second turn events: {joined}");
    client.core().shutdown();
}

#[test]
fn commands_are_one_shot() {
    let cwd = scratch("commands");
    std::fs::create_dir_all(cwd.join(".cursor/commands")).expect("create .cursor/commands");
    std::fs::write(cwd.join(".cursor/commands/greet.md"), "# Greet\nSay hi\n")
        .expect("write command file");

    let client = spawn_in(&cwd, None, None).expect("spawn with commands");
    let first = client.core().take_available_commands();
    assert!(
        first.iter().any(|command| command.name == "greet"),
        "no greet command in {first:?}"
    );
    assert!(
        client.core().take_available_commands().is_empty(),
        "commands not one-shot"
    );
    client.core().shutdown();
}

#[test]
fn resume_of_a_missing_agent_fails_closed() {
    let cwd = scratch("resume-missing");
    match spawn_in(&cwd, None, Some("agent-missing")) {
        Ok(_) => panic!("resume of missing agent must fail"),
        Err(err) => assert!(err.contains("not found"), "error text: {err}"),
    }
}

#[test]
fn resume_returns_the_requested_agent() {
    let cwd = scratch("resume-other");
    let client = spawn_in(&cwd, None, Some("agent-other")).expect("resume existing agent");
    assert_eq!(client.core().session_id(), "agent-other");
    client.core().shutdown();
}

#[test]
fn launch_args_carry_the_sdk_module() {
    let args = sidecar_launch_args(Some("/x/sdk.mjs"));
    assert_eq!(
        args.first().map(String::as_str),
        Some("--input-type=module")
    );
    let tail = &args[args.len() - 3..];
    assert_eq!(tail, ["--", "--sdk-module", "/x/sdk.mjs"]);
}

fn option_ids(client: &CursorSdkClient) -> Vec<String> {
    client.config_options().into_iter().map(|o| o.id).collect()
}

fn select_view(client: &CursorSdkClient, id: &str) -> Option<(String, Vec<(String, String)>)> {
    let option = client.config_options().into_iter().find(|o| o.id == id)?;
    match option.kind {
        super::NeutralConfigKind::Select {
            current_value,
            choices,
        } => Some((
            current_value,
            choices.into_iter().map(|c| (c.value, c.name)).collect(),
        )),
        _ => None,
    }
}

#[test]
fn config_options_for_the_default_model() {
    let cwd = scratch("config-default");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    assert_eq!(option_ids(&client), ["model", "mode"]);
    let (current, choices) = select_view(&client, "model").expect("model select");
    assert_eq!(current, "default");
    assert_eq!(choices.len(), 2);
    assert_eq!(
        select_view(&client, "mode").map(|(c, _)| c),
        Some("agent".to_string())
    );
    client.core().shutdown();
}

#[test]
fn set_model_takes_default_parameters() {
    let cwd = scratch("set-model-defaults");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client.set_model("grok-4.6").expect("set model");
    assert_eq!(client.applied_model(), "grok-4.6|effort=high");
    assert_eq!(option_ids(&client), ["model", "effort", "mode"]);
    let (current, _) = select_view(&client, "effort").expect("effort select");
    assert_eq!(current, "high");
    client.core().begin_prompt("hi").expect("begin prompt");
    let events = drain_turn(&client);
    let joined = format!("{events:?}");
    assert!(joined.contains("echo[grok-4.6]: hi"), "events: {joined}");
    client.core().shutdown();
}

#[test]
fn set_option_updates_the_selection() {
    let cwd = scratch("set-option");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client.set_model("grok-4.6").expect("set model");
    client.set_option("effort", "low").expect("set effort low");
    assert_eq!(client.applied_model(), "grok-4.6|effort=low");
    let err = client
        .set_option("effort", "bogus")
        .expect_err("bogus value must fail");
    assert!(err.contains("unknown effort value"), "error: {err}");
    let err = client
        .set_option("nope", "x")
        .expect_err("unknown option must fail");
    assert!(err.contains("unsupported Cursor option"), "error: {err}");
    client.core().shutdown();
}

#[test]
fn set_model_rejects_an_unknown_model() {
    let cwd = scratch("set-model-unknown");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    let err = client
        .set_model("nope")
        .expect_err("unknown model must fail");
    assert!(err.contains("unknown model"), "error: {err}");
    client.core().shutdown();
}

#[test]
fn spawn_with_a_bare_parameterised_model_applies_defaults() {
    let cwd = scratch("bare-parameterised");
    let client = spawn_in(&cwd, Some("grok-4.6"), None).expect("spawn bare grok-4.6");
    assert_eq!(client.applied_model(), "grok-4.6|effort=high");
    client.core().shutdown();
}

#[test]
fn set_mode_reaches_the_next_prompt() {
    let cwd = scratch("set-mode");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    client.set_mode("plan").expect("set mode plan");
    client.core().begin_prompt("hi").expect("begin prompt");
    let events = drain_turn(&client);
    let joined = format!("{events:?}");
    assert!(joined.contains("mode=plan"), "events: {joined}");
    let err = client.set_mode("bogus").expect_err("bogus mode must fail");
    assert!(err.contains("unknown mode"), "error: {err}");
    client.core().shutdown();
}

#[test]
fn model_catalog_maps_default_to_auto_and_exposes_axes() {
    let cwd = scratch("catalog");
    let client = spawn_in(&cwd, None, None).expect("spawn default session");
    let catalog = client.model_catalog();

    let auto = catalog
        .iter()
        .find(|model| model.id == "auto")
        .expect("auto entry");
    assert_eq!(auto.label, "Auto");
    assert!(auto.efforts.is_empty());
    assert!(!auto.has_fast);

    let grok = catalog
        .iter()
        .find(|model| model.id == "grok-4.6")
        .expect("grok entry");
    assert_eq!(grok.efforts, ["low".to_string(), "high".to_string()]);
    assert!(!grok.has_fast);

    client.core().shutdown();
}
