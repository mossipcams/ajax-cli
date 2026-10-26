//! Fixtures are shaped like the live census in
//! `.planning/spikes/delegate-run-identity/census.md`: nono root, node sidecar,
//! harness `claude`, MCP servers, delegate chains and multicall `ugrep`.

use super::process_tree::{
    background_shells, cached_table, has_background_shell, is_shell, snapshot, ProcessEntry,
};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn entry(pid: u32, ppid: u32, comm: &str) -> ProcessEntry {
    ProcessEntry {
        pid,
        ppid,
        comm: comm.to_string(),
    }
}

fn table(rows: &[(u32, u32, &str)]) -> Vec<ProcessEntry> {
    rows.iter()
        .map(|&(pid, ppid, comm)| entry(pid, ppid, comm))
        .collect()
}

fn shell_pids(table: &[ProcessEntry], root: u32) -> Vec<u32> {
    background_shells(table, root)
        .iter()
        .map(|entry| entry.pid)
        .collect()
}

/// Claude SDK session: nono(10) -> node sidecar(11) + log(12); sidecar ->
/// harness claude(13) -> iwdp-mcp(14), codeburn node(15), opensafari node(16).
fn sdk_session_rows() -> Vec<(u32, u32, &'static str)> {
    vec![
        (10, 1, "nono"),
        (11, 10, "node"),
        (12, 10, "log"),
        (13, 11, "claude"),
        (14, 13, "iwdp-mcp"),
        (15, 13, "node"),
        (16, 13, "node"),
    ]
}

#[test]
fn sdk_session_without_shells_has_no_background_work() {
    let t = table(&sdk_session_rows());

    assert_eq!(shell_pids(&t, 10), Vec::<u32>::new());
}

#[test]
fn background_delegate_reports_only_its_top_shell() {
    let mut rows = sdk_session_rows();
    rows.extend([
        (20, 13, "/bin/zsh"),
        (21, 20, "Python"),
        (22, 21, "Python"),
        (23, 22, "node"),
        (24, 23, "node"),
        (25, 24, "claude"),
        (26, 25, "/bin/zsh"),
        (27, 26, "Python"),
    ]);
    let t = table(&rows);

    assert_eq!(shell_pids(&t, 10), vec![20]);
}

#[test]
fn monitor_job_reports_its_shell_not_the_multicall_grep() {
    let mut rows = sdk_session_rows();
    rows.extend([
        (30, 13, "zsh"),
        (31, 30, "/usr/bin/tail"),
        (32, 30, "claude"),
    ]);
    let t = table(&rows);

    assert_eq!(shell_pids(&t, 10), vec![30]);
}

#[test]
fn login_style_and_full_path_shells_are_recognised() {
    let t = table(&[
        (1, 0, "nono"),
        (2, 1, "-zsh"),
        (3, 1, "/bin/bash"),
        (4, 1, "/usr/local/bin/-fish"),
        (5, 1, "Python"),
    ]);

    assert_eq!(shell_pids(&t, 1), vec![2, 3, 4]);
}

#[test]
fn shell_under_shell_counts_once_as_the_top_shell() {
    let t = table(&[
        (1, 0, "nono"),
        (2, 1, "zsh"),
        (3, 2, "bash"),
        (4, 3, "sleep"),
    ]);

    assert_eq!(shell_pids(&t, 1), vec![2]);
}

#[test]
fn shells_in_another_sessions_subtree_are_excluded() {
    let t = table(&[
        (1, 0, "nono"),
        (2, 1, "zsh"),
        (100, 0, "nono"),
        (101, 100, "zsh"),
    ]);

    assert_eq!(shell_pids(&t, 1), vec![2]);
}

#[test]
fn root_that_is_a_shell_is_not_returned_but_its_shell_children_are() {
    let t = table(&[(1, 0, "zsh"), (2, 1, "zsh"), (3, 1, "node")]);

    assert_eq!(shell_pids(&t, 1), vec![2]);
}

#[test]
fn ppid_cycle_terminates() {
    let t = table(&[(1, 2, "nono"), (2, 1, "node"), (3, 1, "zsh")]);

    assert_eq!(shell_pids(&t, 1), vec![3]);
}

#[test]
fn results_are_sorted_by_pid() {
    let t = table(&[(1, 0, "nono"), (9, 1, "zsh"), (4, 1, "bash")]);

    assert_eq!(shell_pids(&t, 1), vec![4, 9]);
}

#[test]
fn has_background_shell_is_true_only_when_a_shell_hangs_under_the_root() {
    let with_shell = table(&[(1, 0, "nono"), (2, 1, "node"), (3, 2, "zsh")]);
    let shell_under_harness_only = table(&sdk_session_rows());
    let shell_in_other_session = table(&[(1, 0, "nono"), (100, 0, "nono"), (101, 100, "zsh")]);
    let root_is_only_shell = table(&[(1, 0, "zsh")]);

    assert!(has_background_shell(&with_shell, 1));
    assert!(!has_background_shell(&shell_under_harness_only, 10));
    assert!(!has_background_shell(&shell_in_other_session, 1));
    assert!(!has_background_shell(&root_is_only_shell, 1));
    assert!(!has_background_shell(&[], 1));
}

#[test]
fn is_shell_matches_basename_and_login_dash_only() {
    assert!(is_shell("zsh"));
    assert!(is_shell("-zsh"));
    assert!(is_shell("/bin/dash"));
    assert!(is_shell("/usr/local/bin/-fish"));
    assert!(is_shell("fish"));
    assert!(is_shell("sh"));
    assert!(!is_shell("claude"));
    assert!(!is_shell("/Users/matt/.local/bin/claude"));
    assert!(!is_shell(""));
}

#[test]
fn cached_table_reuses_the_same_arc_within_max_age() {
    let cache = Mutex::new(None);
    let base = Instant::now();
    let mut loads = 0;
    let mut load = || {
        loads += 1;
        Some(vec![entry(1, 0, "nono")])
    };

    let first = cached_table(&cache, base, Duration::from_secs(60), &mut load).expect("loaded");
    let second = cached_table(
        &cache,
        base + Duration::from_secs(59),
        Duration::from_secs(60),
        &mut load,
    )
    .expect("cached");

    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(loads, 1);
}

#[test]
fn cached_table_reloads_once_the_copy_is_older_than_max_age() {
    let cache = Mutex::new(None);
    let base = Instant::now();
    let mut loads = 0;
    let mut load = || {
        loads += 1;
        Some(vec![entry(1, 0, "nono")])
    };

    let first = cached_table(&cache, base, Duration::from_secs(2), &mut load).expect("loaded");
    let later = cached_table(
        &cache,
        base + Duration::from_secs(2),
        Duration::from_secs(2),
        &mut load,
    )
    .expect("reloaded");

    assert!(!std::sync::Arc::ptr_eq(&first, &later));
    assert_eq!(loads, 2);
}

#[test]
fn cached_table_does_not_cache_a_failed_snapshot() {
    let cache = Mutex::new(None);
    let base = Instant::now();
    let mut loads = 0;
    let mut failing = || {
        loads += 1;
        None
    };

    assert!(cached_table(&cache, base, Duration::from_secs(60), &mut failing).is_none());
    assert!(cached_table(&cache, base, Duration::from_secs(60), &mut failing).is_none());
    assert_eq!(loads, 2);

    let recovered = || Some(vec![entry(1, 0, "nono")]);
    assert!(cached_table(&cache, base, Duration::from_secs(60), recovered).is_some());
}

#[test]
fn snapshot_returns_table_containing_this_process() {
    let entries = snapshot().expect("the process table should be readable on this host");
    let own = std::process::id();

    assert!(!entries.is_empty());
    let own_entry = entries
        .iter()
        .find(|entry| entry.pid == own)
        .expect("this process must be in the table");
    assert_eq!(own_entry.ppid, std::os::unix::process::parent_id());
}

#[test]
fn snapshot_lists_a_spawned_shell_as_a_child_of_this_process() {
    // `read` is a shell builtin that blocks on stdin without forking, so the shell
    // stays a shell with no child process. A `sleep` grandchild would survive
    // kill() and keep the inherited handles open.
    let mut child = Command::new("sh")
        .args(["-c", "read _"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sh helper");
    let pid = child.id();

    let snapshot_result = snapshot();
    child.kill().expect("kill sh helper");
    child.wait().expect("reap sh helper");

    let entries = snapshot_result.expect("the process table should be readable on this host");
    let found = entries
        .iter()
        .find(|entry| entry.pid == pid)
        .expect("the spawned shell must be in the table");
    assert_eq!(found.ppid, std::process::id());
    assert!(
        is_shell(&found.comm),
        "comm {:?} must read as a shell",
        found.comm
    );
}
