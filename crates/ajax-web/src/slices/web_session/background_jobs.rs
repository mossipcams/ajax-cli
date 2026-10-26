//! Background-job tracking for Ajax Chat sessions (issue #1412).
//!
//! While a session's root turn is not running, every live topmost shell under
//! the session's recorded child pid is one background job: a router delegate,
//! a Monitor watcher or a build the agent left running. The tracker turns those
//! sightings into `ChildStarted` / `ChildEnded` events, so the reducer holds the
//! task at "Background work running" instead of "Response ready".
//!
//! The tracker is pure: it takes injected instants and run-id sets, and
//! [`BackgroundJobs::plan`] decides each tick. Only
//! [`TaskSessionState::poll_background_jobs`] runs `ps`, on the blocking pool,
//! and the actor awaits that one scan.

use super::process_tree::{self, ProcessEntry};
use super::session_activity::SessionActivity;
use super::task_session::TaskSessionState;
use ajax_core::acp_execution_state::AcpExecutionEvent;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant, SystemTime};

/// A job must stay out of the process table this long before it is reported
/// ended. The harness often starts a new turn to handle a finished job, and the
/// debounce keeps that gap from flashing "Response ready".
pub(super) const END_DEBOUNCE: Duration = Duration::from_secs(5);

// ponytail: `ps` polling has a ~2 s detection lag and one exec per poll while a
// job is tracked. Upgrade path: harness-native background signals, such as the
// Claude SDK `background_tasks_changed` message, where a harness exposes them.
pub(super) const POLL_EVERY: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(super) struct BackgroundJobs {
    /// Run id -> the last poll at which the job was seen live.
    tracked: BTreeMap<String, Instant>,
    last_poll: Option<Instant>,
    /// Set when the root turn stops, and once at creation; the next due poll then runs immediately.
    poll_after_turn: bool,
    /// Whether the root turn was running at the previous plan, to see it stop.
    turn_was_running: bool,
}

impl Default for BackgroundJobs {
    fn default() -> Self {
        Self {
            tracked: BTreeMap::new(),
            last_poll: None,
            poll_after_turn: true,
            turn_was_running: false,
        }
    }
}

/// What one tick must do about background jobs.
#[derive(Debug, PartialEq)]
pub(super) enum PollPlan {
    Skip,
    /// The client is gone: every tracked job ends at once, without the debounce.
    EndAll(Vec<AcpExecutionEvent>),
    /// Scan the process subtree of this pid.
    Scan(u32),
}

fn child_ended(run_id: String) -> AcpExecutionEvent {
    AcpExecutionEvent::ChildEnded {
        run_id,
        completion_holding: true,
    }
}

impl BackgroundJobs {
    pub(super) fn is_tracking(&self) -> bool {
        !self.tracked.is_empty()
    }

    /// Decide this tick. `root` is the child pid of the live client, `None` when
    /// there is no client. A stop of the root turn arms the next poll.
    pub(super) fn plan(&mut self, root: Option<u32>, turn_running: bool, now: Instant) -> PollPlan {
        if self.turn_was_running && !turn_running {
            self.poll_after_turn = true;
        }
        self.turn_was_running = turn_running;
        let Some(root) = root else {
            return self.end_all();
        };
        if !self.poll_due(turn_running, now) {
            return PollPlan::Skip;
        }
        self.mark_polled(now);
        PollPlan::Scan(root)
    }

    fn end_all(&mut self) -> PollPlan {
        if self.tracked.is_empty() {
            return PollPlan::Skip;
        }
        let events = std::mem::take(&mut self.tracked)
            .into_keys()
            .map(child_ended)
            .collect();
        PollPlan::EndAll(events)
    }

    /// Due when the root turn is not running, a turn just stopped or jobs are
    /// tracked, and either a turn just stopped or `POLL_EVERY` has passed.
    fn poll_due(&self, turn_running: bool, now: Instant) -> bool {
        if turn_running || !(self.poll_after_turn || self.is_tracking()) {
            return false;
        }
        self.poll_after_turn
            || self
                .last_poll
                .is_none_or(|last| now.saturating_duration_since(last) >= POLL_EVERY)
    }

    fn mark_polled(&mut self, now: Instant) {
        self.last_poll = Some(now);
        self.poll_after_turn = false;
    }

    /// Diff the live run ids against the tracked set. A job is reported ended
    /// only after it has been unseen for `END_DEBOUNCE`.
    pub(super) fn observe(
        &mut self,
        live: &BTreeSet<String>,
        now: Instant,
    ) -> Vec<AcpExecutionEvent> {
        let mut events = Vec::new();
        for run_id in live {
            if self.tracked.insert(run_id.clone(), now).is_none() {
                events.push(AcpExecutionEvent::ChildStarted {
                    run_id: run_id.clone(),
                    completion_holding: true,
                });
            }
        }
        let ended: Vec<String> = self
            .tracked
            .iter()
            .filter(|(run_id, seen)| {
                !live.contains(*run_id) && now.saturating_duration_since(**seen) >= END_DEBOUNCE
            })
            .map(|(run_id, _)| run_id.clone())
            .collect();
        for run_id in ended {
            self.tracked.remove(&run_id);
            events.push(child_ended(run_id));
        }
        events
    }
}

/// Run id for one shell. Its start time is part of the id, so a reused pid is a
/// new job, and the spaces are replaced so the id stays one token.
pub(super) fn run_id(pid: u32, start: &str) -> String {
    format!("bg-{pid}-{}", start.replace(' ', "_"))
}

/// Run ids for the topmost shells under `root` in one process-table snapshot.
pub(super) fn run_ids(table: &[ProcessEntry], root: u32) -> BTreeSet<String> {
    process_tree::background_shells(table, root)
        .into_iter()
        .map(|entry| run_id(entry.pid, &entry.start))
        .collect()
}

/// Blocking: one process-table snapshot. `None` means the snapshot is unavailable
/// (the sandbox denies `ps`). The caller must read that as no evidence, not as
/// every job having ended.
pub(super) fn scan(root: u32) -> Option<BTreeSet<String>> {
    let table = process_tree::snapshot()?;
    Some(run_ids(&table, root))
}

impl TaskSessionState {
    /// Run this tick's background-job plan and report any change through the same
    /// path as turn events.
    pub(super) async fn poll_background_jobs(&mut self) {
        let root = match self.acp.client.as_ref() {
            None => None,
            Some(client) => match client.child_pid() {
                Some(pid) => Some(pid),
                // A live client with no child pid has no process tree: no evidence.
                None => return,
            },
        };
        let now = Instant::now();
        let turn_running = self.evidence.activity_reporter.turn_running();
        let events = match self.background.plan(root, turn_running, now) {
            PollPlan::Skip => return,
            PollPlan::EndAll(events) => events,
            PollPlan::Scan(pid) => {
                let scanned = tokio::task::spawn_blocking(move || scan(pid)).await;
                let Ok(Some(live)) = scanned else {
                    return;
                };
                self.background.observe(&live, now)
            }
        };
        if events.is_empty() {
            return;
        }
        if let Some(activity) = self
            .evidence
            .activity_reporter
            .activity_for_batch(events, SystemTime::now())
        {
            self.report_background_activity(activity);
        }
    }

    fn report_background_activity(&mut self, activity: SessionActivity) {
        self.evidence
            .flush_pending_activity_report(&self.qualified_handle);
        self.evidence.report(&self.qualified_handle, activity);
    }
}
