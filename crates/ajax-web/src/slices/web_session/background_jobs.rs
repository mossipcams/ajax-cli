//! Background-job tracking for Ajax Chat sessions (issue #1412).
//!
//! While a session's root turn is not running, every live topmost shell under
//! the session's recorded child pid is one background job: a router delegate,
//! a Monitor watcher or a build the agent left running. The tracker turns those
//! sightings into `ChildStarted` / `ChildEnded` events, so the reducer holds the
//! task at "Background work running" instead of "Response ready".
//!
//! The tracker is pure: it takes injected instants and run-id sets. Only
//! [`TaskSessionState::poll_background_jobs`] runs `ps`, on the blocking pool.

use super::process_identity;
use super::process_tree;
use super::session_activity::{try_report_session_activity, SessionActivity};
use super::task_session::TaskSessionState;
use ajax_core::acp_execution_state::AcpExecutionEvent;
use std::collections::btree_map::Entry;
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

#[derive(Debug, Default)]
pub(super) struct BackgroundJobs {
    /// Run id -> the last poll at which the job was seen live.
    tracked: BTreeMap<String, Instant>,
    /// Start-time tokens for pids seen in recent scans, so `ps -p` runs once per new pid.
    tokens: BTreeMap<u32, String>,
    last_poll: Option<Instant>,
    /// Set when a turn ends; the next due poll then runs immediately.
    poll_after_turn: bool,
}

impl BackgroundJobs {
    pub(super) fn note_turn_ended(&mut self) {
        self.poll_after_turn = true;
    }

    pub(super) fn is_tracking(&self) -> bool {
        !self.tracked.is_empty()
    }

    /// Due when the root turn is not running, a turn just ended or jobs are
    /// tracked, and either a turn just ended or `POLL_EVERY` has passed.
    pub(super) fn poll_due(&self, turn_running: bool, now: Instant) -> bool {
        if turn_running || !(self.poll_after_turn || self.is_tracking()) {
            return false;
        }
        self.poll_after_turn
            || self
                .last_poll
                .is_none_or(|last| now.saturating_duration_since(last) >= POLL_EVERY)
    }

    pub(super) fn mark_polled(&mut self, now: Instant) {
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
            events.push(AcpExecutionEvent::ChildEnded {
                run_id,
                completion_holding: true,
            });
        }
        events
    }
}

pub(super) fn run_id(pid: u32, token: &str) -> String {
    format!("bg-{pid}-{token}")
}

/// Map shell pids to run ids. `lookup` supplies a start token only for a pid
/// not cached yet; a pid whose token cannot be read has already exited and is
/// left out. Cache entries for pids absent from `pids` are dropped, so a reused
/// pid cannot inherit a token from an earlier job.
pub(super) fn run_ids(
    tokens: &mut BTreeMap<u32, String>,
    pids: &[u32],
    mut lookup: impl FnMut(u32) -> Option<String>,
) -> BTreeSet<String> {
    tokens.retain(|pid, _| pids.contains(pid));
    let mut ids = BTreeSet::new();
    for &pid in pids {
        let token = match tokens.entry(pid) {
            Entry::Occupied(slot) => slot.get().clone(),
            Entry::Vacant(slot) => match lookup(pid) {
                Some(token) => slot.insert(token).clone(),
                None => continue,
            },
        };
        ids.insert(run_id(pid, &token));
    }
    ids
}

/// Blocking: one process-table snapshot, then start-token lookups for new pids.
/// `None` means the snapshot is unavailable (the sandbox denies `ps`). The caller
/// must read that as no evidence, not as every job having ended.
pub(super) fn scan(tokens: &mut BTreeMap<u32, String>, root: u32) -> Option<BTreeSet<String>> {
    let table = process_tree::snapshot()?;
    let pids = process_tree::background_shells(&table, root);
    Some(run_ids(tokens, &pids, |pid| {
        process_identity::start_token(i64::from(pid))
    }))
}

impl TaskSessionState {
    /// Scan the session's process subtree when a poll is due, feed the tracker and
    /// report any change through the same path as turn events.
    pub(super) async fn poll_background_jobs(&mut self) {
        let Some(root) = self
            .acp
            .client
            .as_ref()
            .and_then(|client| client.child_pid())
        else {
            return;
        };
        let now = Instant::now();
        if !self
            .background
            .poll_due(self.evidence.activity_reporter.turn_running(), now)
        {
            return;
        }
        self.background.mark_polled(now);
        let mut tokens = std::mem::take(&mut self.background.tokens);
        let scanned = tokio::task::spawn_blocking(move || {
            let live = scan(&mut tokens, root);
            (tokens, live)
        })
        .await;
        let Ok((tokens, live)) = scanned else {
            return;
        };
        self.background.tokens = tokens;
        let Some(live) = live else {
            return;
        };
        let events = self.background.observe(&live, now);
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
        match try_report_session_activity(
            &self.evidence.report_activity,
            &self.qualified_handle,
            activity,
        ) {
            Ok(()) => {
                self.evidence.activity_reporter.commit(activity);
                self.evidence.pending_activity_report = None;
                self.evidence.activity_report_fault = None;
            }
            Err(error) => {
                self.evidence.note_activity_report_failure(&error);
                self.evidence.pending_activity_report = Some(activity);
            }
        }
    }
}
