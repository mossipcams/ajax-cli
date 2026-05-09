use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::SupervisorError;

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

    pub async fn supervise_exec_json(
        &self,
        prompt: &str,
        events: mpsc::Sender<MonitorEvent>,
    ) -> Result<(), SupervisorError> {
        let mut child = Command::new(&self.program)
            .args(self.exec_json_args(prompt))
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        send_event(
            &events,
            MonitorEvent::Process(ProcessEvent::Started { pid: child.id() }),
        )
        .await?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| SupervisorError::Process("missing codex stdout".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| SupervisorError::Process("missing codex stderr".to_string()))?;

        let stdout_events = events.clone();
        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Some(line) = lines.next_line().await? {
                let event = parse_codex_json_line(&line)
                    .map(MonitorEvent::Agent)
                    .unwrap_or_else(|| MonitorEvent::Process(ProcessEvent::Stdout { line }));
                send_event(&stdout_events, event).await?;
            }
            Ok::<(), SupervisorError>(())
        });

        let stderr_events = events.clone();
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Some(line) = lines.next_line().await? {
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
        send_event(
            &events,
            MonitorEvent::Process(ProcessEvent::Exited {
                code: status.code(),
            }),
        )
        .await?;

        Ok(())
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

pub fn parse_codex_json_line(line: &str) -> Option<AgentEvent> {
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
    lower.contains("approval") || lower.contains("allow command") || lower.contains("proceed?")
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use tokio::sync::mpsc;

    use super::{parse_codex_json_line, CodexAdapter};

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

        adapter.supervise_exec_json("ignored", tx).await.unwrap();

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
            .supervise_exec_json("ignored", tx)
            .await
            .unwrap_err();

        assert!(
            matches!(error, crate::SupervisorError::Process(message) if message.contains("monitor event receiver closed"))
        );

        let _ = fs::remove_file(script);
    }
}
