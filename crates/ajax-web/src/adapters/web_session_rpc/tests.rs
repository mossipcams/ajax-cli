use super::*;
use std::{fs, io::Write, path::PathBuf, time::Duration};

#[test]
fn encode_acp_notification_omits_request_id() {
    let line = encode_acp_notification("session/cancel", json!({ "sessionId": "sess-1" }));
    let parsed: serde_json::Value = serde_json::from_str(line.trim()).expect("json");
    assert_eq!(parsed["jsonrpc"], "2.0");
    assert_eq!(parsed["method"], "session/cancel");
    assert_eq!(parsed["params"]["sessionId"], "sess-1");
    assert!(parsed.get("id").is_none());
}

#[test]
fn parse_acp_event_line_maps_session_update_and_prompt_result() {
    let delta = br#"{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"text":"Hi"}}}}"#;
    assert_eq!(
        parse_acp_event_line(delta).expect("parse"),
        Some(AgentAcpEvent::AssistantDelta {
            text: "Hi".to_string()
        })
    );

    let settled = br#"{"jsonrpc":"2.0","id":4,"result":{"stopReason":"end_turn"}}"#;
    assert_eq!(
        parse_acp_event_line(settled).expect("parse"),
        Some(AgentAcpEvent::AgentSettled)
    );

    let error = br#"{"jsonrpc":"2.0","id":5,"error":{"message":"boom"}}"#;
    assert_eq!(
        parse_acp_event_line(error).expect("parse"),
        Some(AgentAcpEvent::Error {
            message: "boom".to_string()
        })
    );
}

#[test]
fn fake_acp_peer_handshake_prompt_stream_and_settled_without_live_llm() {
    let script_path = write_temp_script(&fake_acp_peer_script());
    let worktree =
        std::env::temp_dir().join(format!("ajax-web-session-acp-{}", std::process::id()));
    fs::create_dir_all(&worktree).expect("worktree");
    let mut peer = AgentAcpProcess::spawn(&worktree, "bash", &[script_path.to_str().unwrap()])
        .expect("spawn fake peer");

    let session_id = peer.handshake(&worktree).expect("handshake");
    assert_eq!(session_id, "test-session");

    peer.send_prompt("hello").expect("send prompt");

    let mut saw_delta = false;
    let mut saw_settled = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        match peer.poll_event() {
            Some(AgentAcpEvent::AssistantDelta { text }) => {
                assert_eq!(text, "Hello");
                saw_delta = true;
            }
            Some(AgentAcpEvent::AgentSettled) => {
                saw_settled = true;
                break;
            }
            Some(AgentAcpEvent::Exited) => break,
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }

    assert!(saw_delta && saw_settled);
    let _ = fs::remove_dir_all(worktree);
}

#[test]
fn send_cancel_writes_notification_without_waiting_for_reply() {
    let script_path = write_temp_script(&fake_acp_peer_script());
    let worktree =
        std::env::temp_dir().join(format!("ajax-web-session-cancel-{}", std::process::id()));
    fs::create_dir_all(&worktree).expect("worktree");
    let mut peer = AgentAcpProcess::spawn(&worktree, "bash", &[script_path.to_str().unwrap()])
        .expect("spawn fake peer");
    peer.handshake(&worktree).expect("handshake");

    peer.send_cancel().expect("cancel");

    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    let mut exited = false;
    while std::time::Instant::now() < deadline {
        if matches!(peer.poll_event(), Some(AgentAcpEvent::Exited)) {
            exited = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        exited,
        "cancel notification should not block waiting for a reply"
    );
    let _ = fs::remove_dir_all(worktree);
}

#[test]
fn park_permission_request_emits_operator_event_without_auto_reply() {
    let script = r#"set -euo pipefail
while IFS= read -r line; do
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$method" in
    initialize|authenticate)
      printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
      ;;
    session/new)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"test-session"}}\n' "$id"
      ;;
    session/prompt)
      printf '{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"title":"Run tests"}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id"
      ;;
    session/cancel)
      exit 0
      ;;
  esac
done"#;
    let script_path = write_temp_script(script);
    let worktree =
        std::env::temp_dir().join(format!("ajax-web-session-park-{}", std::process::id()));
    fs::create_dir_all(&worktree).expect("worktree");
    let mut peer = AgentAcpProcess::spawn(&worktree, "bash", &[script_path.to_str().unwrap()])
        .expect("spawn fake peer");
    peer.handshake(&worktree).expect("handshake");
    peer.send_prompt("hello").expect("prompt");

    let mut saw_permission = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        match peer.poll_event() {
            Some(AgentAcpEvent::OperatorRequest {
                kind: OperatorRequestKind::Permission,
                summary,
                ..
            }) => {
                assert!(summary.contains("Permission"));
                saw_permission = true;
                break;
            }
            Some(AgentAcpEvent::Exited) => break,
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    assert!(saw_permission, "expected parked permission request");
    let _ = fs::remove_dir_all(worktree);
}

fn fake_acp_peer_script() -> String {
    r#"set -euo pipefail
while IFS= read -r line; do
  method=$(printf '%s' "$line" | sed -n 's/.*"method":"\([^"]*\)".*/\1/p')
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9][0-9]*\).*/\1/p')
  case "$method" in
    initialize|authenticate)
      printf '{"jsonrpc":"2.0","id":%s,"result":{}}\n' "$id"
      ;;
    session/new)
      printf '{"jsonrpc":"2.0","id":%s,"result":{"sessionId":"test-session"}}\n' "$id"
      ;;
    session/prompt)
      printf '{"jsonrpc":"2.0","method":"session/update","params":{"update":{"sessionUpdate":"agent_message_chunk","content":{"text":"Hello"}}}}\n'
      printf '{"jsonrpc":"2.0","id":%s,"result":{"stopReason":"end_turn"}}\n' "$id"
      ;;
    session/cancel)
      exit 0
      ;;
  esac
done"#
        .to_string()
}

fn write_temp_script(contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ajax-web-session-fake-acp-{}.sh",
        std::process::id()
    ));
    let mut file = fs::File::create(&path).expect("create script");
    file.write_all(contents.as_bytes()).expect("write script");
    path
}
