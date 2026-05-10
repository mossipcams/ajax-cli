use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::{process::HangDetector, SupervisorError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodexAdapter {
    program: String,
}

impl CodexAdapter {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }

    pub fn exec_json_args(&self, prompt: &str) -> Vec<String> {
        vec!["exec".to_string(), "--json".to_string(), prompt.to_string()]
    }

    pub async fn supervise_exec_json_with_options(
        &self,
        prompt: &str,
        events: mpsc::Sender<MonitorEvent>,
        hang_after: Option<Duration>,
    ) -> Result<Option<i32>, SupervisorError> {
        let mut child = Command::new(&self.program)
            .args(self.exec_json_args(prompt))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let hang_detector = hang_after
            .map(|hang_after| Arc::new(Mutex::new(HangDetector::new(Instant::now(), hang_after))));
        let process_done = Arc::new(AtomicBool::new(false));

        send_event(
            &events,
            MonitorEvent::Process(ProcessEvent::Started { pid: child.id() }),
        )
        .await?;

        let hang_task = hang_detector.as_ref().map(|hang_detector| {
            let hang_events = events.clone();
            let hang_detector = Arc::clone(hang_detector);
            let hang_process_done = Arc::clone(&process_done);
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if hang_process_done.load(Ordering::SeqCst) {
                        break;
                    }
                    let quiet_for = match hang_detector.lock() {
                        Ok(detector) if detector.is_hung(Instant::now()) => {
                            detector.quiet_for(Instant::now())
                        }
                        Ok(_) => continue,
                        Err(_) => break,
                    };
                    let _ = send_event(
                        &hang_events,
                        MonitorEvent::Process(ProcessEvent::Hung { quiet_for }),
                    )
                    .await;
                    break;
                }
            })
        });

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SupervisorError::Process("missing codex stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SupervisorError::Process("missing codex stderr".to_string()))?;

        let stdout_events = events.clone();
        let stdout_hang_detector = hang_detector.clone();
        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Some(line) = lines.next_line().await? {
                observe_output(stdout_hang_detector.as_ref());
                let event = parse_codex_json_line(&line)
                    .map(MonitorEvent::Agent)
                    .unwrap_or_else(|| MonitorEvent::Process(ProcessEvent::Stdout { line }));
                send_event(&stdout_events, event).await?;
            }
            Ok::<(), SupervisorError>(())
        });

        let stderr_events = events.clone();
        let stderr_hang_detector = hang_detector.clone();
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Some(line) = lines.next_line().await? {
                observe_output(stderr_hang_detector.as_ref());
                send_event(
                    &stderr_events,
                    MonitorEvent::Process(ProcessEvent::Stderr { line }),
                )
                .await?;
            }
            Ok::<(), SupervisorError>(())
        });

        stdout_task
            .await
            .map_err(|error| SupervisorError::Process(error.to_string()))??;
        stderr_task
            .await
            .map_err(|error| SupervisorError::Process(error.to_string()))??;

        let status = child.wait().await?;
        process_done.store(true, Ordering::SeqCst);
        if let Some(hang_task) = hang_task {
            hang_task.abort();
        }
        let status_code = status.code();
        send_event(
            &events,
            MonitorEvent::Process(ProcessEvent::Exited { code: status_code }),
        )
        .await?;

        if !status.success() {
            let message = status_code.map_or_else(
                || "codex exited without a status code".to_string(),
                |code| format!("codex exited with status {code}"),
            );
            return Err(SupervisorError::Process(message));
        }

        Ok(status_code)
    }
}

fn observe_output(hang_detector: Option<&Arc<Mutex<HangDetector>>>) {
    if let Some(hang_detector) = hang_detector {
        if let Ok(mut detector) = hang_detector.lock() {
            detector.observe_output(Instant::now());
        }
    }
}

async fn send_event(
    events: &mpsc::Sender<MonitorEvent>,
    event: MonitorEvent,
) -> Result<(), SupervisorError> {
    events
        .send(event)
        .await
        .map_err(|_| SupervisorError::Process("monitor event receiver closed".to_string()))
}

fn parse_codex_json_line(line: &str) -> Option<AgentEvent> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    let event_type = value
        .get("type")
        .or_else(|| value.get("event"))
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let text = value
        .get("message")
        .or_else(|| value.get("text"))
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default();

    if event_type.contains("approval") || mentions_approval(text) {
        return Some(AgentEvent::WaitingForApproval {
            command: string_field(&value, &["command", "cmd"]),
        });
    }

    if event_type.contains("tool") {
        return Some(AgentEvent::ToolCall {
            name: string_field(&value, &["name", "tool"]).unwrap_or_else(|| "tool".to_string()),
        });
    }

    if event_type.contains("start") {
        return Some(AgentEvent::Started {
            agent: "codex".to_string(),
        });
    }

    if event_type.contains("complete") || event_type == "done" {
        return Some(AgentEvent::Completed);
    }

    if event_type.contains("error") || event_type.contains("failed") {
        return Some(AgentEvent::Failed {
            message: if text.is_empty() {
                "codex reported failure".to_string()
            } else {
                text.to_string()
            },
        });
    }

    if !text.is_empty() {
        return Some(AgentEvent::Message {
            text: text.to_string(),
        });
    }

    None
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| value.get(*key))
        .find_map(Value::as_str)
        .map(str::to_string)
}

fn mentions_approval(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("approval required")
        || lower.contains("requires approval")
        || lower.contains("waiting for approval")
        || lower.contains("allow command")
        || lower.contains("proceed?")
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use rstest::rstest;
    use tokio::sync::mpsc;

    use super::{mentions_approval, parse_codex_json_line, CodexAdapter};

    #[test]
    fn codex_json_lines_map_to_agent_events() {
        assert_eq!(
            parse_codex_json_line(r#"{"type":"approval_request","command":"cargo test"}"#),
            Some(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string())
            })
        );
        assert_eq!(
            parse_codex_json_line(r#"{"type":"tool_call","name":"shell"}"#),
            Some(AgentEvent::ToolCall {
                name: "shell".to_string()
            })
        );
        assert_eq!(
            parse_codex_json_line(r#"{"type":"completed"}"#),
            Some(AgentEvent::Completed)
        );
    }

    #[test]
    fn codex_json_messages_do_not_infer_approval_from_negative_phrasing() {
        assert_eq!(
            parse_codex_json_line(r#"{"type":"message","message":"no approval needed"}"#),
            Some(AgentEvent::Message {
                text: "no approval needed".to_string()
            })
        );
    }

    #[rstest]
    #[case("approval required")]
    #[case("requires approval")]
    #[case("waiting for approval")]
    #[case("allow command")]
    #[case("proceed?")]
    fn approval_phrase_variants_are_detected(#[case] text: &str) {
        assert_mentions_approval(text);
    }

    fn assert_mentions_approval(text: &str) {
        assert!(mentions_approval(text), "{text:?} should request approval");
    }

    #[rstest]
    #[case("no approval needed")]
    #[case("approved automatically")]
    #[case("continue without prompting")]
    fn non_approval_phrases_are_not_detected(#[case] text: &str) {
        assert!(
            !mentions_approval(text),
            "{text:?} should not request approval"
        );
    }

    #[rstest]
    #[case(r#"{"type":"error"}"#, "codex reported failure")]
    #[case(r#"{"type":"failed","message":"tests failed"}"#, "tests failed")]
    fn codex_failure_events_map_to_failed_agent_events(
        #[case] line: &str,
        #[case] expected_message: &str,
    ) {
        assert_eq!(
            parse_codex_json_line(line),
            Some(AgentEvent::Failed {
                message: expected_message.to_string()
            })
        );
    }

    #[rstest]
    #[case(r#"{"type":"completed"}"#)]
    #[case(r#"{"type":"done"}"#)]
    fn codex_completion_event_variants_complete(#[case] line: &str) {
        assert_eq!(parse_codex_json_line(line), Some(AgentEvent::Completed));
    }

    #[test]
    fn codex_adapter_builds_exec_json_arguments() {
        let adapter = CodexAdapter::new("codex");

        assert_eq!(
            adapter.exec_json_args("fix tests"),
            vec!["exec", "--json", "fix tests"]
        );
    }

    #[tokio::test]
    async fn codex_supervisor_streams_jsonl_stdout_stderr_and_exit() {
        let script = std::env::temp_dir().join(format!("ajax-fake-codex-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nprintf '{\"type\":\"approval_request\",\"command\":\"cargo test\"}\\n'\nprintf 'warn\\n' >&2\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, mut rx) = mpsc::channel(8);

        adapter
            .supervise_exec_json_with_options("ignored", tx, None)
            .await
            .unwrap();

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert!(matches!(
            events.first(),
            Some(MonitorEvent::Process(ProcessEvent::Started { .. }))
        ));
        assert!(events.contains(&MonitorEvent::Agent(AgentEvent::Started {
            agent: "codex".to_string()
        })));
        assert!(
            events.contains(&MonitorEvent::Agent(AgentEvent::WaitingForApproval {
                command: Some("cargo test".to_string())
            }))
        );
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Stderr {
                line: "warn".to_string()
            }))
        );
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(0)
            }))
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn codex_supervisor_reports_nonzero_agent_exit() {
        let script =
            std::env::temp_dir().join(format!("ajax-fake-codex-nonzero-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nexit 42\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, mut rx) = mpsc::channel(8);

        let error = adapter
            .supervise_exec_json_with_options("ignored", tx, None)
            .await
            .unwrap_err();

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert!(matches!(
            error,
            crate::SupervisorError::Process(message)
                if message == "codex exited with status 42"
        ));
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(42)
            }))
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn codex_supervisor_reports_closed_monitor_channel() {
        let script =
            std::env::temp_dir().join(format!("ajax-fake-codex-closed-{}", std::process::id()));
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, rx) = mpsc::channel(1);
        drop(rx);

        let error = adapter
            .supervise_exec_json_with_options("ignored", tx, None)
            .await
            .unwrap_err();

        assert!(
            matches!(error, crate::SupervisorError::Process(message) if message.contains("monitor event receiver closed"))
        );

        let _ = fs::remove_file(script);
    }
}
