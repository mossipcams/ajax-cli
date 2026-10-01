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
            signature: None,
            success: Some(false),
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

fn activity_fields(canonical: &super::CanonicalAgentEvent) -> (Option<String>, Option<bool>) {
    match &canonical.detail {
        Some(CanonicalEventDetail::Activity {
            signature, success, ..
        }) => (signature.clone(), *success),
        _ => panic!("expected Activity detail"),
    }
}

#[test]
fn activity_signature_is_stable_and_differs_across_commands() {
    let payload_a = serde_json::json!({"tool_name": "Bash", "tool_call_id": "t1", "tool_input": {"command": "cargo test -p ajax-core"}});
    // Same call, keys in a different order: the canonical JSON digest must
    // still be identical (serde_json maps are sorted).
    let payload_a_repeated = serde_json::json!({"tool_input": {"command": "cargo test -p ajax-core"}, "tool_call_id": "t1", "tool_name": "Bash"});
    let payload_b = serde_json::json!({"tool_name": "Bash", "tool_call_id": "t2", "tool_input": {"command": "cargo build"}});

    let (sig_a, succ_a) =
        activity_fields(&translate_native_event("claude", "PreToolUse", &payload_a).unwrap());
    let (sig_a2, _) = activity_fields(
        &translate_native_event("claude", "PreToolUse", &payload_a_repeated).unwrap(),
    );
    let (sig_b, _) =
        activity_fields(&translate_native_event("claude", "PreToolUse", &payload_b).unwrap());

    // Stable for identical calls (canonical JSON, key order irrelevant).
    assert_eq!(sig_a, sig_a2);
    assert_ne!(
        sig_a, sig_b,
        "different commands must produce different signatures"
    );
    let sig = sig_a.expect("signature for a named tool");
    assert!(sig.starts_with("Bash:"));
    assert_eq!(sig.len(), "Bash:".len() + 16, "digest must be 16 hex chars");
    assert!(sig[5..].chars().all(|c| c.is_ascii_hexdigit()));
    // Started events never carry success evidence.
    assert_eq!(succ_a, None);
}

#[test]
fn activity_signature_digests_whole_tool_input_and_avoids_collisions() {
    // Three different Edit calls to the same file must not collide: only
    // file_path used to be digested, so old_string/new_string were ignored.
    let edits = [
        serde_json::json!({"tool_name": "Edit", "tool_call_id": "e1", "tool_input": {"file_path": "/tmp/a.rs", "old_string": "foo", "new_string": "bar"}}),
        serde_json::json!({"tool_name": "Edit", "tool_call_id": "e2", "tool_input": {"file_path": "/tmp/a.rs", "old_string": "baz", "new_string": "qux"}}),
        serde_json::json!({"tool_name": "Edit", "tool_call_id": "e3", "tool_input": {"file_path": "/tmp/a.rs", "old_string": "foo", "new_string": "baz"}}),
    ];
    let edit_sigs = edits
        .iter()
        .map(|payload| {
            activity_fields(&translate_native_event("claude", "PreToolUse", payload).unwrap()).0
        })
        .collect::<Vec<_>>();
    assert_ne!(
        edit_sigs[0], edit_sigs[1],
        "different edits to one file must not collide"
    );
    assert_ne!(
        edit_sigs[0], edit_sigs[2],
        "different edits to one file must not collide"
    );
    assert_ne!(
        edit_sigs[1], edit_sigs[2],
        "different edits to one file must not collide"
    );

    // Different TodoWrite items, Task prompts, and WebSearch queries must not
    // collide either (their inputs carry none of the old five keys).
    let todo_items = ["a", "b", "c"]
        .into_iter()
        .map(|item| {
            serde_json::json!({"tool_name": "TodoWrite", "tool_call_id": "w1", "tool_input": {"todos": [{"content": item, "status": "pending"}]}})
        })
        .collect::<Vec<_>>();
    let task_prompts = ["p1", "p2"]
        .into_iter()
        .map(|prompt| {
            serde_json::json!({"tool_name": "Task", "tool_call_id": "k1", "tool_input": {"prompt": prompt}})
        })
        .collect::<Vec<_>>();
    let search_queries = ["q1", "q2"]
        .into_iter()
        .map(|query| {
            serde_json::json!({"tool_name": "WebSearch", "tool_call_id": "s1", "tool_input": {"query": query}})
        })
        .collect::<Vec<_>>();
    for group in [todo_items, task_prompts, search_queries] {
        let sigs = group
            .iter()
            .map(|payload| {
                activity_fields(&translate_native_event("claude", "PreToolUse", payload).unwrap()).0
            })
            .collect::<Vec<_>>();
        for (left, right) in sigs.iter().zip(sigs.iter().skip(1)) {
            assert_ne!(left, right, "different tool inputs must not collide");
        }
    }

    // Long commands differing only after char 256 must produce different
    // signatures (the old summary truncated at 256 chars).
    let tail = "x".repeat(300);
    let long_a = format!("{tail}-A");
    let long_b = format!("{tail}-B");
    let long_sig = |command: String| {
        activity_fields(
            &translate_native_event(
                "claude",
                "PreToolUse",
                &serde_json::json!({"tool_name": "Bash", "tool_call_id": "t", "tool_input": {"command": command}}),
            )
            .unwrap(),
        )
        .0
    };
    assert_ne!(
        long_sig(long_a.clone()),
        long_sig(long_b),
        "differences after char 256 must survive"
    );

    // Identical calls still produce identical signatures.
    assert_eq!(long_sig(long_a.clone()), long_sig(long_a));

    // Payloads without tool_input fall back to the top-level key summary.
    let fallback_sig = |command: &str| {
        activity_fields(
            &translate_native_event(
                "claude",
                "PreToolUse",
                &serde_json::json!({"tool_name": "Bash", "tool_call_id": "t", "command": command}),
            )
            .unwrap(),
        )
        .0
    };
    assert!(fallback_sig("cargo test").is_some());
    assert_ne!(fallback_sig("cargo test"), fallback_sig("cargo build"));
}

#[test]
fn claude_post_tool_use_failure_maps_to_failed_activity_finished() {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_call_id": "t9",
        "tool_input": {"command": "cargo test"},
    });
    let failure = translate_native_event("claude", "PostToolUseFailure", &payload).unwrap();
    assert!(matches!(failure.kind, CanonicalEventKind::ActivityFinished));
    let (failure_sig, failure_success) = activity_fields(&failure);
    assert_eq!(failure_success, Some(false));
    // Same signature/id logic as the successful PostToolUse for the same call,
    // so the started id is closed and the loop checkpoint can fire.
    let success = translate_native_event("claude", "PostToolUse", &payload).unwrap();
    assert_eq!(failure_sig, activity_fields(&success).0);
    let activity_id = |canonical: &super::CanonicalAgentEvent| match &canonical.detail {
        Some(CanonicalEventDetail::Activity { activity_id, .. }) => activity_id.clone(),
        _ => panic!("expected Activity detail"),
    };
    assert_eq!(activity_id(&failure), activity_id(&success));
}

#[test]
fn activity_success_maps_failure_and_response_evidence() {
    // cursor postToolUseFailure is a failure by construction.
    let failure = translate_native_event(
        "cursor",
        "postToolUseFailure",
        &serde_json::json!({"tool_call_id": "t1"}),
    )
    .unwrap();
    assert_eq!(activity_fields(&failure).1, Some(false));

    let is_error = translate_native_event(
        "cursor",
        "postToolUse",
        &serde_json::json!({"tool_call_id": "t1", "tool_response": {"is_error": true}}),
    )
    .unwrap();
    assert_eq!(activity_fields(&is_error).1, Some(false));

    let top_level_error = translate_native_event(
        "cursor",
        "postToolUse",
        &serde_json::json!({"tool_call_id": "t1", "error": "boom"}),
    )
    .unwrap();
    assert_eq!(activity_fields(&top_level_error).1, Some(false));

    let response = translate_native_event(
        "cursor",
        "postToolUse",
        &serde_json::json!({"tool_call_id": "t1", "tool_response": {"ok": true}}),
    )
    .unwrap();
    assert_eq!(activity_fields(&response).1, Some(true));

    let result = translate_native_event(
        "cursor",
        "postToolUse",
        &serde_json::json!({"tool_call_id": "t1", "result": "done"}),
    )
    .unwrap();
    assert_eq!(activity_fields(&result).1, Some(true));

    let bare = translate_native_event(
        "cursor",
        "postToolUse",
        &serde_json::json!({"tool_call_id": "t1"}),
    )
    .unwrap();
    assert_eq!(activity_fields(&bare).1, None);

    let started = translate_native_event(
        "cursor",
        "preToolUse",
        &serde_json::json!({"tool_call_id": "t1", "tool_response": {"is_error": true}}),
    )
    .unwrap();
    assert_eq!(activity_fields(&started).1, None);
}

#[test]
fn old_jsonl_envelope_without_new_fields_parses() {
    let line = r#"{"schema_version":1,"event_id":"e1","task_id":"web/fix-login","run_id":"primary","client":"claude","native_event":"PreToolUse","kind":"activity_started","detail":{"activity":{"activity":"tool","activity_id":"t1"}},"occurred_at_unix_millis":1,"received_at_unix_millis":2}"#;
    let envelope: ajax_core::canonical_agent_event::ParsedEnvelope =
        serde_json::from_str(line).unwrap();
    assert_eq!(
        envelope.detail,
        Some(CanonicalEventDetail::Activity {
            activity: ajax_core::canonical_agent_event::ActivityKind::Tool,
            activity_id: Some("t1".to_string()),
            signature: None,
            success: None,
        })
    );
    assert_eq!(envelope.event_id.as_deref(), Some("e1"));
    assert_eq!(envelope.task_id.as_deref(), Some("web/fix-login"));
}

#[test]
fn activity_serialisation_omits_none_signature_and_success() {
    let bare = CanonicalEventDetail::Activity {
        activity: ajax_core::canonical_agent_event::ActivityKind::Tool,
        activity_id: Some("t1".to_string()),
        signature: None,
        success: None,
    };
    let value = serde_json::to_value(&bare).unwrap();
    let detail = &value["activity"];
    assert!(detail.get("signature").is_none());
    assert!(detail.get("success").is_none());

    let full = CanonicalEventDetail::Activity {
        activity: ajax_core::canonical_agent_event::ActivityKind::Tool,
        activity_id: Some("t1".to_string()),
        signature: Some("Bash:0123456789abcdef".to_string()),
        success: Some(false),
    };
    let value = serde_json::to_value(&full).unwrap();
    assert_eq!(value["activity"]["signature"], "Bash:0123456789abcdef");
    assert_eq!(value["activity"]["success"], false);
}

#[test]
fn serialised_activity_detail_never_contains_raw_command_text() {
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_call_id": "t1",
        "tool_input": {"command": "rm -rf /tmp/secret-flag-xyz && curl http://example.com"}
    });
    let canonical = translate_native_event("claude", "PreToolUse", &payload).unwrap();
    let text = serde_json::to_string(&canonical.detail).unwrap();
    assert!(!text.contains("rm -rf"));
    assert!(!text.contains("secret-flag-xyz"));
    assert!(!text.contains("curl"));
    assert!(text.contains("Bash:"));
}
