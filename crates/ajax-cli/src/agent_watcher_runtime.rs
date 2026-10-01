//! Web-hosted watcher. Socket lines only wake journal reads; registry writes
//! and notification delivery remain on the existing cockpit refresh path.

use std::{
    collections::{HashMap, HashSet},
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex, OnceLock, Weak},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ajax_core::{
    agent_watcher::{
        apply_verdict, cancel_pending_watcher_nudge, enqueue_watcher_nudge, load_watcher_state,
        step, store_watcher_state, AgentProgressJudge, JudgeError, PendingCheckpoint, Step,
        TaskFrame, WatcherConfig, WatcherDecision, WatcherEvent, WatcherEventDetail,
        WatcherEventKind, WatcherPersistedState, WatcherPhase, WatcherReason, WatcherSnapshot,
        WatcherState, WatcherVerdict,
    },
    commands::CommandContext,
    models::{LifecycleStatus, Task, TaskId},
    registry::{InMemoryRegistry, Registry},
};

mod journal;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod tests;

pub(crate) struct NullJudge;

// Host-only context omitted by the core judge port. Generic judges retain the
// original snapshot-only contract; Laya receives the actual pending checkpoint.
pub(crate) type CheckpointJudge = Arc<
    dyn Fn(&WatcherSnapshot, Option<PendingCheckpoint>) -> Result<WatcherVerdict, JudgeError>
        + Send
        + Sync,
>;

impl AgentProgressJudge for NullJudge {
    fn evaluate(&self, _: &WatcherSnapshot) -> Result<WatcherVerdict, JudgeError> {
        Err(JudgeError::Unavailable)
    }
}

#[derive(Clone)]
struct Frame {
    objective: String,
    harness: String,
    persisted: Option<WatcherPersistedState>,
}

struct Nudge {
    task_id: String,
    nudge_id: String,
    reason: WatcherReason,
    persisted_state: WatcherPersistedState,
}

#[derive(Default)]
struct Mailbox {
    frames: HashMap<String, Frame>,
    tick: Option<u64>,
    states: HashMap<String, WatcherPersistedState>,
    outbox: Vec<Nudge>,
    cancelled: HashSet<String>,
}

pub(crate) struct WatcherRuntime {
    shared: Arc<Mutex<Mailbox>>,
    wake: mpsc::Sender<String>,
}

// Weak handles keep bridge clones attached to the web host for their runtime
// directory without starting workers in ordinary CLI commands or HTTP tests.
static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Weak<WatcherRuntime>>>> = OnceLock::new();

impl WatcherRuntime {
    #[cfg(test)]
    pub(crate) fn start(
        events_dir: PathBuf,
        judge: Arc<dyn AgentProgressJudge + Send + Sync>,
    ) -> io::Result<Arc<Self>> {
        Self::start_with_timeout(events_dir, judge, Duration::from_secs(2))
    }

    #[cfg(test)]
    pub(crate) fn start_with_timeout(
        events_dir: PathBuf,
        judge: Arc<dyn AgentProgressJudge + Send + Sync>,
        timeout: Duration,
    ) -> io::Result<Arc<Self>> {
        Self::start_with_checkpoint_timeout(
            events_dir,
            Arc::new(move |snapshot, _| judge.evaluate(snapshot)),
            timeout,
        )
    }

    pub(crate) fn start_with_checkpoint_timeout(
        events_dir: PathBuf,
        judge: CheckpointJudge,
        timeout: Duration,
    ) -> io::Result<Arc<Self>> {
        let shared = Arc::new(Mutex::new(Mailbox::default()));
        let (wake, inbox) = mpsc::channel::<String>();
        let mut worker = Worker {
            shared: shared.clone(),
            events_dir: events_dir.clone(),
            tasks: HashMap::new(),
            config: WatcherConfig::default(),
            judge,
            judge_thread: None,
            timeout,
            started_at_ms: now_ms(),
        };
        thread::Builder::new()
            .name("ajax-agent-watcher".into())
            .spawn(move || {
                while let Ok(line) = inbox.recv() {
                    worker.consume(&line);
                }
            })?;
        let runtime = Arc::new(Self { shared, wake });
        let mut runtimes = RUNTIMES
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|_| io::Error::other("watcher runtime lock poisoned"))?;
        runtimes.retain(|_, runtime| runtime.strong_count() > 0);
        runtimes.insert(events_dir, Arc::downgrade(&runtime));
        Ok(runtime)
    }

    pub(crate) fn for_events_dir(events_dir: &std::path::Path) -> Option<Arc<Self>> {
        RUNTIMES.get()?.lock().ok()?.get(events_dir)?.upgrade()
    }

    pub(crate) fn sink(&self) -> mpsc::Sender<String> {
        self.wake.clone()
    }

    /// Called only with the live registry on the cockpit refresh lane.
    pub(crate) fn refresh(&self, context: &mut CommandContext<InMemoryRegistry>) -> bool {
        let Ok(mut shared) = self.shared.lock() else {
            return false;
        };
        let mut changed = false;
        // Apply cancellations before any newer nudge from the same journal batch.
        for id in shared.cancelled.drain() {
            if let Some(task) = context.registry.get_task_mut(&TaskId::new(id)) {
                changed |= cancel_pending_watcher_nudge(task);
            }
        }
        for nudge in shared.outbox.drain(..) {
            if let Some(task) = context.registry.get_task_mut(&TaskId::new(nudge.task_id)) {
                if eligible(task) {
                    changed |= enqueue_watcher_nudge(task, &nudge.nudge_id, nudge.reason);
                    changed |= store_watcher_state(task, &nudge.persisted_state);
                }
            }
        }
        for (id, state) in shared.states.drain() {
            if let Some(task) = context.registry.get_task_mut(&TaskId::new(id)) {
                if eligible(task) {
                    changed |= store_watcher_state(task, &state);
                    if matches!(
                        state.phase,
                        WatcherPhase::Escalated | WatcherPhase::WaitingOnUser
                    ) {
                        changed |= cancel_pending_watcher_nudge(task);
                    }
                }
            }
        }
        shared.frames = context
            .registry
            .list_tasks()
            .into_iter()
            .filter(|task| eligible(task))
            .map(|task| {
                (
                    task.id.as_str().to_owned(),
                    Frame {
                        // Task intent stores the original title, not terminal prompts.
                        objective: task.title.clone(),
                        harness: format!("{:?}", task.selected_agent).to_ascii_lowercase(),
                        persisted: load_watcher_state(task),
                    },
                )
            })
            .collect();
        shared.tick = Some(now_ms());
        drop(shared);
        let _ = self.wake.send(String::new());
        changed
    }
}

fn eligible(task: &Task) -> bool {
    !matches!(
        task.lifecycle_status,
        LifecycleStatus::Merged
            | LifecycleStatus::Cleanable
            | LifecycleStatus::Removing
            | LifecycleStatus::TeardownIncomplete
            | LifecycleStatus::Removed
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

struct WatchedTask {
    state: WatcherState,
    offset: u64,
    restoring: Option<WatcherPersistedState>,
    initial_read: bool,
    newest_event_at_ms: Option<u64>,
    last_judge_at_ms: Option<u64>,
}

const EVENT_FRESHNESS_MS: u64 = 5 * 60 * 1000;
const JUDGE_COOLDOWN_MS: u64 = 30 * 1000;

struct Worker {
    shared: Arc<Mutex<Mailbox>>,
    events_dir: PathBuf,
    tasks: HashMap<String, WatchedTask>,
    config: WatcherConfig,
    judge: CheckpointJudge,
    judge_thread: Option<JoinHandle<()>>,
    timeout: Duration,
    started_at_ms: u64,
}

impl Worker {
    fn consume(&mut self, line: &str) {
        let (frames, tick) = {
            let Ok(mut shared) = self.shared.lock() else {
                return;
            };
            (shared.frames.clone(), shared.tick.take())
        };
        self.tasks.retain(|id, _| frames.contains_key(id));
        let notified = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|value| value.get("task_id")?.as_str().map(str::to_owned));
        for (id, frame) in frames {
            if tick.is_none() && notified.as_deref() != Some(&id) {
                continue;
            }
            let mut task = self.tasks.remove(&id).unwrap_or_else(|| WatchedTask {
                state: WatcherState::new(
                    &TaskFrame {
                        objective: frame.objective.clone(),
                    },
                    &id,
                    "primary",
                    &frame.harness,
                ),
                offset: 0,
                restoring: frame.persisted.clone(),
                initial_read: true,
                newest_event_at_ms: None,
                last_judge_at_ms: None,
            });
            task.state.objective = frame.objective;
            task.state.harness = frame.harness;
            if let Err(error) = self.read_journal(&mut task) {
                if error.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(task_id = id, %error, "watcher journal read failed");
                }
            }
            if let Some(now) = tick {
                if task.restoring.is_none() && task.state.phase == WatcherPhase::Recovering {
                    // Synthetic ticks must never replace the durable replay cursor.
                    let seen = task.state.seen_event_ids.clone();
                    self.step(
                        &mut task.state,
                        &WatcherEvent {
                            kind: WatcherEventKind::Heartbeat,
                            detail: WatcherEventDetail::None,
                            occurred_at_ms: now,
                            event_id: format!("watcher-tick-{now}"),
                        },
                        now,
                        false,
                    );
                    task.state.seen_event_ids = seen;
                }
            }
            if task.restoring.is_none() {
                self.resolve_checkpoint(&mut task, tick.unwrap_or_else(now_ms));
                self.publish_state(&task.state);
            }
            self.tasks.insert(id, task);
        }
    }

    fn step(&mut self, state: &mut WatcherState, event: &WatcherEvent, now: u64, replay: bool) {
        if let Step::NeedsJudge(_) = step(state, event, now, &self.config) {
            if replay {
                state.pending_checkpoint = None;
            }
        }
    }

    fn resolve_checkpoint(&mut self, task: &mut WatchedTask, now: u64) {
        if task
            .newest_event_at_ms
            .is_none_or(|at| now.saturating_sub(at) > EVENT_FRESHNESS_MS)
        {
            task.state.pending_checkpoint = None;
            return;
        }
        let state = &task.state;
        if state.pending_checkpoint.is_none() {
            return;
        }
        let pending = self.shared.lock().map_or(true, |shared| {
            shared
                .outbox
                .iter()
                .any(|nudge| nudge.task_id == state.task_id)
        });
        let checkpoint_index = state.event_index;
        let checkpoint_run = state.run_id.clone();
        let mut verdict = if pending
            || state.open_children > 0
            || !state.open_tools.is_empty()
            || state.pending_attention.is_some()
            || matches!(
                state.phase,
                WatcherPhase::Escalated | WatcherPhase::WaitingOnUser
            )
            || state.grace_is_active(now)
            || task
                .last_judge_at_ms
                .is_some_and(|at| now.saturating_sub(at) < JUDGE_COOLDOWN_MS)
        {
            Err(JudgeError::Unavailable)
        } else {
            task.last_judge_at_ms = Some(now);
            self.evaluate(state.snapshot(now), state.pending_checkpoint)
        };
        // Fold evidence arriving during evaluation before acting. Duplicate
        // lines leave the checkpoint intact; fresh evidence invalidates it.
        if std::fs::metadata(self.journal_path(&state.task_id))
            .map_or(true, |meta| meta.len() != task.offset)
            && (self.read_journal(task).is_err()
                || task.state.event_index != checkpoint_index
                || task.state.run_id != checkpoint_run
                || std::fs::metadata(self.journal_path(&task.state.task_id))
                    .map_or(true, |meta| meta.len() != task.offset))
        {
            verdict = Err(JudgeError::Unavailable);
        }
        let state = &mut task.state;
        let decision = apply_verdict(state, verdict, now, &self.config);
        if let WatcherDecision::Nudge { reason } = decision {
            if let Ok(mut shared) = self.shared.lock() {
                // Publish the intervention and its cursor atomically: refresh
                // must never overwrite this nudge's budget with an older state.
                shared
                    .states
                    .insert(state.task_id.clone(), state.persisted());
                shared.outbox.push(Nudge {
                    task_id: state.task_id.clone(),
                    nudge_id: format!(
                        "watcher-{}-{}-{}",
                        state.task_id, state.run_id, state.nudge_seq
                    ),
                    reason,
                    persisted_state: state.persisted(),
                });
            }
        }
    }

    fn publish_state(&self, state: &WatcherState) {
        if state.seen_event_ids.is_empty() {
            return;
        }
        if let Ok(mut shared) = self.shared.lock() {
            if matches!(
                state.phase,
                WatcherPhase::Escalated | WatcherPhase::WaitingOnUser
            ) {
                shared.outbox.retain(|nudge| nudge.task_id != state.task_id);
                shared.cancelled.insert(state.task_id.clone());
            }
            shared
                .states
                .insert(state.task_id.clone(), state.persisted());
        }
    }

    fn evaluate(
        &mut self,
        snapshot: WatcherSnapshot,
        checkpoint: Option<PendingCheckpoint>,
    ) -> Result<WatcherVerdict, JudgeError> {
        // A timed-out judge may still be running. Never accumulate abandoned
        // judge threads; fail open until that single call finishes.
        if self
            .judge_thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
        {
            return Err(JudgeError::Unavailable);
        }
        let judge = self.judge.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        self.judge_thread = Some(
            thread::Builder::new()
                .name("ajax-watcher-judge".into())
                .spawn(move || {
                    let verdict = catch_unwind(AssertUnwindSafe(|| judge(&snapshot, checkpoint)))
                        .unwrap_or(Err(JudgeError::Unavailable));
                    let _ = tx.send(verdict);
                })
                .map_err(|_| JudgeError::Unavailable)?,
        );
        rx.recv_timeout(self.timeout)
            .unwrap_or(Err(JudgeError::Timeout))
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn judge_receives_explicit_checkpoint_without_rewriting_snapshot() {
        let snapshot = WatcherState::new(
            &TaskFrame {
                objective: "fix parser".into(),
            },
            "task",
            "run",
            "codex",
        )
        .snapshot(1000);
        let expected = snapshot.clone();
        let mut worker = Worker {
            shared: Arc::new(Mutex::new(Mailbox::default())),
            events_dir: PathBuf::new(),
            tasks: HashMap::new(),
            config: WatcherConfig::default(),
            judge: Arc::new(move |actual, checkpoint| {
                assert_eq!(actual, &expected);
                assert_eq!(checkpoint, Some(PendingCheckpoint::GraceExpiry));
                Ok(WatcherVerdict {
                    state: ajax_core::agent_watcher::ProgressState::Progressing,
                    confidence: 0.9,
                })
            }),
            judge_thread: None,
            timeout: Duration::from_secs(1),
            started_at_ms: now_ms(),
        };
        assert!(worker
            .evaluate(snapshot, Some(PendingCheckpoint::GraceExpiry))
            .is_ok());
    }
}
