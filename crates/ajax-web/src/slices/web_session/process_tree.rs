//! Background-shell detection for Ajax Chat sessions.
//!
//! A background job (a router delegate, a Monitor watcher, a build) started by
//! a harness runs under a shell that is a descendant of the session's process
//! tree. This module reads the kernel process table and finds those topmost
//! shells. It never identifies the harness or its binaries, so it works whichever
//! agent or router started the job, and multicall binaries (Claude's grep runs as
//! `claude` with argv `ugrep`) cannot be mistaken for a job.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::os_process;

// ponytail: shells are recognised by basename against this fixed list. An exotic
// shell (nushell, ksh, tcsh) is missed, and a harness that keeps an idle
// persistent shell as a direct child reads as background work. See the plan's
// Ceilings in .planning/agent-plans/delegate-run-identity.md.
const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "fish"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcessEntry {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) comm: String,
}

/// Take one process-table snapshot through [`os_process::table`].
///
/// Returns `None` only when the process table cannot be listed.
pub(super) fn snapshot() -> Option<Vec<ProcessEntry>> {
    let rows = os_process::table()?;
    Some(
        rows.into_iter()
            .map(|(pid, ppid, comm)| ProcessEntry { pid, ppid, comm })
            .collect(),
    )
}

// ponytail: one cached table serves every session. The global lock serialises
// the process-table reads, so N sessions polling in the same window cost one
// read, and per-session caches would add nothing.
type SnapshotCache = Mutex<Option<(Instant, Arc<Vec<ProcessEntry>>)>>;

static SNAPSHOT_CACHE: SnapshotCache = Mutex::new(None);

/// The process table, reused while the cached copy is younger than `max_age`.
/// A failed snapshot is not cached, so the next call retries `ps`.
pub(super) fn snapshot_cached(max_age: Duration) -> Option<Arc<Vec<ProcessEntry>>> {
    cached_table(&SNAPSHOT_CACHE, Instant::now(), max_age, snapshot)
}

/// The cache rule behind [`snapshot_cached`], with the clock and loader injected.
pub(super) fn cached_table(
    cache: &SnapshotCache,
    now: Instant,
    max_age: Duration,
    load: impl FnOnce() -> Option<Vec<ProcessEntry>>,
) -> Option<Arc<Vec<ProcessEntry>>> {
    let mut guard = cache.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((taken, table)) = guard.as_ref() {
        if now.saturating_duration_since(*taken) < max_age {
            return Some(Arc::clone(table));
        }
    }
    let table = Arc::new(load()?);
    *guard = Some((now, Arc::clone(&table)));
    Some(table)
}

/// Return the entries of every shell in the strict subtree of `root` that has no
/// shell ancestor between it and `root`, sorted ascending by pid.
///
/// `root` is never returned, even when it is a shell. A ppid cycle terminates
/// because each pid is visited at most once.
pub(super) fn background_shells(table: &[ProcessEntry], root: u32) -> Vec<&ProcessEntry> {
    let mut children: HashMap<u32, Vec<&ProcessEntry>> = HashMap::new();
    for entry in table {
        children.entry(entry.ppid).or_default().push(entry);
    }

    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut found = Vec::new();
    while let Some(pid) = queue.pop_front() {
        for &child in children.get(&pid).into_iter().flatten() {
            if !visited.insert(child.pid) {
                continue;
            }
            if is_shell(&child.comm) {
                found.push(child);
            } else {
                queue.push_back(child.pid);
            }
        }
    }
    found.sort_unstable_by_key(|entry| entry.pid);
    found
}

/// True when `table` has at least one background shell under `root`.
pub(super) fn has_background_shell(table: &[ProcessEntry], root: u32) -> bool {
    !background_shells(table, root).is_empty()
}

/// True when the basename of `comm` (any leading `-` stripped, so login shells
/// such as `-zsh` match) is one of [`SHELLS`].
pub(super) fn is_shell(comm: &str) -> bool {
    let base = comm.rsplit_once('/').map_or(comm, |(_, base)| base);
    let name = base.strip_prefix('-').unwrap_or(base);
    SHELLS.contains(&name)
}
