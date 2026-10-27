use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime},
};

use ajax_core::models::AgentClient;

use super::client::{set_test_acp_command, with_test_handshake_timeout, AcpStdioClient};

fn pid_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", "--", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn pid_alive_until(pid: u32, deadline: Instant) -> bool {
    loop {
        if !pid_alive(pid) {
            return false;
        }
        if Instant::now() >= deadline {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn acp_spawn_failure_kills_grandchildren_of_the_acp_child() {
    let dir = std::env::temp_dir().join(format!(
        "ajax-acp-pg-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");

    let pidfile = dir.join("grandchild.pid");
    let script = r#"sleep 300 & echo $! > "$1"; exec sleep 300"#;
    set_test_acp_command(Some((
        Path::new("sh"),
        &["-c", script, "sh", pidfile.to_str().unwrap()],
    )));

    let spawned = with_test_handshake_timeout(300, || {
        AcpStdioClient::spawn(AgentClient::Codex, &dir, None, None)
    });
    set_test_acp_command(None);

    match spawned {
        Err(_err) => {}
        Ok(_) => {
            panic!("spawn must fail when the ACP child never answers the handshake (issue #1298)")
        }
    }

    let mut pid: Option<u32> = None;
    let read_deadline = Instant::now() + Duration::from_secs(2);
    while pid.is_none() && Instant::now() < read_deadline {
        if let Ok(text) = std::fs::read_to_string(&pidfile) {
            if let Ok(value) = text.trim().parse::<u32>() {
                pid = Some(value);
            }
        }
        if pid.is_none() {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    let pid = pid.expect("the fake ACP child must have written its grandchild's pid (issue #1298)");

    let still_alive = pid_alive_until(pid, Instant::now() + Duration::from_secs(3));
    if still_alive {
        let _ = Command::new("kill")
            .args(["-9", "--", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    assert!(
        !still_alive,
        "grandchild {pid} of the killed ACP child survived a failed spawn — \
         the ACP child must run in its own process group (issue #1298)"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
