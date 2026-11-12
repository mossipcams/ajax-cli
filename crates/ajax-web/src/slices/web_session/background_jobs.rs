use super::process_tree::{self, container_table, has_container_background_shell};
use super::task_session::TaskSessionState;
use crate::adapters::program;
use ajax_core::adapters::docker_sandbox::task_key;
use std::time::{Duration, Instant, SystemTime};

pub(super) const END_DEBOUNCE: Duration = Duration::from_secs(5);

pub(super) const SCAN_TTL: Duration = Duration::from_secs(30);

pub(super) const POLL_EVERY: Duration = Duration::from_secs(2);

pub(super) const IDLE_POLL_EVERY: Duration = Duration::from_secs(5);

pub(super) const PROBE_MAX_AGE: Duration = Duration::from_millis(250);

pub(super) const POLL_MAX_AGE: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Skip,
    Clear,
    Poll(u32),
}

#[derive(Debug, Default)]
pub(super) struct BackgroundLevel {
    last_seen: Option<Instant>,
    last_good_scan: Option<Instant>,
    last_poll: Option<Instant>,
}

impl BackgroundLevel {
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

    fn poll_due(&self, turn_finished: bool, held: bool, now: Instant) -> bool {
        let spacing = if held { POLL_EVERY } else { IDLE_POLL_EVERY };
        turn_finished
            && self
                .last_poll
                .is_none_or(|last| now.saturating_duration_since(last) >= spacing)
    }

    pub(super) fn record_turn_end(&mut self, read: Option<bool>, now: Instant) {
        self.observe(read, now);
    }

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

pub(super) fn probe(root: Option<u32>, max_age: Duration) -> Option<bool> {
    let Some(root) = root else {
        return Some(false);
    };
    let table = process_tree::snapshot_cached(max_age)?;
    Some(process_tree::has_background_shell(&table, root))
}

fn probe_containers(server: &str, key: &str) -> Option<bool> {
    let table = container_table(&program::sandbox_docker_run, server, key)?;
    Some(has_container_background_shell(&table))
}

impl TaskSessionState {
    pub(super) async fn poll_background_jobs(&mut self) {
        let root = self
            .acp
            .agent
            .client()
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
                let server = program::sandbox_docker_server();
                let key = self.worktree_path.as_deref().map(task_key);
                let read = tokio::task::spawn_blocking(move || match (server, key) {
                    (None, _) => probe(Some(pid), max_age),
                    (Some(server), Some(key)) => probe_containers(&server, &key),
                    (Some(_), None) => None,
                })
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
