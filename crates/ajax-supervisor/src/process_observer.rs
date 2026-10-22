use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use ajax_core::{
    adapters::sandbox::{
        default_profile_dir, git_common_dir, resolve_nono, sandbox_enabled, wrap_argv,
        write_profile,
    },
    events::{AgentEvent, MonitorEvent, ProcessEvent},
};
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

/// Runs the agent in `worktree` (inherits the current dir when `None`), wrapped in nono when `AJAX_SANDBOX` is on.
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
        SandboxLaunch::from_env(),
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
    sandbox: SandboxLaunch,
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
                    child.start_kill()?;
                    tokio::time::timeout(Duration::from_secs(2), child.wait())
                        .await
                        .map_err(|_| SupervisorError::Process("cancelled process did not exit".to_string()))??;
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

/// Whether the agent is wrapped in nono, with the inputs resolved up front (fail-closed when they error).
enum SandboxLaunch {
    Disabled,
    Enabled {
        nono: Result<PathBuf, String>,
        profile_dir: Option<PathBuf>,
    },
}

impl SandboxLaunch {
    fn from_env() -> Self {
        if sandbox_enabled() {
            Self::Enabled {
                nono: resolve_nono(),
                profile_dir: default_profile_dir(),
            }
        } else {
            Self::Disabled
        }
    }
}

fn agent_command(
    program: &str,
    args: &[String],
    worktree: Option<&Path>,
    sandbox: SandboxLaunch,
) -> Result<Command, SupervisorError> {
    let (program, args) = match sandbox {
        SandboxLaunch::Disabled => (program.to_string(), args.to_vec()),
        SandboxLaunch::Enabled { nono, profile_dir } => {
            sandboxed_argv(nono, profile_dir, worktree, program, args)?
        }
    };
    let mut command = Command::new(program);
    command.args(args);
    if let Some(worktree) = worktree {
        command.current_dir(worktree);
    }
    Ok(command)
}

/// Wraps the agent in nono; any failure returns an error so the agent never runs unsandboxed.
fn sandboxed_argv(
    nono: Result<PathBuf, String>,
    profile_dir: Option<PathBuf>,
    worktree: Option<&Path>,
    program: &str,
    args: &[String],
) -> Result<(String, Vec<String>), SupervisorError> {
    let nono = nono.map_err(SupervisorError::Process)?;
    let profile_dir = profile_dir.ok_or_else(|| {
        SupervisorError::Process(
            "agent sandbox is on but no nono profile directory is available (HOME is unset); set AJAX_SANDBOX=off to run without nono"
                .to_string(),
        )
    })?;
    let profile = write_profile(&profile_dir).map_err(|error| {
        SupervisorError::Process(format!(
            "agent sandbox is on but the nono profile could not be written to {}: {error}; set AJAX_SANDBOX=off to run without nono",
            profile_dir.display()
        ))
    })?;
    let workdir = match worktree {
        Some(worktree) => worktree.to_path_buf(),
        None => std::env::current_dir().map_err(|error| {
            SupervisorError::Process(format!(
                "agent sandbox is on but the working directory is unknown ({error}); set AJAX_SANDBOX=off to run without nono"
            ))
        })?,
    };
    let read_write: Vec<PathBuf> = git_common_dir(&workdir).into_iter().collect();
    Ok(wrap_argv(
        &nono,
        &profile,
        &workdir,
        &read_write,
        program,
        args,
    ))
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
mod sandbox_launch_tests {
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        path::{Path, PathBuf},
    };

    use ajax_core::{
        adapters::sandbox::git_common_dir,
        events::{MonitorEvent, ProcessEvent},
    };
    use tokio::sync::{mpsc, watch};

    use super::{agent_command, sandboxed_argv, supervise_launch, SandboxLaunch};
    use crate::{agent::codex::CodexAdapter, SupervisorError};

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/supervisor-tests")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn enabled_argv_wraps_program_with_profile_and_worktree_grants() {
        let dir = scratch_dir("enabled-argv");
        let worktree = dir.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        let profile_dir = dir.join("profiles");

        let (program, argv) = sandboxed_argv(
            Ok(PathBuf::from("/opt/nono")),
            Some(profile_dir.clone()),
            Some(&worktree),
            "codex",
            &args(&["exec"]),
        )
        .unwrap();

        let profile = profile_dir.join("ajax-agent.json");
        assert!(profile.is_file());
        let grants: Vec<String> = git_common_dir(&worktree)
            .into_iter()
            .flat_map(|common| ["--allow".to_string(), common.display().to_string()])
            .collect();
        let mut expected: Vec<String> = [
            "-s",
            "run",
            "--allow-cwd",
            "--no-rollback-prompt",
            "--profile",
        ]
        .map(String::from)
        .into();
        expected.push(profile.display().to_string());
        expected.extend(["--workdir".to_string(), worktree.display().to_string()]);
        expected.extend(grants);
        expected.extend(args(&[
            "--",
            "codex",
            "-c",
            "sandbox_mode=\"danger-full-access\"",
            "exec",
        ]));
        assert_eq!(program, "/opt/nono");
        assert_eq!(argv, expected);
    }

    #[test]
    fn enabled_sandbox_fails_closed_when_nono_or_profile_dir_is_unavailable() {
        let dir = scratch_dir("fail-closed-argv");
        let worktree = dir.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        let blocker = dir.join("blocker");
        fs::write(&blocker, "not a directory").unwrap();

        let cases = [
            sandboxed_argv(
                Err("agent sandbox is on but nono is not installed; set AJAX_SANDBOX=off".into()),
                Some(dir.join("profiles")),
                Some(&worktree),
                "codex",
                &[],
            ),
            sandboxed_argv(
                Ok(PathBuf::from("/opt/nono")),
                None,
                Some(&worktree),
                "codex",
                &[],
            ),
            sandboxed_argv(
                Ok(PathBuf::from("/opt/nono")),
                Some(blocker.join("profiles")),
                Some(&worktree),
                "codex",
                &[],
            ),
        ];
        for result in cases {
            let message = result
                .expect_err("sandbox failure must not produce a command")
                .to_string();
            assert!(message.contains("nono"), "{message}");
            assert!(message.contains("AJAX_SANDBOX=off"), "{message}");
        }
    }

    #[test]
    fn disabled_command_keeps_program_and_args_and_sets_worktree_cwd() {
        let worktree = scratch_dir("disabled-cwd");

        let command = agent_command(
            "codex",
            &args(&["exec", "fix"]),
            Some(&worktree),
            SandboxLaunch::Disabled,
        )
        .unwrap();

        let std_command = command.as_std();
        assert_eq!(std_command.get_program(), "codex");
        assert_eq!(
            std_command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            vec!["exec".to_string(), "fix".to_string()]
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
            SandboxLaunch::Enabled {
                nono: Err(
                    "agent sandbox is on but nono is not installed; set AJAX_SANDBOX=off"
                        .to_string(),
                ),
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

    #[tokio::test]
    async fn disabled_supervised_process_runs_in_worktree() {
        let dir = scratch_dir("cwd-spawn");
        let worktree = dir.join("worktree");
        fs::create_dir_all(&worktree).unwrap();
        let script = dir.join("codex");
        fs::write(&script, "#!/bin/sh\npwd -P >&2\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let adapter = CodexAdapter::new(script.display().to_string());
        let (tx, mut rx) = mpsc::channel(8);
        let (_cancel_tx, cancel_rx) = watch::channel(false);

        let status = supervise_launch(
            &adapter,
            "ignored",
            tx,
            None,
            cancel_rx,
            Some(&worktree),
            SandboxLaunch::Disabled,
        )
        .await
        .unwrap();

        assert_eq!(status, Some(0));
        let mut stderr = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let MonitorEvent::Process(ProcessEvent::Stderr { line }) = event {
                stderr.push(line);
            }
        }
        assert_eq!(
            stderr,
            vec![worktree.canonicalize().unwrap().display().to_string()]
        );
    }
}
