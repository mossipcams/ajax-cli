//! Fixtures are shaped like the live census in
//! `.planning/spikes/delegate-run-identity/census.md`: nono root, node sidecar,
//! harness `claude`, MCP servers, delegate chains and multicall `ugrep`.

use super::process_tree::{background_shells, parse_ps, snapshot, ProcessEntry};

fn table(rows: &[(u32, u32, &str)]) -> Vec<ProcessEntry> {
    rows.iter()
        .map(|&(pid, ppid, comm)| ProcessEntry {
            pid,
            ppid,
            comm: comm.to_string(),
        })
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

    assert_eq!(background_shells(&t, 10), Vec::<u32>::new());
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

    assert_eq!(background_shells(&t, 10), vec![20]);
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

    assert_eq!(background_shells(&t, 10), vec![30]);
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

    assert_eq!(background_shells(&t, 1), vec![2, 3, 4]);
}

#[test]
fn shell_under_shell_counts_once_as_the_top_shell() {
    let t = table(&[
        (1, 0, "nono"),
        (2, 1, "zsh"),
        (3, 2, "bash"),
        (4, 3, "sleep"),
    ]);

    assert_eq!(background_shells(&t, 1), vec![2]);
}

#[test]
fn shells_in_another_sessions_subtree_are_excluded() {
    let t = table(&[
        (1, 0, "nono"),
        (2, 1, "zsh"),
        (100, 0, "nono"),
        (101, 100, "zsh"),
    ]);

    assert_eq!(background_shells(&t, 1), vec![2]);
}

#[test]
fn root_that_is_a_shell_is_not_returned_but_its_shell_children_are() {
    let t = table(&[(1, 0, "zsh"), (2, 1, "zsh"), (3, 1, "node")]);

    assert_eq!(background_shells(&t, 1), vec![2]);
}

#[test]
fn ppid_cycle_terminates() {
    let t = table(&[(1, 2, "nono"), (2, 1, "node"), (3, 1, "zsh")]);

    assert_eq!(background_shells(&t, 1), vec![3]);
}

#[test]
fn results_are_sorted_by_pid() {
    let t = table(&[(1, 0, "nono"), (9, 1, "zsh"), (4, 1, "bash")]);

    assert_eq!(background_shells(&t, 1), vec![4, 9]);
}

#[test]
fn parse_ps_handles_leading_spaces_spaced_comm_and_skips_garbage() {
    let output = "\
  1     0 /sbin/launchd
   42   1 /bin/zsh
garbage line
x 1 zsh
7 abc zsh
9 3 Google Chrome Helper (Renderer)
8 1
";

    let parsed = parse_ps(output);

    assert_eq!(
        parsed,
        table(&[
            (1, 0, "/sbin/launchd"),
            (42, 1, "/bin/zsh"),
            (9, 3, "Google Chrome Helper (Renderer)"),
        ])
    );
}

#[test]
fn snapshot_returns_table_containing_this_process() {
    let entries = snapshot().expect("ps -A should succeed on this host");
    let own = std::process::id();

    assert!(!entries.is_empty());
    assert!(entries.iter().any(|entry| entry.pid == own));
}
