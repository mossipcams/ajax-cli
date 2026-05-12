use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, watch},
};

use crate::SupervisorError;

pub type StdoutParser = Arc<dyn Fn(&str) -> Option<AgentEvent> + Send + Sync>;

pub trait ProcessProtocol {
    fn process_name(&self) -> &str;
    fn program(&self) -> &str;
    fn args(&self, prompt: &str) -> Vec<String>;
    fn parse_stdout_line(&self, line: &str) -> Option<AgentEvent>;
    fn stdout_parser(&self) -> StdoutParser;
}

#[derive(Clone, Debug)]
pub struct HangDetector {
    last_output_at: Instant,
    hang_after: Duration,
}

impl HangDetector {
    pub fn new(now: Instant, hang_after: Duration) -> Self {
        Self {
            last_output_at: now,
            hang_after,
        }
    }

    pub fn observe_output(&mut self, now: Instant) {
        self.last_output_at = now;
    }

    pub fn quiet_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last_output_at)
    }

    pub fn is_hung(&self, now: Instant) -> bool {
        self.quiet_for(now) >= self.hang_after
    }
}

pub async fn supervise_process<P: ProcessProtocol>(
    protocol: &P,
    prompt: &str,
    events: mpsc::Sender<MonitorEvent>,
    hang_after: Option<Duration>,
) -> Result<Option<i32>, SupervisorError> {
    let (_cancel_tx, cancel_rx) = watch::channel(false);
    supervise_process_with_cancellation(protocol, prompt, events, hang_after, cancel_rx).await
}

pub async fn supervise_process_with_cancellation<P: ProcessProtocol>(
    protocol: &P,
    prompt: &str,
    events: mpsc::Sender<MonitorEvent>,
    hang_after: Option<Duration>,
    mut cancel: watch::Receiver<bool>,
) -> Result<Option<i32>, SupervisorError> {
    let mut command = Command::new(protocol.program());
    command
        .args(protocol.args(prompt))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
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
        .ok_or_else(|| SupervisorError::Process("missing process stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| SupervisorError::Process("missing process stderr".to_string()))?;

    let stdout_hang_detector = hang_detector.clone();
    let stdout_task = tokio::spawn(read_stdout_lines(
        stdout,
        events.clone(),
        stdout_hang_detector,
        protocol.stdout_parser(),
    ));

    let stderr_hang_detector = hang_detector.clone();
    let stderr_events = events.clone();
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

    let status = tokio::select! {
        status = child.wait() => status?,
        changed = cancel.changed() => {
            process_done.store(true, Ordering::SeqCst);
            if let Some(hang_task) = hang_task {
                hang_task.abort();
            }
            stdout_task.abort();
            stderr_task.abort();
            return match changed {
                Ok(()) if *cancel.borrow() => {
                    Err(SupervisorError::Process("process cancelled".to_string()))
                }
                Ok(()) => Err(SupervisorError::Process("process cancellation channel changed".to_string())),
                Err(_) => Err(SupervisorError::Process("process cancellation channel closed".to_string())),
            };
        }
    };
    process_done.store(true, Ordering::SeqCst);
    if let Some(hang_task) = hang_task {
        hang_task.abort();
    }
    stdout_task
        .await
        .map_err(|error| SupervisorError::Process(error.to_string()))??;
    stderr_task
        .await
        .map_err(|error| SupervisorError::Process(error.to_string()))??;
    let status_code = status.code();
    send_event(
        &events,
        MonitorEvent::Process(ProcessEvent::Exited { code: status_code }),
    )
    .await?;

    if !status.success() {
        let message = status_code.map_or_else(
            || format!("{} exited without a status code", protocol.process_name()),
            |code| format!("{} exited with status {code}", protocol.process_name()),
        );
        return Err(SupervisorError::Process(message));
    }

    Ok(status_code)
}

async fn read_stdout_lines<R>(
    stdout: R,
    events: mpsc::Sender<MonitorEvent>,
    hang_detector: Option<Arc<Mutex<HangDetector>>>,
    parse_line: StdoutParser,
) -> Result<(), SupervisorError>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await? {
        observe_output(hang_detector.as_ref());
        let event = parse_line(&line)
            .map(MonitorEvent::Agent)
            .unwrap_or_else(|| MonitorEvent::Process(ProcessEvent::Stdout { line }));
        send_event(&events, event).await?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::fs::PermissionsExt, time::Duration};

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use tokio::sync::mpsc;

    use crate::{agent::codex::CodexAdapter, process_observer::supervise_process};

    #[tokio::test]
    async fn process_observer_streams_stdout_stderr_and_exit() {
        let script =
            std::env::temp_dir().join(format!("ajax-process-observer-{}", std::process::id()));
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

        let status = supervise_process(&adapter, "ignored", tx, None)
            .await
            .unwrap();
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert_eq!(status, Some(0));
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
    async fn process_observer_reports_nonzero_exit_after_emitting_exit_event() {
        let script = std::env::temp_dir().join(format!(
            "ajax-process-observer-nonzero-{}",
            std::process::id()
        ));
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

        let error = supervise_process(&adapter, "ignored", tx, None)
            .await
            .unwrap_err();
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        assert!(matches!(
            error,
            SupervisorError::Process(message) if message == "codex exited with status 42"
        ));
        assert!(
            events.contains(&MonitorEvent::Process(ProcessEvent::Exited {
                code: Some(42)
            }))
        );

        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn process_observer_emits_hung_when_process_is_quiet() {
        let script =
            std::env::temp_dir().join(format!("ajax-process-observer-hung-{}", std::process::id()));
        fs::write(
            &script,
            "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\nsleep 1\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, mut rx) = mpsc::channel(8);

        let supervise = tokio::spawn(async move {
            supervise_process(&adapter, "ignored", tx, Some(Duration::from_millis(100))).await
        });
        let mut saw_hung = false;
        while let Some(event) = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("hung event should arrive before timeout")
        {
            if matches!(event, MonitorEvent::Process(ProcessEvent::Hung { .. })) {
                saw_hung = true;
                break;
            }
        }

        assert!(saw_hung);
        let _ = supervise.await.unwrap();
        let _ = fs::remove_file(script);
    }

    #[test]
    fn hang_detector_tracks_quiet_processes() {
        let start = std::time::Instant::now();
        let mut detector = super::HangDetector::new(start, Duration::from_secs(30));

        assert!(!detector.is_hung(start + Duration::from_secs(29)));
        assert!(detector.is_hung(start + Duration::from_secs(30)));

        detector.observe_output(start + Duration::from_secs(40));

        assert!(!detector.is_hung(start + Duration::from_secs(60)));
        assert!(detector.is_hung(start + Duration::from_secs(70)));
    }

    use crate::SupervisorError;
}
