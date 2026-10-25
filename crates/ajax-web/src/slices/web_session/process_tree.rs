//! Background-shell detection for Ajax Chat sessions.
//!
//! A background job (a router delegate, a Monitor watcher, a build) started by
//! a harness runs under a shell that is a descendant of the session's process
//! tree. This module takes one `ps` snapshot and finds those topmost shells. It
//! never identifies the harness or its binaries, so it works whichever agent or
//! router started the job, and multicall binaries (Claude's grep runs as
//! `claude` with argv `ugrep`) cannot be mistaken for a job.
//!
//! Nothing calls this module yet. Slice 2 of issue #1412 wires it into the
//! session tick, so the public items allow dead code until then.

use std::collections::{HashMap, HashSet, VecDeque};
use std::process::{Command, Stdio};

// ponytail: shells are recognised by basename against this fixed list. An exotic
// shell (nushell, ksh, tcsh) is missed, and a harness that keeps an idle
// persistent shell as a direct child reads as background work. See the plan's
// Ceilings in .planning/agent-plans/delegate-run-identity.md.
const SHELLS: [&str; 5] = ["sh", "bash", "zsh", "dash", "fish"];

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) struct ProcessEntry {
    pub(super) pid: u32,
    pub(super) ppid: u32,
    pub(super) comm: String,
}

/// Take one process-table snapshot via `ps -A -o pid=,ppid=,comm=`.
///
/// Returns `Some(parsed table)` only when the command ran and exited 0; every
/// other outcome is `None`.
#[allow(dead_code)]
pub(super) fn snapshot() -> Option<Vec<ProcessEntry>> {
    let output = Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,comm="])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(parse_ps(&stdout))
}

/// Parse `ps -o pid=,ppid=,comm=` output. Lines that do not start with two
/// unsigned integers followed by a non-empty command are skipped.
#[allow(dead_code)]
pub(super) fn parse_ps(output: &str) -> Vec<ProcessEntry> {
    output.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<ProcessEntry> {
    let (pid, rest) = split_token(line)?;
    let (ppid, comm) = split_token(rest)?;
    let comm = comm.trim();
    if comm.is_empty() {
        return None;
    }
    Some(ProcessEntry {
        pid: pid.parse().ok()?,
        ppid: ppid.parse().ok()?,
        comm: comm.to_string(),
    })
}

/// Split off the first whitespace-delimited token, returning it with the rest
/// of the line. Leading whitespace is ignored.
fn split_token(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let end = line.find(char::is_whitespace).unwrap_or(line.len());
    let (token, rest) = line.split_at(end);
    if token.is_empty() {
        None
    } else {
        Some((token, rest))
    }
}

/// Return the pids of every shell in the strict subtree of `root` that has no
/// shell ancestor between it and `root`, sorted ascending.
///
/// `root` is never returned, even when it is a shell. A ppid cycle terminates
/// because each pid is visited at most once.
#[allow(dead_code)]
pub(super) fn background_shells(table: &[ProcessEntry], root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<&ProcessEntry>> = HashMap::new();
    for entry in table {
        children.entry(entry.ppid).or_default().push(entry);
    }

    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    let mut found = Vec::new();
    while let Some(pid) = queue.pop_front() {
        for child in children.get(&pid).into_iter().flatten() {
            if !visited.insert(child.pid) {
                continue;
            }
            if is_shell(&child.comm) {
                found.push(child.pid);
            } else {
                queue.push_back(child.pid);
            }
        }
    }
    found.sort_unstable();
    found
}

/// True when the basename of `comm` (any leading `-` stripped, so login shells
/// such as `-zsh` match) is one of [`SHELLS`].
fn is_shell(comm: &str) -> bool {
    let base = comm.rsplit_once('/').map_or(comm, |(_, base)| base);
    let name = base.strip_prefix('-').unwrap_or(base);
    SHELLS.contains(&name)
}
