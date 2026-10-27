use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

pub(crate) fn own_group(command: &mut Command) -> &mut Command {
    command.process_group(0)
}

pub(crate) fn spawn_leader(command: &mut Command) -> std::io::Result<std::process::Child> {
    own_group(command).spawn()
}

pub(crate) fn kill_tree(child: &mut std::process::Child) {
    kill_group(child.id());
    let _ = child.kill();
}

pub(crate) fn kill_group(leader_pid: u32) -> bool {
    Command::new("kill")
        .args(["-KILL", "--", &format!("-{leader_pid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
