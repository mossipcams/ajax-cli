use super::*;
use ajax_core::models::AgentClient;
use std::collections::VecDeque;

#[test]
fn prepare_task_session_returns_worktree_for_cursor_task() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Cursor;
    task.set_skip_interactive_agent(true);
    let worktree = std::env::temp_dir().join("ajax-web-session-test-fix-login");
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    task.worktree_path = worktree;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let plan = prepare_task_session(&context, "web/fix-login", "auto").expect("plan");
    assert_eq!(plan.qualified_handle, "web/fix-login");
    assert_eq!(plan.model, "auto");
    assert!(plan
        .worktree_path
        .ends_with("ajax-web-session-test-fix-login"));
}

#[test]
fn prepare_task_session_rejects_interactive_cursor_without_skip_bit() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Cursor;
    let worktree = std::env::temp_dir().join("ajax-web-session-test-interactive-cursor");
    let _ = std::fs::remove_dir_all(&worktree);
    std::fs::create_dir_all(&worktree).expect("worktree dir");
    task.worktree_path = worktree;
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let error = prepare_task_session(&context, "web/fix-login", "auto").unwrap_err();
    assert_eq!(error, SessionRouteError::NotOrchestrationChat);
}

#[test]
fn prepare_task_session_admits_every_provisioned_acp_harness() {
    for (agent, label) in [
        (AgentClient::Codex, "codex"),
        (AgentClient::Claude, "claude"),
        (AgentClient::Pi, "pi"),
    ] {
        let mut task = crate::test_support::fix_login_task();
        task.selected_agent = agent;
        task.set_skip_interactive_agent(true);
        let worktree = std::env::temp_dir().join(format!("ajax-web-session-test-{label}"));
        let _ = std::fs::remove_dir_all(&worktree);
        std::fs::create_dir_all(&worktree).expect("worktree dir");
        task.worktree_path = worktree;
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let plan = prepare_task_session(&context, "web/fix-login", "auto").expect("plan");
        assert_eq!(plan.agent, agent, "{label} should attach to its own ACP");
    }
}

#[test]
fn prepare_task_session_rejects_agent_without_acp() {
    let mut task = crate::test_support::fix_login_task();
    task.selected_agent = AgentClient::Other;
    task.set_skip_interactive_agent(true);
    let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
    let error = prepare_task_session(&context, "web/fix-login", "auto").unwrap_err();
    assert_eq!(error, SessionRouteError::NotOrchestrationChat);
}

#[test]
fn normalize_session_model_defaults_and_rejects_junk() {
    assert_eq!(normalize_session_model("").unwrap(), "auto");
    assert_eq!(normalize_session_model("  ").unwrap(), "auto");
    assert_eq!(
        normalize_session_model("composer-2.5").unwrap(),
        "composer-2.5"
    );
    assert_eq!(
        normalize_session_model("claude-opus-4-8[context=1m,effort=high,fast=false]").unwrap(),
        "claude-opus-4-8[context=1m,effort=high,fast=false]"
    );
    assert!(normalize_session_model("bad model").is_err());
    assert!(normalize_session_model(&"x".repeat(129)).is_err());
}

#[test]
fn map_tool_call_to_structured_event_not_raw_json() {
    let update = serde_json::json!({
        "sessionId": "sess_1",
        "update": {
            "sessionUpdate": "tool_call",
            "toolCallId": "call_001",
            "title": "Read configuration",
            "kind": "read",
            "status": "pending",
            "locations": [{ "path": "/repo/config.json" }]
        }
    });
    assert_eq!(
        map_acp_session_update(&update),
        vec![SessionServerEvent::ToolCall {
            call_id: "call_001".to_string(),
            title: "Read configuration".to_string(),
            kind: "read".to_string(),
            status: "pending".to_string(),
            locations: vec!["/repo/config.json".to_string()],
        }]
    );
}

#[test]
fn map_tool_call_update_keeps_call_id_when_title_absent() {
    let update = serde_json::json!({
        "update": {
            "sessionUpdate": "tool_call_update",
            "toolCallId": "call_001",
            "status": "completed"
        }
    });
    let events = map_acp_session_update(&update);
    let SessionServerEvent::ToolCall {
        call_id,
        title,
        status,
        ..
    } = &events[0]
    else {
        panic!("expected tool call, got {events:?}");
    };
    assert_eq!(call_id, "call_001");
    assert_eq!(title, "");
    assert_eq!(status, "completed");
}

#[test]
fn map_tool_call_without_id_is_dropped() {
    let update = serde_json::json!({
        "update": { "sessionUpdate": "tool_call", "title": "Nameless" }
    });
    assert!(map_acp_session_update(&update).is_empty());
}

#[test]
fn map_thought_uses_its_own_role_so_chat_can_separate_reasoning() {
    let update = serde_json::json!({
        "update": {
            "sessionUpdate": "thought_chunk",
            "content": { "type": "text", "text": "Checking the router" }
        }
    });
    assert_eq!(
        map_acp_session_update(&update),
        vec![SessionServerEvent::Message {
            role: "thought".to_string(),
            text: "Checking the router".to_string(),
        }]
    );
}

#[test]
fn map_plan_to_structured_entries() {
    let update = serde_json::json!({
        "update": {
            "sessionUpdate": "plan",
            "entries": [
                { "content": "Read the router", "status": "completed" },
                { "content": "Patch the guard", "status": "in_progress" },
                { "content": "   ", "status": "pending" }
            ]
        }
    });
    assert_eq!(
        map_acp_session_update(&update),
        vec![SessionServerEvent::Plan {
            entries: vec![
                PlanEntry {
                    content: "Read the router".to_string(),
                    status: "completed".to_string(),
                },
                PlanEntry {
                    content: "Patch the guard".to_string(),
                    status: "in_progress".to_string(),
                },
            ],
        }]
    );
}

#[test]
fn unknown_update_body_is_pretty_printed_not_a_single_line_dump() {
    let update = serde_json::json!({
        "update": { "sessionUpdate": "some_future_update", "detail": ["a"] }
    });
    let events = map_acp_session_update(&update);
    let SessionServerEvent::Artifact { body, .. } = &events[0] else {
        panic!("expected artifact, got {events:?}");
    };
    assert!(body.as_deref().unwrap_or_default().contains('\n'));
}

/// Cursor emits these on every `session/new`; they are capability
/// announcements, not conversation, and must not reach the transcript.
#[test]
fn capability_announcements_are_dropped() {
    for kind in ["available_commands_update", "current_mode_update"] {
        let update = serde_json::json!({
            "update": { "sessionUpdate": kind, "availableCommands": [] }
        });
        assert!(
            map_acp_session_update(&update).is_empty(),
            "{kind} should not reach the transcript"
        );
    }
}

#[test]
fn map_agent_message_chunk_to_browser_message() {
    let update = serde_json::json!({
        "sessionId": "sess_1",
        "update": {
            "sessionUpdate": "agent_message_chunk",
            "content": { "type": "text", "text": "Working on it" }
        }
    });
    let events = map_acp_session_update(&update);
    assert_eq!(
        events,
        vec![SessionServerEvent::Message {
            role: "agent".to_string(),
            text: "Working on it".to_string(),
        }]
    );
}

#[test]
fn cancel_message_defaults_keep_queue_false() {
    let msg: SessionClientMessage = serde_json::from_str(r#"{"type":"cancel"}"#).expect("cancel");
    assert_eq!(msg, SessionClientMessage::Cancel { keep_queue: false });
}

#[test]
fn cancel_message_keep_queue_true() {
    let msg: SessionClientMessage =
        serde_json::from_str(r#"{"type":"cancel","keepQueue":true}"#).expect("cancel");
    assert_eq!(msg, SessionClientMessage::Cancel { keep_queue: true });
}

#[test]
fn dispatch_prompt_starts_when_idle() {
    let mut queued = VecDeque::new();
    assert_eq!(
        dispatch_prompt(false, &mut queued, "hello".to_string()),
        PromptDispatch::StartNow
    );
    assert!(queued.is_empty());
}

#[test]
fn dispatch_prompt_queues_when_in_flight() {
    let mut queued = VecDeque::new();
    assert_eq!(
        dispatch_prompt(true, &mut queued, "next".to_string()),
        PromptDispatch::Queued
    );
    assert_eq!(queued, VecDeque::from(["next".to_string()]));
}

#[test]
fn dispatch_prompt_cap_drops_oldest() {
    let mut queued: VecDeque<String> = (0..MAX_QUEUED_PROMPTS)
        .map(|i| format!("old-{i}"))
        .collect();
    assert_eq!(
        dispatch_prompt(true, &mut queued, "new".to_string()),
        PromptDispatch::Queued
    );
    assert_eq!(queued.len(), MAX_QUEUED_PROMPTS);
    assert_eq!(queued.front().map(String::as_str), Some("old-1"));
    assert_eq!(queued.back().map(String::as_str), Some("new"));
}

#[test]
fn clear_prompt_queue_empties_queued() {
    let mut queued = VecDeque::from(["a".to_string(), "b".to_string()]);
    clear_prompt_queue(&mut queued);
    assert!(queued.is_empty());
}

#[test]
fn apply_cancel_to_queue_keep_true_leaves_queue() {
    let mut queued = VecDeque::from(["next".to_string()]);
    apply_cancel_to_queue(&mut queued, true);
    assert_eq!(queued, VecDeque::from(["next".to_string()]));
}

#[test]
fn apply_cancel_to_queue_keep_false_clears() {
    let mut queued = VecDeque::from(["next".to_string()]);
    apply_cancel_to_queue(&mut queued, false);
    assert!(queued.is_empty());
}

#[test]
fn map_acp_client_request_session_request_permission() {
    let params = serde_json::json!({
        "requestId": "req-1",
        "title": "Run tests?",
        "message": "Allow npm test",
    });
    assert_eq!(
        map_acp_client_request("session/request_permission", &params),
        Some(SessionServerEvent::PermissionRequest {
            request_id: "req-1".to_string(),
            title: Some("Run tests?".to_string()),
            detail: Some("Allow npm test".to_string()),
        })
    );
}

#[test]
fn map_acp_client_request_permission_nested_fields() {
    let params = serde_json::json!({
        "request_id": "req-2",
        "permission": {
            "title": "Deploy?",
            "description": "Push to prod",
        },
    });
    assert_eq!(
        map_acp_client_request("session/request_permission", &params),
        Some(SessionServerEvent::PermissionRequest {
            request_id: "req-2".to_string(),
            title: Some("Deploy?".to_string()),
            detail: Some("Push to prod".to_string()),
        })
    );
}
