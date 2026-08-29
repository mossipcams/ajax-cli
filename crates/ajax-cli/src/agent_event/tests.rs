use std::fs;

use ajax_core::canonical_agent_event::{
    AttentionReason, CanonicalEventDetail, CanonicalEventKind, TurnOutcome,
};

use crate::agent_runtime::{self, AgentRuntimeSnapshot, AgentRuntimeState};

use super::{
    resolve_cursor_identity, run_agent_event, session_start_env_stdout, translate_native_event,
    AgentEventIdentity, AgentEventOutcome,
};

fn kind(client: &str, event: &str, payload: &serde_json::Value) -> Option<CanonicalEventKind> {
    translate_native_event(client, event, payload).map(|canonical| canonical.kind)
}

fn temp_events_fixture(label: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "ajax-agent-event-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let events_dir = root.join("agent-events");
    fs::create_dir_all(&events_dir).unwrap();
    (root, events_dir)
}

fn test_identity(dir: &std::path::Path, task_id: &str) -> AgentEventIdentity {
    AgentEventIdentity {
        task_id: task_id.to_string(),
        run_id: "primary".to_string(),
        events_dir: dir.to_path_buf(),
    }
}

#[test]
fn claude_stop_with_background_tasks_stays_a_turn_start() {
    let with_tasks = serde_json::json!({"background_tasks":[{"id":1}]});
    assert_eq!(
        kind("claude", "Stop", &with_tasks),
        Some(CanonicalEventKind::TurnStarted)
    );
    let empty_tasks = serde_json::json!({"background_tasks":[]});
    assert_eq!(
        kind("claude", "Stop", &empty_tasks),
        Some(CanonicalEventKind::TurnSettled)
    );
    assert_eq!(
        kind("claude", "Stop", &serde_json::json!({})),
        Some(CanonicalEventKind::TurnSettled)
    );
}

#[test]
fn cursor_stop_error_is_a_failed_turn_settled() {
    let payload = serde_json::json!({"status":"error"});
    let canonical = translate_native_event("cursor", "stop", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::TurnSettled);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Outcome {
            outcome: TurnOutcome::Failed
        })
    );
}

#[test]
fn claude_notification_permission_vs_question() {
    let permission = serde_json::json!({
        "message": "Claude needs your permission to run Bash"
    });
    assert_eq!(
        translate_native_event("claude", "Notification", &permission)
            .and_then(|canonical| canonical.detail),
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );
    // Bare Notification arm fallback unchanged for permission-shaped messages.
    let permission_shaped = serde_json::json!({
        "message": "Claude needs your permission to run Bash"
    });
    assert_eq!(
        translate_native_event("claude", "Notification", &permission_shaped)
            .and_then(|canonical| canonical.detail),
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );
}

#[test]
fn claude_ask_user_question_pretooluse_requests_attention() {
    let ask = serde_json::json!({"tool_name": "AskUserQuestion"});
    let canonical = translate_native_event("claude", "PreToolUse", &ask).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Question
        })
    );

    let bash = serde_json::json!({"tool_name": "Bash"});
    let canonical = translate_native_event("claude", "PreToolUse", &bash).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::ActivityStarted);
}

#[test]
fn claude_ask_user_question_posttooluse_clears_attention() {
    let ask = serde_json::json!({"tool_name": "AskUserQuestion"});
    let canonical = translate_native_event("claude", "PostToolUse", &ask).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionCleared);
    assert_eq!(canonical.detail, None);

    let bash = serde_json::json!({"tool_name": "Bash"});
    let canonical = translate_native_event("claude", "PostToolUse", &bash).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::ActivityFinished);
}

#[test]
fn claude_notification_matcher_selects_phase() {
    let payload = serde_json::json!({});
    let permission =
        translate_native_event("claude", "Notification:permission_prompt", &payload).unwrap();
    assert_eq!(permission.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        permission.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );

    for event in [
        "Notification:elicitation_dialog",
        "Notification:agent_needs_input",
    ] {
        let canonical = translate_native_event("claude", event, &payload).unwrap();
        assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
        assert_eq!(
            canonical.detail,
            Some(CanonicalEventDetail::Attention {
                attention: AttentionReason::Question
            })
        );
    }

    let idle_prompt =
        translate_native_event("claude", "Notification:idle_prompt", &payload).unwrap();
    assert_eq!(idle_prompt.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        idle_prompt.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Question
        })
    );

    let agent_completed =
        translate_native_event("claude", "Notification:agent_completed", &payload).unwrap();
    assert_eq!(agent_completed.kind, CanonicalEventKind::TurnSettled);
    assert_eq!(
        agent_completed.detail,
        Some(CanonicalEventDetail::Outcome {
            outcome: TurnOutcome::Completed
        })
    );
}

#[test]
fn claude_stop_failure_settles_without_error() {
    let canonical =
        translate_native_event("claude", "StopFailure", &serde_json::json!({})).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::TurnSettled);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Outcome {
            outcome: TurnOutcome::Interrupted
        })
    );
    assert_ne!(
        canonical.detail,
        Some(CanonicalEventDetail::Outcome {
            outcome: TurnOutcome::Failed
        }),
        "Failed would project to TaskStatus::Error"
    );
}

#[test]
fn cursor_before_shell_execution_requests_permission_attention() {
    let payload = serde_json::json!({});
    let canonical = translate_native_event("cursor", "beforeShellExecution", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );
}

#[test]
fn cursor_before_mcp_execution_requests_permission_attention() {
    let payload = serde_json::json!({});
    let canonical = translate_native_event("cursor", "beforeMCPExecution", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );
}

#[test]
fn cursor_notification_permission_prompt_requests_permission_attention() {
    let payload = serde_json::json!({});
    let canonical =
        translate_native_event("cursor", "Notification:permission_prompt", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Permission
        })
    );
}

#[test]
fn cursor_notification_elicitation_dialog_requests_question_attention() {
    let payload = serde_json::json!({});
    let canonical =
        translate_native_event("cursor", "Notification:elicitation_dialog", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::AttentionRequested);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Attention {
            attention: AttentionReason::Question
        })
    );
}

#[test]
fn cursor_elicitation_result_starts_turn() {
    let payload = serde_json::json!({});
    let canonical = translate_native_event("cursor", "ElicitationResult", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::TurnStarted);
    assert_eq!(canonical.detail, None);
}

#[test]
fn cursor_post_tool_use_failure_finishes_activity() {
    let payload = serde_json::json!({"tool_call_id": "t1"});
    let canonical = translate_native_event("cursor", "postToolUseFailure", &payload).unwrap();
    assert_eq!(canonical.kind, CanonicalEventKind::ActivityFinished);
    assert_eq!(
        canonical.detail,
        Some(CanonicalEventDetail::Activity {
            activity: ajax_core::canonical_agent_event::ActivityKind::Tool,
            activity_id: Some("t1".to_string()),
        })
    );

    let subagent_start =
        translate_native_event("cursor", "subagentStart", &serde_json::json!({})).unwrap();
    assert_eq!(subagent_start.kind, CanonicalEventKind::ChildStarted);

    let subagent_stop =
        translate_native_event("cursor", "subagentStop", &serde_json::json!({})).unwrap();
    assert_eq!(subagent_stop.kind, CanonicalEventKind::ChildSettled);
}

#[test]
fn four_client_event_mappings_to_canonical_kinds() {
    let payload = serde_json::json!({});
    // Claude
    assert_eq!(
        kind("claude", "UserPromptSubmit", &payload),
        Some(CanonicalEventKind::TurnStarted)
    );
    assert_eq!(
        kind("claude", "SessionStart", &payload),
        Some(CanonicalEventKind::SessionOpened)
    );
    assert_eq!(
        kind("claude", "SessionEnd", &payload),
        Some(CanonicalEventKind::SessionClosed)
    );
    // Codex
    assert_eq!(
        kind("codex", "UserPromptSubmit", &payload),
        Some(CanonicalEventKind::TurnStarted)
    );
    assert_eq!(
        kind("codex", "Stop", &payload),
        Some(CanonicalEventKind::TurnSettled)
    );
    assert_eq!(
        kind("codex", "PermissionRequest", &payload),
        Some(CanonicalEventKind::AttentionRequested)
    );
    // Cursor
    assert_eq!(
        kind("cursor", "preToolUse", &payload),
        Some(CanonicalEventKind::ActivityStarted)
    );
    assert_eq!(
        kind("cursor", "postToolUse", &payload),
        Some(CanonicalEventKind::ActivityFinished)
    );
    assert_eq!(
        kind("cursor", "sessionStart", &payload),
        Some(CanonicalEventKind::SessionOpened)
    );
    // Pi
    assert_eq!(
        kind("pi", "before_agent_start", &payload),
        Some(CanonicalEventKind::TurnStarted)
    );
    assert_eq!(
        kind("pi", "agent_settled", &payload),
        Some(CanonicalEventKind::TurnSettled)
    );
    assert_eq!(kind("pi", "agent_end", &payload), None);
}

#[test]
fn translate_ignores_unknown_events() {
    assert_eq!(kind("nope", "stop", &serde_json::json!({})), None);
}

#[test]
fn run_agent_event_appends_jsonl_only_no_scalar_snapshot() {
    let (root, dir) = temp_events_fixture("jsonl-only");
    write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
    let identity = test_identity(&dir, "web/fix-login");

    run_agent_event(
        Some(&identity),
        "claude",
        "UserPromptSubmit",
        &serde_json::json!({}),
    )
    .unwrap();

    let stem = "web__fix-login";
    let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
    let lines: Vec<&str> = jsonl.lines().collect();
    assert_eq!(lines.len(), 1);
    let envelope: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["kind"], "turn_started");

    // The legacy scalar `{stem}.json` snapshot is no longer written.
    assert!(!dir.join(format!("{stem}.json")).exists());

    fs::remove_dir_all(root).unwrap();
}

fn write_test_runtime_snapshot(
    events_dir: &std::path::Path,
    task_id: &str,
    state: AgentRuntimeState,
    observed_at_unix_millis: u128,
) {
    let runtime_root = events_dir.parent().unwrap().join("agent-runtime");
    fs::create_dir_all(&runtime_root).unwrap();
    let snapshot = AgentRuntimeSnapshot {
        task_id: task_id.to_string(),
        state,
        observed_at_unix_millis,
        pid: Some(42),
        exit_code: None,
        message: None,
    };
    let stem = agent_runtime::task_file_stem(task_id);
    let encoded = serde_json::to_vec(&snapshot).unwrap();
    fs::write(runtime_root.join(format!("{stem}.json")), encoded).unwrap();
}

#[test]
fn run_agent_event_noop_without_identity() {
    assert!(matches!(
        run_agent_event(
            None,
            "claude",
            "Stop",
            &serde_json::json!({"background_tasks":[]}),
        ),
        Ok(AgentEventOutcome::NoIdentity)
    ));
}

#[cfg(unix)]
#[test]
fn socket_send_delivers_line_when_listener_present() {
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::set_test_notify_socket_override;

    let (root, dir) = temp_events_fixture("socket-notify");
    let socket_path = std::path::PathBuf::from(format!(
        "/tmp/ajax-notify-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let listener = UnixListener::bind(&socket_path).unwrap();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let _ = tx.send(line);
        }
    });

    set_test_notify_socket_override(Some(socket_path.clone()));

    write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
    let identity = test_identity(&dir, "web/fix-login");
    run_agent_event(
        Some(&identity),
        "claude",
        "UserPromptSubmit",
        &serde_json::json!({}),
    )
    .unwrap();

    set_test_notify_socket_override(None);

    let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let envelope: serde_json::Value = serde_json::from_str(received.trim()).unwrap();
    assert_eq!(envelope["schema_version"], 1);
    assert_eq!(envelope["kind"], "turn_started");

    let stem = "web__fix-login";
    let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
    assert_eq!(jsonl.lines().count(), 1);

    let _ = fs::remove_file(&socket_path);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_agent_event_appends_when_runtime_snapshot_running() {
    let (root, dir) = temp_events_fixture("runtime-running");
    write_test_runtime_snapshot(&dir, "web/fix-login", AgentRuntimeState::Running, 1);
    let identity = test_identity(&dir, "web/fix-login");

    run_agent_event(
        Some(&identity),
        "cursor",
        "beforeSubmitPrompt",
        &serde_json::json!({}),
    )
    .unwrap();

    let stem = "web__fix-login";
    let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
    assert_eq!(jsonl.lines().count(), 1);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_agent_event_rejects_after_stale_exit_for_non_settle_events() {
    let (root, dir) = temp_events_fixture("runtime-stale-exit");
    let stale_at = agent_runtime::now_millis().unwrap().saturating_sub(60_000);
    write_test_runtime_snapshot(
        &dir,
        "web/fix-login",
        AgentRuntimeState::ExitedSuccess,
        stale_at,
    );
    let identity = test_identity(&dir, "web/fix-login");

    assert!(matches!(
        run_agent_event(
            Some(&identity),
            "cursor",
            "preToolUse",
            &serde_json::json!({}),
        ),
        Ok(AgentEventOutcome::RejectedByRuntime)
    ));

    let stem = "web__fix-login";
    assert!(!dir.join(format!("{stem}.jsonl")).exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_agent_event_accepts_fresh_exit_for_turn_settled() {
    let (root, dir) = temp_events_fixture("runtime-fresh-exit-settle");
    write_test_runtime_snapshot(
        &dir,
        "web/fix-login",
        AgentRuntimeState::ExitedSuccess,
        agent_runtime::now_millis().unwrap(),
    );
    let identity = test_identity(&dir, "web/fix-login");

    run_agent_event(Some(&identity), "cursor", "stop", &serde_json::json!({})).unwrap();

    let stem = "web__fix-login";
    let jsonl = fs::read_to_string(dir.join(format!("{stem}.jsonl"))).unwrap();
    assert_eq!(jsonl.lines().count(), 1);
    let envelope: serde_json::Value = serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
    assert_eq!(envelope["kind"], "turn_settled");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn run_agent_event_rejects_without_runtime_snapshot() {
    let (root, dir) = temp_events_fixture("runtime-missing");
    let identity = test_identity(&dir, "web/fix-login");

    assert!(matches!(
        run_agent_event(
            Some(&identity),
            "cursor",
            "beforeSubmitPrompt",
            &serde_json::json!({}),
        ),
        Ok(AgentEventOutcome::RejectedByRuntime)
    ));

    let stem = "web__fix-login";
    assert!(!dir.join(format!("{stem}.jsonl")).exists());

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cursor_event_resolves_identity_from_cwd_index_without_ajax_env() {
    let ajax_home = std::env::temp_dir().join(format!(
        "ajax-home-cwd-index-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let events_dir = ajax_home.join("cache/agent-events");
    fs::create_dir_all(&events_dir).unwrap();
    let project_dir = ajax_home.join("worktrees/web-fix-login");
    fs::create_dir_all(&project_dir).unwrap();
    agent_runtime::publish_cwd_index(&events_dir, "web/fix-login", "primary", &project_dir)
        .unwrap();

    let identity = resolve_cursor_identity(
        &project_dir.to_string_lossy(),
        &serde_json::json!({}),
        None,
        Some(&ajax_home),
    )
    .unwrap();
    assert_eq!(identity.task_id, "web/fix-login");
    assert_eq!(identity.run_id, "primary");
    assert_eq!(identity.events_dir, events_dir.canonicalize().unwrap());

    write_test_runtime_snapshot(&events_dir, "web/fix-login", AgentRuntimeState::Running, 1);
    run_agent_event(
        Some(&identity),
        "cursor",
        "beforeSubmitPrompt",
        &serde_json::json!({}),
    )
    .unwrap();

    let stem = "web__fix-login";
    let jsonl = fs::read_to_string(events_dir.join(format!("{stem}.jsonl"))).unwrap();
    assert_eq!(jsonl.lines().count(), 1);

    fs::remove_dir_all(ajax_home).unwrap();
}

#[test]
fn cursor_resolves_identity_from_xdg_cache_ajax_without_ajax_home() {
    let home = std::env::temp_dir().join(format!(
        "ajax-xdg-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let events_dir = home.join(".cache/ajax/agent-events");
    fs::create_dir_all(&events_dir).unwrap();
    let project_dir = home.join("worktrees/web-fix-login");
    fs::create_dir_all(&project_dir).unwrap();
    agent_runtime::publish_cwd_index(&events_dir, "web/fix-login", "primary", &project_dir)
        .unwrap();

    let identity = resolve_cursor_identity(
        &project_dir.to_string_lossy(),
        &serde_json::json!({}),
        Some(&home),
        None,
    )
    .expect("stable XDG ~/.cache/ajax must resolve without AJAX_HOME");
    assert_eq!(identity.task_id, "web/fix-login");
    assert_eq!(identity.events_dir, events_dir.canonicalize().unwrap());

    fs::remove_dir_all(home).unwrap();
}

#[test]
fn cursor_session_start_stdout_includes_session_env() {
    let (_, events_dir) = temp_events_fixture("session-start-env");
    let identity = test_identity(&events_dir, "web/fix-login");
    let stdout = session_start_env_stdout(&identity);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["env"]["AJAX_TASK_ID"], "web/fix-login");
    assert_eq!(parsed["env"]["AJAX_RUN_ID"], "primary");
    assert_eq!(
        parsed["env"]["AJAX_AGENT_EVENTS_DIR"].as_str().unwrap(),
        events_dir.to_string_lossy()
    );
}

#[test]
fn cursor_without_index_still_noops() {
    let ajax_home = std::env::temp_dir().join(format!(
        "ajax-home-missing-index-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let events_dir = ajax_home.join("cache/agent-events");
    fs::create_dir_all(&events_dir).unwrap();
    let project_dir = ajax_home.join("worktrees/web-fix-login");
    fs::create_dir_all(&project_dir).unwrap();

    assert!(resolve_cursor_identity(
        &project_dir.to_string_lossy(),
        &serde_json::json!({}),
        None,
        Some(&ajax_home),
    )
    .is_none());

    write_test_runtime_snapshot(&events_dir, "web/fix-login", AgentRuntimeState::Running, 1);
    assert!(matches!(
        run_agent_event(None, "cursor", "beforeSubmitPrompt", &serde_json::json!({})),
        Ok(AgentEventOutcome::NoIdentity)
    ));

    let stem = "web__fix-login";
    assert!(!events_dir.join(format!("{stem}.jsonl")).exists());

    fs::remove_dir_all(ajax_home).unwrap();
}
