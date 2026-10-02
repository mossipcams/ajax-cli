//! Persistent local decision sidecar. Transport failures always fail open.
use ajax_core::{
    agent_watcher::{
        AgentProgressJudge, JudgeError, PendingCheckpoint, ProgressState, WatcherSnapshot,
        WatcherVerdict,
    },
    config::{LayaCommand, WatcherConfig},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;
mod transport;
#[cfg(test)]
pub(crate) use transport::tests::capture_warnings;

const MIN_CONFIDENCE: f64 = 0.6;
const BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(10);

type Verdict = Result<WatcherVerdict, JudgeError>;

struct Request {
    snapshot: Value,
    deadline: Instant,
    reply: mpsc::SyncSender<Verdict>,
}

pub(crate) struct LayaJudge {
    requests: mpsc::SyncSender<Request>,
    ready: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    timeout: Duration,
}

/// Disabled watcher has no judge or child. The web host also skips runtime
/// startup without a command; the null judge remains a fail-open fallback.
pub(crate) fn configured_judge(
    config: &WatcherConfig,
) -> Option<crate::agent_watcher_runtime::CheckpointJudge> {
    if !config.enabled {
        return None;
    }
    Some(match configured_command(config) {
        Some(command) => {
            let judge = LayaJudge::new(
                command.clone(),
                Duration::from_millis(config.judge_timeout_ms.clamp(200, 10_000)),
            );
            Arc::new(move |snapshot, checkpoint| judge.evaluate_checkpoint(snapshot, checkpoint))
        }
        None => Arc::new(|snapshot, _| crate::agent_watcher_runtime::NullJudge.evaluate(snapshot)),
    })
}

/// Empty commands use the same idle host and null judge as an unset command.
pub(crate) fn configured_command(config: &WatcherConfig) -> Option<&LayaCommand> {
    config
        .laya_command
        .as_ref()
        .filter(|command| match command {
            LayaCommand::String(command) => !command.trim().is_empty(),
            LayaCommand::Argv(args) => args.iter().any(|arg| !arg.trim().is_empty()),
        })
}

impl LayaJudge {
    pub(crate) fn new(command: LayaCommand, timeout: Duration) -> Self {
        let (requests, inbox) = mpsc::sync_channel(1);
        let ready = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_ready = ready.clone();
        let worker_stop = stop.clone();
        // Failure to create the worker leaves a permanently unavailable judge.
        let _ = thread::Builder::new()
            .name("ajax-laya".into())
            .spawn(move || {
                supervise(command, inbox, worker_ready, worker_stop);
            });
        Self {
            requests,
            ready,
            stop,
            timeout,
        }
    }
}

impl AgentProgressJudge for LayaJudge {
    fn evaluate(&self, snapshot: &WatcherSnapshot) -> Verdict {
        self.evaluate_checkpoint(snapshot, None)
    }
}

impl LayaJudge {
    fn evaluate_checkpoint(
        &self,
        snapshot: &WatcherSnapshot,
        checkpoint: Option<PendingCheckpoint>,
    ) -> Verdict {
        let deadline = Instant::now() + self.timeout;
        if self
            .ready
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(JudgeError::Unavailable);
        }
        let (reply, result) = mpsc::sync_channel(1);
        self.requests
            .try_send(Request {
                snapshot: compact_snapshot(snapshot, checkpoint),
                deadline,
                reply,
            })
            .map_err(|_| JudgeError::Unavailable)?;
        result
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(Err(JudgeError::Timeout))
    }
}

impl Drop for LayaJudge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn supervise(
    command: LayaCommand,
    inbox: mpsc::Receiver<Request>,
    ready: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    let mut id = 0u64;
    let mut backoff = BACKOFF;
    while !stop.load(Ordering::Acquire) {
        let mut failure = JudgeError::Unavailable;
        let diagnostics;
        match transport::Sidecar::spawn(&command, stop.clone()) {
            Err(error) => {
                diagnostics = error.to_string();
                if error.kind() == std::io::ErrorKind::InvalidInput {
                    tracing::warn!(%diagnostics, "Laya judge unavailable: invalid command");
                    return;
                }
            }
            Ok(mut child) => {
                let initialized = loop {
                    match child.receive(Instant::now() + STARTUP_TIMEOUT) {
                        Ok(value) if value["type"] == "ready" => break true,
                        Ok(_) => continue,
                        Err(error) => {
                            failure = error;
                            if !child.healthy() || stop.load(Ordering::Acquire) {
                                break false;
                            }
                            if error == JudgeError::Malformed {
                                continue;
                            }
                            retry_delay(&inbox, &stop, &mut backoff, failure, &child.diagnostics());
                        }
                    }
                };
                if initialized {
                    ready.store(true, Ordering::Release);
                    loop {
                        if stop.load(Ordering::Acquire) || !child.idle() {
                            break;
                        }
                        match inbox.recv_timeout(POLL) {
                            Ok(request) => {
                                id = id.wrapping_add(1);
                                let mut verdict =
                                    child.exchange(id, request.snapshot, request.deadline);
                                if Instant::now() >= request.deadline && verdict.is_ok() {
                                    verdict = Err(JudgeError::Timeout);
                                }
                                if let Err(error) = verdict {
                                    failure = error;
                                }
                                let succeeded = verdict.is_ok();
                                ready.store(succeeded, Ordering::Release);
                                // A caller timing out must never tear down a healthy child.
                                let _ = request.reply.send(verdict);
                                if succeeded {
                                    backoff = BACKOFF;
                                } else {
                                    if !child.healthy() {
                                        break;
                                    }
                                    retry_delay(
                                        &inbox,
                                        &stop,
                                        &mut backoff,
                                        failure,
                                        &child.diagnostics(),
                                    );
                                    if !child.idle() {
                                        break;
                                    }
                                    ready.store(true, Ordering::Release);
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => return,
                        }
                    }
                }
                ready.store(false, Ordering::Release);
                diagnostics = child.diagnostics();
                // Cleanup and joining happen only on this worker, never on evaluate.
            }
        }
        retry_delay(&inbox, &stop, &mut backoff, failure, &diagnostics);
    }
}

fn retry_delay(
    inbox: &mpsc::Receiver<Request>,
    stop: &AtomicBool,
    backoff: &mut Duration,
    error: JudgeError,
    diagnostics: &str,
) {
    if stop.load(Ordering::Acquire) {
        return;
    }
    // One warning per retry cycle, combining protocol errors and bounded stderr.
    tracing::warn!(
        ?error,
        retry_after_secs = backoff.as_secs(),
        diagnostics,
        "Laya judge unavailable"
    );
    let retry_at = Instant::now() + *backoff;
    *backoff = (*backoff * 2).min(MAX_BACKOFF);
    while !stop.load(Ordering::Acquire) && Instant::now() < retry_at {
        match inbox.recv_timeout(POLL) {
            Ok(request) => {
                let _ = request.reply.send(Err(JudgeError::Unavailable));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn parse_reply(value: Value, id: u64) -> Verdict {
    #[derive(Deserialize)]
    struct Reply {
        id: u64,
        state: ProgressState,
        confidence: f64,
    }
    let reply: Reply = serde_json::from_value(value).map_err(|_| JudgeError::Malformed)?;
    if reply.id != id || !reply.confidence.is_finite() || !(0.0..=1.0).contains(&reply.confidence) {
        return Err(JudgeError::Malformed);
    }
    Ok(WatcherVerdict {
        state: if reply.confidence < MIN_CONFIDENCE {
            ProgressState::Uncertain
        } else {
            reply.state
        },
        confidence: reply.confidence,
    })
}

fn compact_snapshot(snapshot: &WatcherSnapshot, checkpoint: Option<PendingCheckpoint>) -> Value {
    fn text(value: &str, limit: usize) -> String {
        value.chars().take(limit).collect()
    }
    fn recent(values: &[String], limit: usize) -> Vec<String> {
        values[values.len().saturating_sub(8)..]
            .iter()
            .map(|s| text(s, limit))
            .collect()
    }
    json!({
        "objective": text(&snapshot.objective, 512),
        "harness": text(&snapshot.harness, 32),
        "phase": snapshot.phase,
        "checkpoint": checkpoint,
        "recent_signatures": recent(&snapshot.recent_signatures, 80),
        "recent_events": recent(&snapshot.recent_events, 32),
        "pending_attention": snapshot.pending_attention,
        "open_children": snapshot.open_children,
        "intervention_count": snapshot.intervention_count,
        "ms_since_meaningful_activity": snapshot.ms_since_meaningful_activity,
        "last_verdict": snapshot.last_verdict,
    })
}
