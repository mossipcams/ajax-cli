//! Task-scoped process facade over the official ACP Rust SDK connection actor.

use ajax_core::{
    adapters::{acp_args_for_candidate, acp_launch_for_agent, acp_spawn_model_for_argv, AcpLaunch},
    models::AgentClient,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use super::apply_model::{operator_pin_satisfied, ApplyModelOutcome};
use super::sdk_connection::{self, ClientCommand, ConnectionReady, RunOptions};
use agent_client_protocol::schema::v1::{ContentBlock, SessionConfigOption, SessionNotification};

#[cfg(test)]
use std::{cell::RefCell, path::PathBuf};

#[cfg(test)]
thread_local! {
    static TEST_ACP_PROGRAM: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    static TEST_ACP_EXTRA_ARGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

#[cfg(test)]
static TEST_ACP_EXTRA_ARGS_GLOBAL: Mutex<Vec<String>> = Mutex::new(Vec::new());

#[cfg(test)]
static TEST_ACP_LOCK: Mutex<()> = Mutex::new(());

/// Run `f` with ACP spawn redirected to a test program (typically a Node fake agent).
#[cfg(test)]
pub(crate) fn with_test_acp_program<F, R>(path: &Path, f: F) -> R
where
    F: FnOnce() -> R,
{
    let _guard = TEST_ACP_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    TEST_ACP_PROGRAM.with(|slot| {
        *slot.borrow_mut() = Some(path.to_path_buf());
        *TEST_ACP_COMMAND.lock().unwrap() = Some((
            PathBuf::from("node"),
            vec![path.to_string_lossy().into_owned()],
        ));
        let result = f();
        *slot.borrow_mut() = None;
        *TEST_ACP_COMMAND.lock().unwrap() = None;
        result
    })
}

/// Process-global ACP spawn override, consulted only when the thread-local one
/// is unset. A test that drives the real session socket cannot reach the
/// spawning thread any other way: the child is spawned on whichever runtime
/// worker upgraded the WebSocket. The program runs directly, with no `node`
/// wrapper, so a test can point ACP at any hanging or failing binary.
#[cfg(test)]
static TEST_ACP_COMMAND: Mutex<Option<(PathBuf, Vec<String>)>> = Mutex::new(None);

#[cfg(test)]
pub(crate) fn set_test_acp_command(command: Option<(&Path, &[&str])>) {
    *TEST_ACP_COMMAND.lock().unwrap() = command.map(|(program, args)| {
        (
            program.to_path_buf(),
            args.iter().map(|arg| (*arg).to_string()).collect(),
        )
    });
}

/// Restores thread-local and global extra args when `with_test_acp_extra_args` exits,
/// including on panic, so a failing test cannot leak argv into a later one.
#[cfg(test)]
struct TestAcpExtraArgsGuard {
    saved_thread: Vec<String>,
    saved_global: Vec<String>,
}

#[cfg(test)]
impl Drop for TestAcpExtraArgsGuard {
    fn drop(&mut self) {
        TEST_ACP_EXTRA_ARGS.with(|slot| {
            *slot.borrow_mut() = self.saved_thread.clone();
        });
        *TEST_ACP_EXTRA_ARGS_GLOBAL.lock().unwrap() = self.saved_global.clone();
    }
}

/// Add argv tokens for the next test ACP spawns inside `f` (e.g. `--load-fail`).
#[cfg(test)]
pub(crate) fn with_test_acp_extra_args<F, R>(args: &[&str], f: F) -> R
where
    F: FnOnce() -> R,
{
    TEST_ACP_EXTRA_ARGS.with(|slot| {
        let saved = slot.borrow().clone();
        let extra: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
        *slot.borrow_mut() = extra.clone();
        let saved_global = TEST_ACP_EXTRA_ARGS_GLOBAL.lock().unwrap().clone();
        *TEST_ACP_EXTRA_ARGS_GLOBAL.lock().unwrap() = extra;
        let _guard = TestAcpExtraArgsGuard {
            saved_thread: saved,
            saved_global,
        };
        f()
    })
}

/// Bound on the ACP handshake (initialize, session/new, config). Generous for a
/// cold bridge, short enough that a stuck harness reports rather than hangs.
pub(super) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(45);

/// Keep only the last few KiB: enough to explain a failure, bounded for a
/// long-lived session.
const STDERR_TAIL_BYTES: usize = 4096;

fn drain_stderr(stderr: impl std::io::Read + Send + 'static, sink: Arc<Mutex<String>>) {
    let reader = BufReader::new(stderr);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let mut tail = sink.lock().unwrap();
        tail.push_str(&line);
        tail.push('\n');
        if tail.len() > STDERR_TAIL_BYTES {
            let cut = tail.len() - STDERR_TAIL_BYTES;
            *tail = tail[cut..].to_string();
        }
    }
}

#[derive(Debug, Clone)]
pub enum AcpClientEvent {
    ConfigOptionsUpdated {
        applied_model: String,
        config_options: Vec<SessionConfigOption>,
    },
    AvailableCommandsUpdated {
        available_commands: Vec<agent_client_protocol::schema::v1::AvailableCommand>,
    },
    SessionInfoUpdated {
        title: Option<String>,
    },
    SessionUpdate(Box<SessionNotification>),
    UnknownSessionUpdate(Value),
    ClientRequest {
        id: Value,
        method: String,
        params: Value,
    },
    ElicitationRequest {
        request_id: String,
        message: String,
        schema: Value,
    },
    RequestFinished {
        id: u64,
        method: &'static str,
        result: Result<Value, String>,
    },
    Error(String),
    Exited,
}

pub struct SpawnReport {
    pub load_session_advertised: bool,
    pub close_advertised: bool,
    pub resumed: bool,
    /// Harness-reported model id after handshake apply ([#952](https://github.com/mossipcams/ajax-cli/issues/952)).
    pub applied_model: String,
    pub model_apply_error: Option<String>,
    pub config_options: Option<Vec<agent_client_protocol::schema::v1::SessionConfigOption>>,
    pub prompt_capabilities: super::PromptCapabilityDescriptor,
}

/// Bound on ACP `session/close` during child teardown.
const CLOSE_SESSION_TIMEOUT: Duration = Duration::from_secs(10);

pub struct AcpStdioClient {
    commands: tokio::sync::mpsc::UnboundedSender<ClientCommand>,
    events: Receiver<AcpClientEvent>,
    next_id: u64,
    busy: Arc<AtomicBool>,
    session_id: String,
    close_advertised: bool,
    /// Kept because each harness advertises its model catalog here.
    session_new_result: Value,
    child: Child,
    torn_down: bool,
    _connection: thread::JoinHandle<()>,
}

impl AcpStdioClient {
    /// Spawn the harness's ACP process. `model` is only placed on the argv for
    /// harnesses that pin at spawn (Cursor); the bridges select in-band.
    pub fn spawn(
        agent: AgentClient,
        worktree_path: &Path,
        model: Option<&str>,
        resume_session_id: Option<&str>,
    ) -> Result<(Self, SpawnReport), String> {
        Self::spawn_internal(agent, worktree_path, model, model, resume_session_id)
    }

    /// Spawn with operator-pin recovery when the first handshake leaves a mismatched model.
    pub fn spawn_with_operator_pin(
        agent: AgentClient,
        worktree_path: &Path,
        operator_pin: &str,
        resume_session_id: Option<&str>,
    ) -> Result<(Self, SpawnReport), String> {
        let launch = acp_launch_for_agent(agent);
        let spawn_model = launch
            .as_ref()
            .and_then(|launch| acp_spawn_model_for_argv(*launch, Some(operator_pin)));
        let model_pins_at_spawn = launch.is_some_and(|launch| launch.model_pins_at_spawn());

        let attempt = |resume: Option<&str>| {
            Self::spawn_internal(
                agent,
                worktree_path,
                spawn_model.as_deref(),
                Some(operator_pin),
                resume,
            )
        };

        let (client, report) = attempt(resume_session_id)?;
        if Self::pin_report_acceptable(operator_pin, &report, model_pins_at_spawn) {
            return Ok((client, report));
        }
        drop(client);
        attempt(None)
    }

    fn pin_report_acceptable(
        operator_pin: &str,
        report: &SpawnReport,
        model_pins_at_spawn: bool,
    ) -> bool {
        let satisfied = if let Some(options) = report.config_options.as_deref() {
            super::config_options::pin_satisfied(Some(options), operator_pin, model_pins_at_spawn)
        } else {
            operator_pin_satisfied(operator_pin, &report.applied_model, model_pins_at_spawn)
        };
        satisfied && report.model_apply_error.is_none()
    }

    fn spawn_internal(
        agent: AgentClient,
        worktree_path: &Path,
        spawn_model: Option<&str>,
        apply_pin: Option<&str>,
        resume_session_id: Option<&str>,
    ) -> Result<(Self, SpawnReport), String> {
        let mut child = spawn_acp_process(agent, worktree_path, spawn_model)?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "acp process missing stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "acp process missing stdout".to_string())?;
        let stderr_tail: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let sink = Arc::clone(&stderr_tail);
            thread::spawn(move || drain_stderr(stderr, sink));
        }
        let (event_tx, event_rx) = mpsc::channel();
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let busy = Arc::new(AtomicBool::new(false));
        let connection_busy = Arc::clone(&busy);
        let cwd = worktree_path.to_path_buf();
        let apply_pin = apply_pin.map(str::to_string);
        let resume_session_id = resume_session_id.map(str::to_string);
        let connection = thread::spawn(move || {
            sdk_connection::run(RunOptions {
                stdin,
                stdout,
                commands: command_rx,
                events: event_tx,
                ready: ready_tx,
                busy: connection_busy,
                agent,
                cwd,
                apply_pin,
                resume_session_id,
            });
        });
        let ready = ready_rx
            .recv_timeout(HANDSHAKE_TIMEOUT + Duration::from_secs(1))
            .map_err(|_| format!("ACP startup timed out{}", stderr_hint(&stderr_tail)))?
            .map_err(|error| format!("{error}{}", stderr_hint(&stderr_tail)))?;
        let ConnectionReady {
            session_id,
            session_new_result,
            config_options,
            prompt_capabilities,
            close_advertised,
            load_session_advertised,
            resumed,
            applied_model,
            model_apply_error,
        } = ready;
        if resumed {
            while event_rx.try_recv().is_ok() {}
            let _ = command_tx.send(ClientCommand::InstallLiveSession);
        }
        let client = Self {
            commands: command_tx,
            events: event_rx,
            next_id: 1,
            busy,
            session_id,
            close_advertised,
            session_new_result,
            child,
            torn_down: false,
            _connection: connection,
        };
        let report = SpawnReport {
            load_session_advertised,
            close_advertised,
            resumed,
            applied_model,
            model_apply_error,
            config_options,
            prompt_capabilities,
        };
        Ok((client, report))
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// End the ACP child: optional `session/close`, then stdio teardown.
    /// Returns a warning when advertised close fails or times out.
    pub fn shutdown(&mut self) -> Option<String> {
        self.tear_down()
    }

    fn tear_down(&mut self) -> Option<String> {
        if self.torn_down {
            return None;
        }
        self.torn_down = true;
        let mut close_error = None;
        if self.close_advertised {
            let (result_tx, result_rx) = mpsc::channel();
            if self
                .commands
                .send(ClientCommand::CloseSession { result: result_tx })
                .is_ok()
            {
                close_error = match result_rx.recv_timeout(CLOSE_SESSION_TIMEOUT) {
                    Ok(Ok(())) => None,
                    Ok(Err(error)) => Some(error),
                    Err(_) => Some(format!(
                        "ACP session/close timed out after {}s",
                        CLOSE_SESSION_TIMEOUT.as_secs()
                    )),
                };
            } else {
                close_error = Some("ACP connection is closed".to_string());
            }
        }
        let _ = self.commands.send(ClientCommand::Shutdown);
        let _ = self.child.kill();
        let _ = self.child.wait();
        close_error
    }

    /// True when the ACP OS process has exited. Reconnect must respawn, not
    /// reattach to a dead stdio pipe.
    pub fn host_exited(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => true,
        }
    }

    #[cfg(test)]
    pub(crate) fn kill_host_for_test(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    #[cfg(test)]
    pub(crate) fn child_id(&self) -> u32 {
        self.child.id()
    }

    pub fn poll_event(&self) -> Option<AcpClientEvent> {
        self.events.try_recv().ok()
    }

    pub fn wait_event(&self, timeout: Duration) -> Option<AcpClientEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    pub fn begin_prompt(&mut self, blocks: &[ContentBlock]) -> Result<u64, String> {
        if self.busy.swap(true, Ordering::AcqRel) {
            return Err("prompt already in flight".to_string());
        }
        let id = self.next_id;
        self.next_id += 1;
        let (result_tx, result_rx) = mpsc::channel();
        if self
            .commands
            .send(ClientCommand::Prompt {
                id,
                blocks: blocks.to_vec(),
                result: result_tx,
            })
            .is_err()
        {
            self.busy.store(false, Ordering::Release);
            return Err("ACP connection is closed".to_string());
        }
        match result_rx.recv_timeout(HANDSHAKE_TIMEOUT) {
            Ok(Ok(())) => Ok(id),
            Ok(Err(error)) => {
                self.busy.store(false, Ordering::Release);
                Err(error)
            }
            Err(_) => {
                self.busy.store(false, Ordering::Release);
                Err("ACP prompt dispatch timed out".to_string())
            }
        }
    }

    /// Cancel the in-flight turn, returning the permission requests it answered.
    ///
    /// ACP cancellation is a notification. Sent as a request, every installed
    /// harness answers `Method not found` and keeps working — Stop did nothing.
    /// The agent ends the turn with `stopReason: "cancelled"`, which settles the
    /// prompt already in flight.
    pub(crate) fn cancel(&mut self) -> Result<super::CancelOutcome, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(ClientCommand::Cancel { result: result_tx })
            .map_err(|_| "ACP connection is closed".to_string())?;
        result_rx
            .recv_timeout(HANDSHAKE_TIMEOUT)
            .map_err(|_| "ACP command timed out".to_string())?
    }

    pub(crate) fn prompt_in_flight(&self) -> bool {
        self.busy.load(Ordering::Acquire)
    }

    pub fn respond_client_request(&mut self, id: &Value, result: Value) -> Result<(), String> {
        let approved = result
            .get("approved")
            .and_then(Value::as_bool)
            .ok_or_else(|| "permission response missing approved".to_string())?;
        let request_id = match id {
            Value::String(value) => value.clone(),
            Value::Number(value) => value.to_string(),
            _ => return Err("unsupported permission request id".to_string()),
        };
        self.command_result(|result| ClientCommand::RespondPermission {
            request_id,
            approved,
            result,
        })
    }

    pub fn respond_elicitation(
        &mut self,
        request_id: &str,
        action: agent_client_protocol::schema::v1::ElicitationAction,
    ) -> Result<(), String> {
        let id = request_id.to_string();
        self.command_result(move |result| ClientCommand::RespondElicitation {
            request_id: id,
            action,
            result,
        })
    }

    /// Raw `session/new` result, which is where each harness advertises the
    /// models it can run. Empty until a session has been created.
    pub fn session_new_result(&self) -> &Value {
        &self.session_new_result
    }

    /// Apply an operator model pin on the live ACP session without respawning.
    pub fn apply_model_pin(&self, desired_model: &str) -> Result<ApplyModelOutcome, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(ClientCommand::ApplyModelPin {
                desired_model: desired_model.to_string(),
                result: result_tx,
            })
            .map_err(|_| "ACP connection is closed".to_string())?;
        result_rx
            .recv_timeout(HANDSHAKE_TIMEOUT)
            .map_err(|_| "ACP apply model timed out".to_string())?
    }

    /// Apply one advertised config option on the live ACP session without respawning.
    pub fn apply_config_option(
        &self,
        config_id: &str,
        value: agent_client_protocol::schema::v1::SessionConfigOptionValue,
    ) -> Result<ApplyModelOutcome, String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(ClientCommand::ApplyConfigOption {
                config_id: config_id.to_string(),
                value,
                result: result_tx,
            })
            .map_err(|_| "ACP connection is closed".to_string())?;
        result_rx
            .recv_timeout(HANDSHAKE_TIMEOUT)
            .map_err(|_| "ACP apply config option timed out".to_string())?
    }

    fn command_result(
        &self,
        command: impl FnOnce(mpsc::Sender<Result<(), String>>) -> ClientCommand,
    ) -> Result<(), String> {
        let (result_tx, result_rx) = mpsc::channel();
        self.commands
            .send(command(result_tx))
            .map_err(|_| "ACP connection is closed".to_string())?;
        result_rx
            .recv_timeout(HANDSHAKE_TIMEOUT)
            .map_err(|_| "ACP command timed out".to_string())?
    }
}

impl Drop for AcpStdioClient {
    fn drop(&mut self) {
        let _ = self.tear_down();
    }
}

fn stderr_hint(stderr_tail: &Mutex<String>) -> String {
    let tail = stderr_tail.lock().unwrap();
    let line = tail
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim();
    if line.is_empty() {
        String::new()
    } else {
        format!(" — {}", line.chars().take(200).collect::<String>())
    }
}

/// Build argv for one candidate program of this harness's ACP launch.
pub(crate) fn acp_args_for_program(
    launch: AcpLaunch,
    base_args: &[&str],
    model: Option<&str>,
) -> Vec<String> {
    acp_args_for_candidate(
        launch,
        base_args,
        acp_spawn_model_for_argv(launch, model).as_deref(),
    )
}

/// Candidate list for this harness, native endpoint first.
///
/// A harness that grows its own `acp` subcommand should be used directly rather
/// than through its packaged adapter, so the CLI is asked (once per TTL) whether
/// it advertises one. Asking beats trying: an unknown argument is a prompt to
/// some CLIs, which would start a real session.
fn acp_candidates(launch: AcpLaunch) -> Vec<(String, Vec<String>)> {
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(program) = launch.native_program {
        if native_acp_advertised(program) {
            candidates.push((program.to_string(), vec!["acp".to_string()]));
        }
    }
    for (program, base_args) in launch.candidates {
        candidates.push((
            (*program).to_string(),
            base_args.iter().map(|arg| (*arg).to_string()).collect(),
        ));
    }
    // Last resort: run the adapter straight from npm. A machine that has it
    // installed never reaches this, and one that does not still gets a session
    // instead of an error — at the cost of the first fetch.
    if let Some(package) = launch.acp_package {
        candidates.push((
            "npx".to_string(),
            vec!["-y".to_string(), package.to_string()],
        ));
    }
    candidates
}

/// True when `<program> --help` lists an `acp` subcommand. Cached: this runs on
/// every session acquire and the answer only changes when the CLI is upgraded.
fn native_acp_advertised(program: &str) -> bool {
    static CACHE: Mutex<Option<HashMap<String, (Instant, bool)>>> = Mutex::new(None);
    const TTL: Duration = Duration::from_secs(300);

    if let Ok(guard) = CACHE.lock() {
        if let Some((checked_at, advertised)) =
            guard.as_ref().and_then(|entries| entries.get(program))
        {
            if checked_at.elapsed() < TTL {
                return *advertised;
            }
        }
    }

    let Some(mut probe) = crate::adapters::program::harness_command(program) else {
        return false;
    };
    #[allow(clippy::let_and_return)]
    let advertised = probe
        .arg("--help")
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .is_some_and(|output| {
            let text = String::from_utf8_lossy(&output.stdout);
            text.lines().any(|line| {
                let line = line.trim_start();
                line.starts_with("acp ") || line == "acp"
            })
        });

    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(program.to_string(), (Instant::now(), advertised));
    }
    advertised
}

fn spawn_acp_process(
    agent: AgentClient,
    worktree_path: &Path,
    model: Option<&str>,
) -> Result<Child, String> {
    #[cfg(test)]
    if let Some(program) = TEST_ACP_PROGRAM.with(|slot| slot.borrow().clone()) {
        let extra_args = TEST_ACP_EXTRA_ARGS.with(|slot| slot.borrow().clone());
        let mut command = std::process::Command::new("node");
        command.arg(program);
        if let Some(launch) = acp_launch_for_agent(agent) {
            if launch.model_pins_at_spawn() {
                if let Some(spawn_model) = acp_spawn_model_for_argv(launch, model) {
                    command.arg("--model").arg(spawn_model);
                }
            }
        }
        command.args(extra_args);
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        return command
            .spawn()
            .map_err(|error| format!("failed to spawn test acp program: {error}"));
    }

    #[cfg(test)]
    {
        let override_command = TEST_ACP_COMMAND.lock().unwrap().clone();
        if let Some((program, mut args)) = override_command {
            args.extend(TEST_ACP_EXTRA_ARGS_GLOBAL.lock().unwrap().iter().cloned());
            let mut command = std::process::Command::new(program);
            command.args(args);
            if let Some(launch) = acp_launch_for_agent(agent) {
                if launch.model_pins_at_spawn() {
                    if let Some(spawn_model) = acp_spawn_model_for_argv(launch, model) {
                        command.arg("--model").arg(spawn_model);
                    }
                }
            }
            command
                .current_dir(worktree_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            return command
                .spawn()
                .map_err(|error| format!("failed to spawn test acp command: {error}"));
        }
    }

    let Some(launch) = acp_launch_for_agent(agent) else {
        return Err(format!("no ACP mapping for agent {agent:?}"));
    };
    for (program, base_args) in acp_candidates(launch) {
        let base_args: Vec<&str> = base_args.iter().map(String::as_str).collect();
        let args = acp_args_for_program(launch, &base_args, model);
        // Resolved outside the server's PATH when a version manager moved it.
        let Some(mut command) = crate::adapters::program::harness_command(&program) else {
            continue;
        };
        command.args(&args);
        command
            .current_dir(worktree_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Ok(child) = command.spawn() {
            return Ok(child);
        }
    }
    // Every candidate is missing: an install problem, not a runtime failure.
    Err(format!(
        "{agent:?} ACP agent is not installed — {}",
        launch.install_hint
    ))
}
