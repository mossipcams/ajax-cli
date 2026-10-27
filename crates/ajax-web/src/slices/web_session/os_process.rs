#[cfg(target_os = "macos")]
fn bsd_info(pid: i32) -> Option<nix::libc::proc_bsdinfo> {
    let mut info: nix::libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<nix::libc::proc_bsdinfo>() as nix::libc::c_int;
    let written = unsafe {
        nix::libc::proc_pidinfo(
            pid,
            nix::libc::PROC_PIDTBSDINFO,
            0,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        )
    };
    (written == size).then_some(info)
}

#[cfg(target_os = "macos")]
pub(super) fn start_token(pid: i64) -> Option<String> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let info = bsd_info(pid)?;
    Some(format!(
        "{}.{:06}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    ))
}

#[cfg(target_os = "macos")]
fn pid_count() -> Option<usize> {
    let count = unsafe { nix::libc::proc_listallpids(std::ptr::null_mut(), 0) };
    usize::try_from(count).ok().filter(|count| *count > 0)
}

#[cfg(target_os = "macos")]
fn list_pids() -> Option<Vec<i32>> {
    const HEADROOM: usize = 64;
    let mut capacity = pid_count()? + HEADROOM;
    for _ in 0..2 {
        let mut pids = vec![0i32; capacity];
        let bytes = nix::libc::c_int::try_from(capacity * std::mem::size_of::<i32>()).ok()?;
        let listed = unsafe { nix::libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        let listed = usize::try_from(listed).ok().filter(|listed| *listed > 0)?;
        if listed < capacity {
            pids.truncate(listed);
            return Some(pids);
        }
        capacity = pid_count()? + HEADROOM;
    }
    None
}

#[cfg(target_os = "macos")]
pub(super) fn table() -> Option<Vec<(u32, u32, String)>> {
    let rows = list_pids()?
        .into_iter()
        .filter_map(|pid| {
            let info = bsd_info(pid)?;
            let name = info.pbi_comm;
            let bytes: Vec<u8> = name
                .iter()
                .take_while(|byte| **byte != 0)
                .map(|byte| *byte as u8)
                .collect();
            Some((
                u32::try_from(pid).ok()?,
                info.pbi_ppid,
                String::from_utf8_lossy(&bytes).into_owned(),
            ))
        })
        .collect();
    Some(rows)
}

#[cfg(target_os = "macos")]
const P_STAT_OFFSET: usize = 36;

#[cfg(target_os = "macos")]
pub(super) fn is_zombie(pid: i64) -> bool {
    let Some(pid) = i32::try_from(pid).ok().filter(|pid| *pid > 0) else {
        return false;
    };
    let mut mib = [
        nix::libc::CTL_KERN,
        nix::libc::KERN_PROC,
        nix::libc::KERN_PROC_PID,
        pid,
    ];
    let mut buf = [0u8; 1024];
    let mut len = buf.len();
    let rc = unsafe {
        nix::libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as nix::libc::c_uint,
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    rc == 0 && len > P_STAT_OFFSET && u32::from(buf[P_STAT_OFFSET]) == nix::libc::SZOMB
}

#[cfg(target_os = "linux")]
fn proc_stat_after_comm(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.get(stat.rfind(')')? + 1..)?;
    Some(after_comm.to_string())
}

#[cfg(target_os = "linux")]
pub(super) fn start_token(pid: i64) -> Option<String> {
    let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let boot_id = boot_id.trim();
    if boot_id.is_empty() {
        return None;
    }
    let after_comm = proc_stat_after_comm(pid)?;
    let starttime = after_comm.split_whitespace().nth(19)?;
    Some(format!("{boot_id}:{starttime}"))
}

#[cfg(target_os = "linux")]
pub(super) fn is_zombie(pid: i64) -> bool {
    proc_stat_after_comm(pid)
        .is_some_and(|after_comm| after_comm.split_whitespace().next() == Some("Z"))
}

#[cfg(target_os = "linux")]
fn linux_row(pid: u32) -> Option<(u32, u32, String)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_string();
    let ppid = stat
        .get(close + 1..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((pid, ppid, comm))
}

#[cfg(target_os = "linux")]
pub(super) fn table() -> Option<Vec<(u32, u32, String)>> {
    let entries = std::fs::read_dir("/proc").ok()?;
    let rows = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_str()?;
            if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            linux_row(name.parse().ok()?)
        })
        .collect();
    Some(rows)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn start_token(_pid: i64) -> Option<String> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn is_zombie(_pid: i64) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn table() -> Option<Vec<(u32, u32, String)>> {
    None
}

#[cfg(unix)]
pub(super) fn exists(pid: i64) -> bool {
    let Some(pid) = i32::try_from(pid).ok().filter(|pid| *pid > 0) else {
        return false;
    };
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None) {
        Ok(()) => true,
        Err(nix::errno::Errno::ESRCH) => false,
        Err(_) => true,
    }
}

#[cfg(not(unix))]
pub(super) fn exists(_pid: i64) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use super::{exists, is_zombie, start_token};

    #[test]
    fn live_child_token_is_stable_and_gone_after_exit() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        let pid = child.id() as i64;

        let first = start_token(pid).expect("live child must have a start token");
        let second = start_token(pid).expect("live child must still have a token");
        assert_eq!(
            first, second,
            "the token must be stable while the process lives"
        );

        child.kill().expect("kill sleep helper");
        child.wait().expect("reap sleep helper");

        assert_eq!(
            start_token(pid),
            None,
            "a reaped process must have no token"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn live_child_token_starts_with_the_boot_id() {
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .expect("the boot id must be readable on linux");
        let prefix = format!("{}:", boot_id.trim());
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        let pid = child.id() as i64;

        let token = start_token(pid);
        child.kill().expect("kill sleep helper");
        child.wait().expect("reap sleep helper");

        let token = token.expect("live child must have a start token");
        assert!(
            token.starts_with(&prefix),
            "token {token:?} must start with the boot id {prefix:?}"
        );
    }

    #[test]
    fn non_positive_pids_have_no_token() {
        assert_eq!(start_token(0), None);
        assert_eq!(start_token(-1), None);
    }

    #[test]
    fn exists_tracks_a_child_from_spawn_to_reap() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        let pid = child.id() as i64;

        assert!(exists(pid), "a live child must exist");

        child.kill().expect("kill sleep helper");
        child.wait().expect("reap sleep helper");

        assert!(!exists(pid), "a reaped child must not exist");
    }

    #[test]
    fn non_positive_pids_do_not_exist() {
        assert!(!exists(0));
        assert!(!exists(-1));
    }

    #[test]
    fn live_child_is_not_a_zombie() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn sleep helper");
        let pid = child.id() as i64;

        assert!(!is_zombie(pid), "a running child must not read as a zombie");

        child.kill().expect("kill sleep helper");
        child.wait().expect("reap sleep helper");
    }

    #[test]
    fn exited_unreaped_child_becomes_a_zombie() {
        let mut child = Command::new("true")
            .stdin(Stdio::null())
            .spawn()
            .expect("spawn true helper");
        let pid = child.id() as i64;

        let deadline = Instant::now() + Duration::from_secs(5);
        while !is_zombie(pid) {
            assert!(
                Instant::now() < deadline,
                "an exited, unreaped child must become a zombie"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(exists(pid), "a zombie still exists");

        child.wait().expect("reap true helper");
    }
}
