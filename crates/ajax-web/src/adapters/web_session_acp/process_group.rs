//! Process-group helpers: harness children run as their own group leader so everything they spawn dies with them (issue #1298).
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

/// Make the child spawned from `command` the leader of a new process group.
pub(crate) fn own_group(command: &mut Command) -> &mut Command {
    command.process_group(0)
}

/// Spawn `command` as the leader of its own process group.
pub(crate) fn spawn_leader(command: &mut Command) -> std::io::Result<std::process::Child> {
    own_group(command).spawn()
}

/// Kill the child's whole process group, then the child itself as a fallback.
pub(crate) fn kill_tree(child: &mut std::process::Child) {
    kill_group(child.id());
    let _ = child.kill();
}

/// SIGKILL the whole process group led by `leader_pid`. True when the signal was delivered.
// ponytail: shells out to kill(1) like orphan_sweep does; use nix::sys::signal::killpg if nix becomes an ajax-web dependency.
pub(crate) fn kill_group(leader_pid: u32) -> bool {
    Command::new("kill")
        .args(["-KILL", "--", &format!("-{leader_pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
