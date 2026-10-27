use std::{
    path::{Path, PathBuf},
    process::ExitStatus,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ajax_core::{
    adapters::sandbox::{default_profile_dir, resolve_nono, sandbox_command, sandbox_enabled},
    events::{AgentEvent, MonitorEvent, ProcessEvent},
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, Command},
    sync::{mpsc, watch},
};

use crate::SupervisorError;

pub type StdoutParser = Arc<dyn Fn(&str) -> Option<AgentEvent> + Send + Sync>;

pub trait ProcessProtocol {
    fn process_name(&self) -> &str;
    fn program(&self) -> &str;
    fn args(&self, prompt: &str) -> Vec<String>;
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
    supervise_process_with_cancellation(protocol, prompt, events, hang_after, cancel_rx, None).await
}

pub async fn supervise_process_with_cancellation<P: ProcessProtocol>(
    protocol: &P,
    prompt: &str,
    events: mpsc::Sender<MonitorEvent>,
    hang_after: Option<Duration>,
    cancel: watch::Receiver<bool>,
    worktree: Option<&Path>,
) -> Result<Option<i32>, SupervisorError> {
    supervise_launch(
        protocol,
        prompt,
        events,
        hang_after,
        cancel,
        worktree,
        SandboxInputs {
            enabled: sandbox_enabled(),
            nono: resolve_nono,
            profile_dir: default_profile_dir(),
        },
    )
    .await
}

async fn supervise_launch<P: ProcessProtocol>(
    protocol: &P,
    prompt: &str,
    events: mpsc::Sender<MonitorEvent>,
    hang_after: Option<Duration>,
    mut cancel: watch::Receiver<bool>,
    worktree: Option<&Path>,
    sandbox: SandboxInputs<impl FnOnce() -> Result<PathBuf, String>>,
) -> Result<Option<i32>, SupervisorError> {
    let mut command = agent_command(
        protocol.program(),
        &protocol.args(prompt),
        worktree,
        sandbox,
    )?;
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let hang_detector = hang_after
        .map(|hang_after| Arc::new(Mutex::new(HangDetector::new(Instant::now(), hang_after))));
    let process_done = Arc::new(AtomicBool::new(false));
    let hung_reported = Arc::new(AtomicBool::new(false));

    send_event(
        &events,
        MonitorEvent::Process(ProcessEvent::Started { pid: child.id() }),
    )
    .await?;

    let hang_task = hang_detector.as_ref().map(|hang_detector| {
        let hang_events = events.clone();
        let hang_detector = Arc::clone(hang_detector);
        let hang_process_done = Arc::clone(&process_done);
        let hang_reported = Arc::clone(&hung_reported);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                if hang_process_done.load(Ordering::SeqCst) {
                    break;
                }
                match send_hung_if_quiet(Some(&hang_detector), &hang_events, &hang_reported).await {
                    Ok(true) => break,
                    Ok(false) => continue,
                    Err(_) => break,
                }
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

    let stderr_task = tokio::spawn(read_stderr_lines(stderr, events.clone()));

    let status = tokio::select! {
        status = child.wait() => {
            status?
        }
        changed = cancel.changed() => {
            process_done.store(true, Ordering::SeqCst);
            if let Some(hang_task) = hang_task {
                hang_task.abort();
            }
            stdout_task.abort();
            stderr_task.abort();
            return match changed {
                Ok(()) if *cancel.borrow() => {
                    let _ = send_event(
                        &events,
                        MonitorEvent::Process(ProcessEvent::Exited { code: None }),
                    )
                    .await;
                    terminate(&mut child).await?;
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
    send_hung_if_quiet(hang_detector.as_ref(), &events, &hung_reported).await?;
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

struct SandboxInputs<N> {
    enabled: bool,
    nono: N,
    profile_dir: Option<PathBuf>,
}

fn agent_command(
    program: &str,
    args: &[String],
    worktree: Option<&Path>,
    sandbox: SandboxInputs<impl FnOnce() -> Result<PathBuf, String>>,
) -> Result<Command, SupervisorError> {
    let cwd = match worktree {
        Some(worktree) => worktree.to_path_buf(),
        None => std::env::current_dir().map_err(|error| {
            SupervisorError::Process(format!("failed to read the working directory: {error}"))
        })?,
    };
    let (program, args) = sandbox_command(
        sandbox.enabled,
        sandbox.nono,
        sandbox.profile_dir,
        &cwd,
        &[],
        program,
        args,
    )
    .map_err(SupervisorError::Process)?;
    let mut command = Command::new(program);
    command.args(args);
    if let Some(worktree) = worktree {
        command.current_dir(worktree);
    }
    Ok(command)
}

async fn terminate(child: &mut Child) -> Result<ExitStatus, SupervisorError> {
    if let Some(pid) = child.id() {
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
    if let Ok(Ok(status)) = tokio::time::timeout(Duration::from_secs(2), child.wait()).await {
        return Ok(status);
    }
    child.start_kill()?;
    let status = tokio::time::timeout(Duration::from_secs(2), child.wait())
        .await
        .map_err(|_| SupervisorError::Process("cancelled process did not exit".to_string()))??;
    Ok(status)
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
        let event = match parse_line(&line) {
            Some(event) => {
                observe_output(hang_detector.as_ref());
                MonitorEvent::Agent(event)
            }
            None => MonitorEvent::Process(ProcessEvent::Stdout { line }),
        };
        send_event(&events, event).await?;
    }
    Ok(())
}

async fn read_stderr_lines<R>(
    stderr: R,
    events: mpsc::Sender<MonitorEvent>,
) -> Result<(), SupervisorError>
where
    R: tokio::io::AsyncRead + Send + Unpin + 'static,
{
    let mut lines = BufReader::new(stderr).lines();
    while let Some(line) = lines.next_line().await? {
        send_event(
            &events,
            MonitorEvent::Process(ProcessEvent::Stderr { line }),
        )
        .await?;
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

async fn send_hung_if_quiet(
    hang_detector: Option<&Arc<Mutex<HangDetector>>>,
    events: &mpsc::Sender<MonitorEvent>,
    hung_reported: &AtomicBool,
) -> Result<bool, SupervisorError> {
    if hung_reported.load(Ordering::SeqCst) {
        return Ok(false);
    }
    let Some(hang_detector) = hang_detector else {
        return Ok(false);
    };

    let quiet_for = match hang_detector.lock() {
        Ok(detector) => {
            let now = Instant::now();
            detector.is_hung(now).then(|| detector.quiet_for(now))
        }
        Err(_) => None,
    };
    let Some(quiet_for) = quiet_for else {
        return Ok(false);
    };
    if hung_reported.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }

    send_event(
        events,
        MonitorEvent::Process(ProcessEvent::Hung { quiet_for }),
    )
    .await?;
    Ok(true)
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
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::{atomic::AtomicBool, Arc, Mutex},
        time::{Duration, Instant},
    };

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

    #[tokio::test]
    async fn process_observer_reports_hung_when_quiet_process_exits_before_poll_wakes() {
        let script = std::env::temp_dir().join(format!(
            "ajax-process-observer-fast-hung-{}",
            std::process::id()
        ));
        fs::write(&script, "#!/bin/sh\nprintf '{\"type\":\"started\"}\\n'\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, mut rx) = mpsc::channel(8);

        let status = supervise_process(&adapter, "ignored", tx, Some(Duration::ZERO))
            .await
            .unwrap();
        let mut saw_hung = false;
        while let Ok(event) = rx.try_recv() {
            if matches!(event, MonitorEvent::Process(ProcessEvent::Hung { .. })) {
                saw_hung = true;
                break;
            }
        }

        assert_eq!(status, Some(0));
        assert!(saw_hung);
        let _ = fs::remove_file(script);
    }

    #[tokio::test]
    async fn final_hang_check_reports_quiet_detector_before_process_exit() {
        let detector = Arc::new(Mutex::new(super::HangDetector::new(
            Instant::now() - Duration::from_millis(20),
            Duration::from_millis(10),
        )));
        let (tx, mut rx) = mpsc::channel(1);

        let hung_reported = AtomicBool::new(false);

        super::send_hung_if_quiet(Some(&detector), &tx, &hung_reported)
            .await
            .expect("hung event should send");

        assert!(matches!(
            rx.recv().await,
            Some(MonitorEvent::Process(ProcessEvent::Hung { .. }))
        ));
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

#[cfg(test)]
mod launch_tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{atomic::AtomicBool, Arc, Mutex},
        time::{Duration, Instant},
    };

    use ajax_core::events::{AgentEvent, MonitorEvent, ProcessEvent};
    use tokio::sync::{mpsc, watch};

    use super::{
        agent_command, read_stderr_lines, read_stdout_lines, send_hung_if_quiet, supervise_launch,
        HangDetector, ProcessProtocol, SandboxInputs, StdoutParser,
    };
    use crate::{agent::codex::CodexAdapter, SupervisorError};

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/supervisor-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    struct ShellProtocol(String);

    impl ProcessProtocol for ShellProtocol {
        fn process_name(&self) -> &str {
            "shell"
        }

        fn program(&self) -> &str {
            "/bin/sh"
        }

        fn args(&self, _prompt: &str) -> Vec<String> {
            vec!["-c".to_string(), self.0.clone()]
        }

        fn stdout_parser(&self) -> StdoutParser {
            Arc::new(|_line: &str| None::<AgentEvent>)
        }
    }

    #[test]
    fn enabled_sandbox_wraps_agent_in_nono_with_worktree_cwd() {
        let dir = scratch_dir("enabled-argv");
        let worktree = dir.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        let profile_dir = dir.join("profiles");

        let command = agent_command(
            "codex",
            &["exec".to_string()],
            Some(&worktree),
            SandboxInputs {
                enabled: true,
                nono: || Ok(PathBuf::from("/opt/nono")),
                profile_dir: Some(profile_dir.clone()),
            },
        )
        .unwrap();

        let std_command = command.as_std();
        assert_eq!(std_command.get_program(), "/opt/nono");
        assert_eq!(std_command.get_current_dir(), Some(worktree.as_path()));
        assert!(profile_dir.join("ajax-agent.json").is_file());
        let argv: Vec<String> = std_command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(argv
            .windows(2)
            .any(|pair| pair[0] == "--workdir" && pair[1] == worktree.display().to_string()));
        assert!(argv.ends_with(
            &[
                "--",
                "codex",
                "-c",
                "sandbox_mode=\"danger-full-access\"",
                "exec"
            ]
            .map(String::from)
        ));
    }

    #[test]
    fn disabled_command_keeps_program_and_args_and_sets_worktree_cwd() {
        let worktree = scratch_dir("disabled-cwd");

        let command = agent_command(
            "codex",
            &["exec".to_string(), "fix".to_string()],
            Some(&worktree),
            SandboxInputs {
                enabled: false,
                nono: || panic!("nono must not be resolved when the sandbox is off"),
                profile_dir: None,
            },
        )
        .unwrap();

        let std_command = command.as_std();
        assert_eq!(std_command.get_program(), "codex");
        assert_eq!(
            std_command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["exec", "fix"]
        );
        assert_eq!(std_command.get_current_dir(), Some(worktree.as_path()));
    }

    #[tokio::test]
    async fn enabled_sandbox_failure_stops_before_spawn() {
        let adapter = CodexAdapter::new("/nonexistent/codex".to_string());
        let (tx, mut rx) = mpsc::channel(8);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let error = supervise_launch(
            &adapter,
            "ignored",
            tx,
            None,
            cancel_rx,
            None,
            SandboxInputs {
                enabled: true,
                nono: || Err("nono is not installed; set AJAX_SANDBOX=off".to_string()),
                profile_dir: None,
            },
        )
        .await
        .unwrap_err();

        assert!(
            matches!(&error, SupervisorError::Process(message)
                if message.contains("nono") && message.contains("AJAX_SANDBOX=off")),
            "{error}"
        );
        assert!(rx.try_recv().is_err());
    }

    fn process_alive(pid: u32) -> bool {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    async fn read_pid(path: &Path) -> u32 {
        for _ in 0..400 {
            if let Some(pid) = fs::read_to_string(path)
                .ok()
                .and_then(|text| text.trim().parse().ok())
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!("pid never written to {}", path.display());
    }

    #[tokio::test]
    async fn cancel_terminates_grandchild_through_term_forwarding_parent() {
        let dir = scratch_dir("cancel-term");
        let pid_file = dir.join("grandchild.pid");
        let protocol = ShellProtocol(format!(
            "trap 'kill $CHILD; exit 143' TERM; sleep 30 & CHILD=$!; echo $CHILD > {}; wait",
            pid_file.display()
        ));
        let (tx, _rx) = mpsc::channel(8);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let canceller = tokio::spawn(async move {
            let grandchild = read_pid(&pid_file).await;
            cancel_tx.send(true).unwrap();
            grandchild
        });

        let error = supervise_launch(
            &protocol,
            "ignored",
            tx,
            None,
            cancel_rx,
            None,
            SandboxInputs {
                enabled: false,
                nono: || panic!("nono must not be resolved when the sandbox is off"),
                profile_dir: None,
            },
        )
        .await
        .unwrap_err();
        let grandchild = canceller.await.unwrap();

        assert!(
            matches!(&error, SupervisorError::Process(message) if message == "process cancelled"),
            "{error}"
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while process_alive(grandchild) && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        assert!(
            !process_alive(grandchild),
            "grandchild {grandchild} survived cancel"
        );
    }

    #[tokio::test]
    async fn issue_1237_cancel_ends_event_stream_with_terminal_event() {
        let dir = scratch_dir("issue-1237-cancel-terminal");
        let pid_file = dir.join("agent.pid");
        let protocol = ShellProtocol(format!("echo $$ > {}; sleep 30", pid_file.display()));
        let (tx, mut rx) = mpsc::channel(8);
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let canceller = tokio::spawn(async move {
            read_pid(&pid_file).await;
            cancel_tx.send(true).unwrap();
        });

        let error = supervise_launch(
            &protocol,
            "ignored",
            tx,
            None,
            cancel_rx,
            None,
            SandboxInputs {
                enabled: false,
                nono: || panic!("nono must not be resolved when the sandbox is off"),
                profile_dir: None,
            },
        )
        .await
        .unwrap_err();
        canceller.await.unwrap();

        let mut events = Vec::new();
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(event) = rx.recv().await {
                events.push(event);
            }
        })
        .await
        .expect("event stream should close after cancellation");

        assert!(
            matches!(&error, SupervisorError::Process(message) if message == "process cancelled"),
            "{error}"
        );
        assert!(
            matches!(
                events.last(),
                Some(MonitorEvent::Process(ProcessEvent::Exited { .. }))
            ),
            "cancelled run ended its event stream without a terminal lifecycle event; events: {events:?}"
        );
    }

    fn stale_hang_detector() -> Arc<Mutex<HangDetector>> {
        Arc::new(Mutex::new(HangDetector::new(
            Instant::now() - Duration::from_secs(10),
            Duration::from_secs(1),
        )))
    }

    async fn would_report_hung(detector: &Arc<Mutex<HangDetector>>) -> bool {
        let (tx, _rx) = mpsc::channel(1);
        send_hung_if_quiet(Some(detector), &tx, &AtomicBool::new(false))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn unparsed_stdout_does_not_reset_hang_detection() {
        let detector = stale_hang_detector();
        let (tx, mut rx) = mpsc::channel(8);
        let parse_line: StdoutParser = Arc::new(|_line: &str| None::<AgentEvent>);

        read_stdout_lines(
            &b"heartbeat 0\nheartbeat 1\n"[..],
            tx,
            Some(Arc::clone(&detector)),
            parse_line,
        )
        .await
        .unwrap();

        assert_eq!(
            rx.recv().await,
            Some(MonitorEvent::Process(ProcessEvent::Stdout {
                line: "heartbeat 0".to_string()
            }))
        );
        assert!(would_report_hung(&detector).await);
    }

    #[tokio::test]
    async fn stderr_does_not_reset_hang_detection() {
        let detector = stale_hang_detector();
        let (tx, mut rx) = mpsc::channel(8);

        read_stderr_lines(&b"heartbeat 0\nheartbeat 1\n"[..], tx)
            .await
            .unwrap();

        assert_eq!(
            rx.recv().await,
            Some(MonitorEvent::Process(ProcessEvent::Stderr {
                line: "heartbeat 0".to_string()
            }))
        );
        assert!(would_report_hung(&detector).await);
    }

    #[tokio::test]
    async fn parsed_agent_events_keep_hang_detection_quiet() {
        let detector = stale_hang_detector();
        let (tx, mut rx) = mpsc::channel(8);
        let parse_line: StdoutParser = Arc::new(|line: &str| {
            line.strip_prefix("AGENT ").map(|text| AgentEvent::Message {
                text: text.to_string(),
            })
        });

        read_stdout_lines(
            &b"AGENT tick 0\n"[..],
            tx,
            Some(Arc::clone(&detector)),
            parse_line,
        )
        .await
        .unwrap();

        assert_eq!(
            rx.recv().await,
            Some(MonitorEvent::Agent(AgentEvent::Message {
                text: "tick 0".to_string()
            }))
        );
        assert!(!would_report_hung(&detector).await);
    }
}
