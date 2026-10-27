use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use super::os_process;

const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "fish"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProcessEntry {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) comm: String,
}

pub(super) fn snapshot() -> Option<Vec<ProcessEntry>> {
    let rows = os_process::table()?;
    Some(
        rows.into_iter()
            .map(|(pid, ppid, comm)| ProcessEntry { pid, ppid, comm })
            .collect(),
    )
}

type SnapshotCache = Mutex<Option<(Instant, Arc<Vec<ProcessEntry>>)>>;

static SNAPSHOT_CACHE: SnapshotCache = Mutex::new(None);

pub(super) fn snapshot_cached(max_age: Duration) -> Option<Arc<Vec<ProcessEntry>>> {
    cached_table(&SNAPSHOT_CACHE, Instant::now(), max_age, snapshot)
}

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

pub(super) fn has_background_shell(table: &[ProcessEntry], root: u32) -> bool {
    !background_shells(table, root).is_empty()
}

pub(super) fn is_shell(comm: &str) -> bool {
    let base = comm.rsplit_once('/').map_or(comm, |(_, base)| base);
    let name = base.strip_prefix('-').unwrap_or(base);
    SHELLS.contains(&name)
}
