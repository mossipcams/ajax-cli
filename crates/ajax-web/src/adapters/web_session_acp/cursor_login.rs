use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::cursor_sdk_spawn::cursor_login_launch;
use super::jsonl_process::{JsonlProcess, JsonlRecord};

const GLOBAL_TIMEOUT: Duration = Duration::from_secs(300);
const STATUS_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum CursorLoginPhase {
    Idle,
    Waiting {
        url: String,
    },
    Verifying,
    Succeeded {
        email: Option<String>,
        #[serde(rename = "expiresAtMs")]
        expires_at_ms: Option<u64>,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorCredentialStatus {
    pub present: bool,
    pub valid: Option<bool>,
    pub email: Option<String>,
    pub expires_at_ms: Option<u64>,
    pub error: Option<String>,
}

struct Inner {
    phase: CursorLoginPhase,
    flow_id: u64,
    cancel_flag: Arc<AtomicBool>,
}

pub struct CursorLogin {
    store_path: Option<PathBuf>,
    timeout: Duration,
    state: Arc<Mutex<Inner>>,
}

impl CursorLogin {
    pub fn new(store_path: Option<PathBuf>, timeout: Duration) -> Self {
        Self {
            store_path,
            timeout,
            state: Arc::new(Mutex::new(Inner {
                phase: CursorLoginPhase::Idle,
                flow_id: 0,
                cancel_flag: Arc::new(AtomicBool::new(false)),
            })),
        }
    }

    pub fn global() -> &'static CursorLogin {
        static GLOBAL: OnceLock<CursorLogin> = OnceLock::new();
        GLOBAL.get_or_init(|| Self::new(None, GLOBAL_TIMEOUT))
    }

    pub fn phase(&self) -> CursorLoginPhase {
        self.state
            .lock()
            .expect("login state lock poisoned")
            .phase
            .clone()
    }

    pub fn start(&self) -> CursorLoginPhase {
        let (flow, cancel_flag) = {
            let mut state = self.lock();
            if matches!(
                state.phase,
                CursorLoginPhase::Waiting { .. } | CursorLoginPhase::Verifying
            ) {
                return state.phase.clone();
            }
            state.flow_id += 1;
            let cancel_flag = Arc::new(AtomicBool::new(false));
            state.cancel_flag = cancel_flag.clone();
            (state.flow_id, cancel_flag)
        };

        let (program, args) = match cursor_login_launch("login", self.store_path.as_deref()) {
            Ok(launch) => launch,
            Err(error) => {
                self.fail_flow(flow, error.to_string());
                return self.phase();
            }
        };
        let process = match JsonlProcess::spawn_unsandboxed(&program, &args, &std::env::temp_dir())
        {
            Ok(process) => process,
            Err(error) => {
                self.fail_flow(flow, format!("cursor login spawn failed: {error}"));
                return self.phase();
            }
        };

        watch_sidecar(
            Arc::clone(&self.state),
            flow,
            cancel_flag,
            process,
            self.timeout,
        );
        self.phase()
    }

    pub fn cancel(&self) -> CursorLoginPhase {
        {
            let state = self.lock();
            if matches!(
                state.phase,
                CursorLoginPhase::Waiting { .. } | CursorLoginPhase::Verifying
            ) {
                state.cancel_flag.store(true, Ordering::Relaxed);
            }
        }
        self.phase()
    }

    pub fn status(&self) -> Result<CursorCredentialStatus, String> {
        let (program, args) = cursor_login_launch("status", self.store_path.as_deref())
            .map_err(|error| error.to_string())?;
        let process = JsonlProcess::spawn_unsandboxed(&program, &args, &std::env::temp_dir())
            .map_err(|error| format!("cursor status spawn failed: {error}"))?;
        let deadline = Instant::now() + STATUS_TIMEOUT;
        loop {
            match process.try_recv() {
                Ok(JsonlRecord::Record(value)) => {
                    if value.get("type").and_then(Value::as_str) == Some("status") {
                        return Ok(status_from_value(&value));
                    }
                }
                Ok(JsonlRecord::Error(text)) => {
                    return Err(format!("cursor status emitted non-JSON line: {text}"));
                }
                Ok(JsonlRecord::Exited) => {
                    return Err("cursor status exited before reporting".to_string());
                }
                Err(mpsc::TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "cursor status timed out after {} ms",
                            STATUS_TIMEOUT.as_millis()
                        ));
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("cursor status channel closed".to_string());
                }
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.state.lock().expect("login state lock poisoned")
    }

    fn fail_flow(&self, flow: u64, message: String) {
        let mut state = self.lock();
        if state.flow_id == flow {
            state.phase = CursorLoginPhase::Failed { message };
        }
    }
}

fn is_terminal(phase: &CursorLoginPhase) -> bool {
    matches!(
        phase,
        CursorLoginPhase::Succeeded { .. } | CursorLoginPhase::Failed { .. }
    )
}

fn apply_phase(state: &Mutex<Inner>, flow: u64, next: CursorLoginPhase) {
    let mut guard = state.lock().expect("login state lock poisoned");
    if guard.flow_id != flow || is_terminal(&guard.phase) {
        return;
    }
    guard.phase = next;
}

fn watch_sidecar(
    state: Arc<Mutex<Inner>>,
    flow: u64,
    cancel_flag: Arc<AtomicBool>,
    mut process: JsonlProcess,
    timeout: Duration,
) {
    std::thread::spawn(move || {
        let deadline = Instant::now() + timeout;
        let mut stdin_closed = false;
        loop {
            match process.try_recv() {
                Ok(JsonlRecord::Record(value)) => match value.get("type").and_then(Value::as_str) {
                    Some("url") => apply_phase(
                        &state,
                        flow,
                        CursorLoginPhase::Waiting {
                            url: value
                                .get("url")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        },
                    ),
                    Some("verifying") => {
                        apply_phase(&state, flow, CursorLoginPhase::Verifying);
                    }
                    Some("succeeded") => apply_phase(
                        &state,
                        flow,
                        CursorLoginPhase::Succeeded {
                            email: value
                                .get("email")
                                .and_then(|v| v.as_str().map(String::from)),
                            expires_at_ms: value.get("expiresAtMs").and_then(Value::as_u64),
                        },
                    ),
                    Some("failed") => apply_phase(
                        &state,
                        flow,
                        CursorLoginPhase::Failed {
                            message: value
                                .get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("cursor login failed")
                                .to_string(),
                        },
                    ),
                    _ => {}
                },
                Ok(JsonlRecord::Error(_)) => {}
                Ok(JsonlRecord::Exited) => {
                    apply_phase(
                        &state,
                        flow,
                        CursorLoginPhase::Failed {
                            message: "cursor login exited unexpectedly".to_string(),
                        },
                    );
                    break;
                }
                Err(mpsc::TryRecvError::Empty) | Err(mpsc::TryRecvError::Disconnected) => {}
            }

            if !stdin_closed && cancel_flag.load(Ordering::Relaxed) {
                process.close_stdin();
                stdin_closed = true;
            }
            if Instant::now() >= deadline {
                apply_phase(
                    &state,
                    flow,
                    CursorLoginPhase::Failed {
                        message: format!("cursor login timed out after {} ms", timeout.as_millis()),
                    },
                );
                process.close_stdin();
                break;
            }

            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

fn status_from_value(value: &Value) -> CursorCredentialStatus {
    let present = value
        .get("present")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    CursorCredentialStatus {
        present,
        valid: value.get("valid").and_then(Value::as_bool),
        email: value
            .get("email")
            .and_then(|v| v.as_str().map(String::from)),
        expires_at_ms: value.get("expiresAtMs").and_then(Value::as_u64),
        error: value
            .get("error")
            .and_then(|v| v.as_str().map(String::from)),
    }
}
