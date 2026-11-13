use super::process_tree::{
    background_shells, cached_table, container_table, has_background_shell,
    has_container_background_shell, is_shell, parse_top, snapshot, ProcessEntry,
};
use std::cell::RefCell;
use std::io;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Output, Stdio};
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

fn sdk_session_rows() -> Vec<(u32, u32, &'static str)> {
    vec![
        (10, 1, "docker"),
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
        (1, 0, "docker"),
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
        (1, 0, "docker"),
        (2, 1, "zsh"),
        (3, 2, "bash"),
        (4, 3, "sleep"),
    ]);

    assert_eq!(shell_pids(&t, 1), vec![2]);
}

#[test]
fn shells_in_another_sessions_subtree_are_excluded() {
    let t = table(&[
        (1, 0, "docker"),
        (2, 1, "zsh"),
        (100, 0, "docker"),
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
    let t = table(&[(1, 2, "docker"), (2, 1, "node"), (3, 1, "zsh")]);

    assert_eq!(shell_pids(&t, 1), vec![3]);
}

#[test]
fn results_are_sorted_by_pid() {
    let t = table(&[(1, 0, "docker"), (9, 1, "zsh"), (4, 1, "bash")]);

    assert_eq!(shell_pids(&t, 1), vec![4, 9]);
}

#[test]
fn has_background_shell_is_true_only_when_a_shell_hangs_under_the_root() {
    let with_shell = table(&[(1, 0, "docker"), (2, 1, "node"), (3, 2, "zsh")]);
    let shell_under_harness_only = table(&sdk_session_rows());
    let shell_in_other_session = table(&[(1, 0, "docker"), (100, 0, "docker"), (101, 100, "zsh")]);
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
        Some(vec![entry(1, 0, "docker")])
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
        Some(vec![entry(1, 0, "docker")])
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

    let recovered = || Some(vec![entry(1, 0, "docker")]);
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

fn docker_output(stdout: &str, code: i32) -> Output {
    Output {
        status: ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

#[test]
fn parse_top_skips_the_header_and_reads_host_pids() {
    let entries = parse_top(
        "PID PPID COMMAND\n  101     5 tini\n  102   101 claude\n  103   102 /bin/bash\n",
    );

    assert_eq!(
        entries,
        vec![
            entry(101, 5, "tini"),
            entry(102, 101, "claude"),
            entry(103, 102, "/bin/bash"),
        ]
    );
}

#[test]
fn container_table_reads_top_of_every_container_for_the_task() {
    let calls = RefCell::new(Vec::new());
    let run = |args: &[&str]| -> io::Result<Output> {
        calls.borrow_mut().push(args.join(" "));
        if args[0] == "ps" {
            return Ok(docker_output("c1 boot-1 srv\nc2 boot-1 srv\n", 0));
        }
        match args[1] {
            "c1" => Ok(docker_output("PID PPID COMMAND\n 101 5 tini\n", 0)),
            _ => Ok(docker_output(
                "PID PPID COMMAND\n 201 7 tini\n 202 201 zsh\n",
                0,
            )),
        }
    };

    let table = container_table(&run, "srv", "key").expect("readable");

    assert_eq!(
        table,
        vec![
            entry(101, 5, "tini"),
            entry(201, 7, "tini"),
            entry(202, 201, "zsh")
        ]
    );
    assert_eq!(
        calls.into_inner(),
        vec![
            "ps --no-trunc --filter label=ajax.server=srv --filter label=ajax.task_key=key --format {{.ID}} {{.Label \"ajax.boot\"}} {{.Label \"ajax.server\"}}",
            "top c1 -eo pid,ppid,comm",
            "top c2 -eo pid,ppid,comm",
        ]
    );
}

#[test]
fn container_table_is_unknown_when_docker_fails() {
    let daemon_down = |_: &[&str]| -> io::Result<Output> { Err(io::Error::other("no socket")) };
    assert_eq!(container_table(&daemon_down, "srv", "key"), None);

    let list_fails = |_: &[&str]| -> io::Result<Output> { Ok(docker_output("", 1)) };
    assert_eq!(container_table(&list_fails, "srv", "key"), None);

    let top_fails = |args: &[&str]| -> io::Result<Output> {
        if args[0] == "ps" {
            Ok(docker_output("c1 boot-1 srv\n", 0))
        } else {
            Ok(docker_output("", 1))
        }
    };
    assert_eq!(container_table(&top_fails, "srv", "key"), None);
}

#[test]
fn container_background_shell_is_found_under_the_init_process() {
    let with_shell = table(&[(101, 5, "tini"), (102, 101, "claude"), (103, 102, "bash")]);
    let without_shell = table(&[(101, 5, "tini"), (102, 101, "claude"), (104, 102, "node")]);

    assert!(has_container_background_shell(&with_shell));
    assert!(!has_container_background_shell(&without_shell));
    assert!(!has_container_background_shell(&[]));
}
