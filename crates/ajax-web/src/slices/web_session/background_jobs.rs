//! Background-work level for Ajax Chat sessions (issue #1412).
//!
//! Background work is a level: once a session's root turn has finished, is any
//! topmost shell under the session's child pid still alive? The level is read
//! while the root turn is finished, not only after an observed turn end: Claude
//! SDK-initiated turns never reach the reporter, so their shells would otherwise
//! go unseen. The reporter probes it when a turn ends (immediacy), and the actor
//! ticks read it on a fixed cadence: every `IDLE_POLL_EVERY` while not held,
//! every `POLL_EVERY` while held. The reporter holds the level as one synthetic
//! completion-holding child, so the status reads `Background work running`.
//!
//! [`BackgroundLevel`] keeps the evidence timing and is pure: it takes injected
//! instants and read results. Only [`TaskSessionState::poll_background_jobs`]
//! runs `ps`, on the blocking pool.

use super::process_tree;
use super::task_session::TaskSessionState;
use std::time::{Duration, Instant, SystemTime};

/// The level stays held this long after the last shell was seen, so a follow-up
/// turn that starts to handle a finished job does not flash `Response ready`.
pub(super) const END_DEBOUNCE: Duration = Duration::from_secs(5);

/// The level stays held this long after the last successful scan. A failing
/// `ps` therefore cannot hold the status forever.
pub(super) const SCAN_TTL: Duration = Duration::from_secs(30);

// ponytail: `ps` polling has a ~2 s detection lag and one exec per poll while the
// level is held. Upgrade path: harness-native background signals, such as the
// Claude SDK `background_tasks_changed` message, where a harness exposes them.
pub(super) const POLL_EVERY: Duration = Duration::from_secs(2);

/// The tick cadence while the level is not held. A shell started after a turn
/// ended is seen within this long.
// ponytail: idle cost ceiling. Every session with a finished turn polls every
// IDLE_POLL_EVERY, and the table cache is shared by age, so all sessions together
// cost at most about one `ps` per 5 s host-wide. Upgrade path: harness-native
// background signals, which would remove the idle poll entirely.
pub(super) const IDLE_POLL_EVERY: Duration = Duration::from_secs(5);

/// A turn-end read may reuse a process table this old, so turn ends share one `ps`.
pub(super) const PROBE_MAX_AGE: Duration = Duration::from_millis(250);

/// A held tick read may reuse a process table this old.
pub(super) const POLL_MAX_AGE: Duration = Duration::from_secs(1);

/// What one tick must do about the level.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Skip,
    /// The level can no longer hold (no client, or the turn is not finished): drop it at once.
    Clear,
    /// Read the level for the shell subtree of this pid.
    Poll(u32),
}

/// Evidence timing for the level. `None` timestamps mean no such evidence yet.
#[derive(Debug, Default)]
pub(super) struct BackgroundLevel {
    /// The last read that saw a background shell.
    last_seen: Option<Instant>,
    /// The last read that completed, whatever it found.
    last_good_scan: Option<Instant>,
    last_poll: Option<Instant>,
}

impl BackgroundLevel {
    /// Decide this tick. `root` is the child pid of the live client, `None` when
    /// there is no client or it has no child pid. `held` is the level as the
    /// reporter holds it.
    pub(super) fn step(
        &mut self,
        root: Option<u32>,
        turn_finished: bool,
        held: bool,
        now: Instant,
    ) -> Step {
        if held && (root.is_none() || !turn_finished) {
            return Step::Clear;
        }
        let Some(pid) = root else {
            return Step::Skip;
        };
        if !self.poll_due(turn_finished, held, now) {
            return Step::Skip;
        }
        self.last_poll = Some(now);
        Step::Poll(pid)
    }

    /// Due only after the turn finished, and one poll spacing after the last poll.
    fn poll_due(&self, turn_finished: bool, held: bool, now: Instant) -> bool {
        let spacing = if held { POLL_EVERY } else { IDLE_POLL_EVERY };
        turn_finished
            && self
                .last_poll
                .is_none_or(|last| now.saturating_duration_since(last) >= spacing)
    }

    /// Record the read taken at turn end.
    pub(super) fn record_turn_end(&mut self, read: Option<bool>, now: Instant) {
        self.observe(read, now);
    }

    /// Record a tick's read. Returns whether the level stays held.
    pub(super) fn record_poll(&mut self, read: Option<bool>, held: bool, now: Instant) -> bool {
        self.observe(read, now);
        match read {
            Some(true) => true,
            Some(false) => {
                held && self
                    .last_seen
                    .is_some_and(|seen| now.saturating_duration_since(seen) < END_DEBOUNCE)
            }
            None => {
                held && self
                    .last_good_scan
                    .is_some_and(|good| now.saturating_duration_since(good) < SCAN_TTL)
            }
        }
    }

    fn observe(&mut self, read: Option<bool>, now: Instant) {
        match read {
            Some(true) => {
                self.last_seen = Some(now);
                self.last_good_scan = Some(now);
            }
            Some(false) => {
                self.last_good_scan = Some(now);
            }
            None => {}
        }
    }
}

/// Whether `root`'s shell subtree holds a background shell, from the shared
/// process table no older than `max_age`. No root (no client, or no child pid)
/// has no process tree, so the level is clear. `None` means the table is
/// unavailable (the sandbox denies `ps`): that is no evidence, not a clear level.
pub(super) fn probe(root: Option<u32>, max_age: Duration) -> Option<bool> {
    let Some(root) = root else {
        return Some(false);
    };
    let table = process_tree::snapshot_cached(max_age)?;
    Some(process_tree::has_background_shell(&table, root))
}

impl TaskSessionState {
    /// Run this tick's step on the background level, and report a change through
    /// the same path as turn events.
    pub(super) async fn poll_background_jobs(&mut self) {
        let root = self
            .acp
            .client
            .as_ref()
            .and_then(|client| client.child_pid());
        let now = Instant::now();
        let turn_finished = self.evidence.activity_reporter.turn_finished();
        let held = self.evidence.activity_reporter.background_held();
        let activity = match self.background.step(root, turn_finished, held, now) {
            Step::Skip => return,
            Step::Clear => self
                .evidence
                .activity_reporter
                .set_background(false, SystemTime::now()),
            Step::Poll(pid) => {
                let max_age = if held { POLL_MAX_AGE } else { IDLE_POLL_EVERY };
                let read = tokio::task::spawn_blocking(move || probe(Some(pid), max_age))
                    .await
                    .ok()
                    .flatten();
                let next = self.background.record_poll(read, held, now);
                if next == held {
                    return;
                }
                self.evidence
                    .activity_reporter
                    .set_background(next, SystemTime::now())
            }
        };
        if let Some(activity) = activity {
            self.evidence.report(&self.qualified_handle, activity);
        }
    }
}
