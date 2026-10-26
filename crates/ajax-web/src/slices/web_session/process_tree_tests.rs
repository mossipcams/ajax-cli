//! Fixtures are shaped like the live census in
//! `.planning/spikes/delegate-run-identity/census.md`: nono root, node sidecar,
//! harness `claude`, MCP servers, delegate chains and multicall `ugrep`.

use super::process_tree::{background_shells, parse_ps, snapshot, ProcessEntry};

const START: &str = "Sat Oct 25 02:10:36 2026";

fn entry(pid: u32, ppid: u32, start: &str, comm: &str) -> ProcessEntry {
    ProcessEntry {
        pid,
        ppid,
        start: start.to_string(),
        comm: comm.to_string(),
    }
}

fn table(rows: &[(u32, u32, &str)]) -> Vec<ProcessEntry> {
    rows.iter()
        .map(|&(pid, ppid, comm)| entry(pid, ppid, START, comm))
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
fn background_shells_carry_the_start_time_from_the_snapshot() {
    let t = vec![
        entry(1, 0, START, "nono"),
        entry(20, 1, "Sun Oct 26 09:00:00 2026", "zsh"),
    ];

    let shells = background_shells(&t, 1);

    assert_eq!(shells.len(), 1);
    assert_eq!(shells[0].start, "Sun Oct 26 09:00:00 2026");
}

#[test]
fn parse_ps_handles_leading_spaces_spaced_comm_and_skips_garbage() {
    let output = "\
  1     0 Sat Oct 25 02:10:36 2026 /sbin/launchd
   42   1 Sat Oct 25 02:10:37 2026 /bin/zsh
garbage line
x 1 Sat Oct 25 02:10:36 2026 zsh
7 abc Sat Oct 25 02:10:36 2026 zsh
9 3 Sat Oct 25 02:10:38 2026 Google Chrome Helper (Renderer)
8 1
";

    let parsed = parse_ps(output);

    assert_eq!(
        parsed,
        vec![
            entry(1, 0, "Sat Oct 25 02:10:36 2026", "/sbin/launchd"),
            entry(42, 1, "Sat Oct 25 02:10:37 2026", "/bin/zsh"),
            entry(
                9,
                3,
                "Sat Oct 25 02:10:38 2026",
                "Google Chrome Helper (Renderer)"
            ),
        ]
    );
}

#[test]
fn parse_ps_collapses_a_padded_day_and_keeps_spaces_in_the_command() {
    let parsed =
        parse_ps("  42   1 Sat Oct  5 02:10:36 2026 /Applications/Google Chrome.app/Helper\n");

    assert_eq!(
        parsed,
        vec![entry(
            42,
            1,
            "Sat Oct 5 02:10:36 2026",
            "/Applications/Google Chrome.app/Helper"
        )]
    );
}

#[test]
fn parse_ps_skips_lines_with_six_tokens_or_no_command() {
    let parsed = parse_ps(
        "8 1 Sat Oct 25 02:10:36\n\
         9 1 Sat Oct 25 02:10:36 2026\n\
         10 1 Sat Oct 25 02:10:36 2026   \n",
    );

    assert_eq!(parsed, Vec::new());
}

#[test]
fn snapshot_returns_table_containing_this_process() {
    let entries = snapshot().expect("ps -A should succeed on this host");
    let own = std::process::id();

    assert!(!entries.is_empty());
    assert!(entries.iter().any(|entry| entry.pid == own));
}
