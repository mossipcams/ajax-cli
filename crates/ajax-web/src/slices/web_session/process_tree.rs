use std::collections::{HashMap, HashSet, VecDeque};
use std::io;
use std::process::Output;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ajax_core::adapters::docker_sandbox::{parse_ps, ps_argv, top_argv};

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

pub(super) fn docker_text(
    run: &dyn Fn(&[&str]) -> io::Result<Output>,
    argv: &[String],
) -> Option<String> {
    let args: Vec<&str> = argv.iter().map(String::as_str).collect();
    let output = run(&args).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn container_table(
    run: &dyn Fn(&[&str]) -> io::Result<Output>,
    server: &str,
    key: &str,
) -> Option<Vec<ProcessEntry>> {
    let listing = docker_text(run, &ps_argv(server, key))?;
    let mut table = Vec::new();
    for row in parse_ps(&listing) {
        let processes = docker_text(run, &top_argv(&row.id))?;
        table.extend(parse_top(&processes));
    }
    Some(table)
}

pub(super) fn parse_top(output: &str) -> Vec<ProcessEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let ppid = fields.next()?.parse().ok()?;
            let comm = fields.collect::<Vec<_>>().join(" ");
            (!comm.is_empty()).then_some(ProcessEntry { pid, ppid, comm })
        })
        .collect()
}

pub(super) fn has_container_background_shell(table: &[ProcessEntry]) -> bool {
    let pids: HashSet<u32> = table.iter().map(|entry| entry.pid).collect();
    table
        .iter()
        .filter(|entry| !pids.contains(&entry.ppid))
        .any(|root| has_background_shell(table, root.pid))
}

pub(super) fn is_shell(comm: &str) -> bool {
    let base = comm.rsplit_once('/').map_or(comm, |(_, base)| base);
    let name = base.strip_prefix('-').unwrap_or(base);
    SHELLS.contains(&name)
}
