//! In-process process probes, read from the kernel without spawning `ps`.
//!
//! `ps` is setuid, so the nono sandbox refuses to exec it; reading the start
//! time and state directly keeps the identity guard and the orphan sweep
//! working inside the sandbox.

/// The BSD info record for `pid`, or `None` when the kernel returns no full
/// record (exited, zombie, or denied).
#[cfg(target_os = "macos")]
fn bsd_info(pid: i32) -> Option<nix::libc::proc_bsdinfo> {
    // SAFETY: proc_bsdinfo is plain C data, so all-zero bytes are a valid value.
    let mut info: nix::libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<nix::libc::proc_bsdinfo>() as nix::libc::c_int;
    // SAFETY: `info` is a live, writable proc_bsdinfo and `size` is exactly its
    // byte length, so the kernel writes at most `size` bytes into it.
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

/// Start-time token for `pid`, or `None` when `pid` is not positive, no such
/// process exists, or the platform has no probe.
#[cfg(target_os = "macos")]
pub(super) fn start_token(pid: i64) -> Option<String> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let info = bsd_info(pid)?;
    Some(format!(
        "{}.{:06}",
        info.pbi_start_tvsec, info.pbi_start_tvusec
    ))
}

/// Number of pids the kernel reports, from a size-zero `proc_listallpids` call.
#[cfg(target_os = "macos")]
fn pid_count() -> Option<usize> {
    // SAFETY: a null buffer with size 0 is the documented way to ask for the
    // count only; the kernel writes nothing.
    let count = unsafe { nix::libc::proc_listallpids(std::ptr::null_mut(), 0) };
    usize::try_from(count).ok().filter(|count| *count > 0)
}

/// Every pid the kernel lists, sized from a count call with headroom. A buffer
/// that fills is retried once with a larger one; a second full buffer gives up.
#[cfg(target_os = "macos")]
fn list_pids() -> Option<Vec<i32>> {
    const HEADROOM: usize = 64;
    let mut capacity = pid_count()? + HEADROOM;
    for _ in 0..2 {
        let mut pids = vec![0i32; capacity];
        let bytes = nix::libc::c_int::try_from(capacity * std::mem::size_of::<i32>()).ok()?;
        // SAFETY: `pids` is a live Vec of `capacity` i32s and `bytes` is exactly
        // its byte length, so the kernel writes at most `bytes` bytes into it.
        let listed = unsafe { nix::libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        // libproc reports a failed listing as 0, not -1, and a real table is never
        // empty (launchd and this process are always present), so 0 or less is failure.
        let listed = usize::try_from(listed).ok().filter(|listed| *listed > 0)?;
        if listed < capacity {
            pids.truncate(listed);
            return Some(pids);
        }
        capacity = pid_count()? + HEADROOM;
    }
    None
}

/// The process table: (pid, ppid, comm) for every process whose BSD info the
/// kernel returns. Zombies and exited or denied processes are skipped, and
/// `None` means the pid list itself could not be read.
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

/// Byte offset of `p_stat` in the kernel's `kinfo_proc` (sys/sysctl.h). The
/// embedded `extern_proc` comes first: 16 bytes of union, `p_vmspace`,
/// `p_sigacts`, `p_flag`, then `p_stat`.
#[cfg(target_os = "macos")]
const P_STAT_OFFSET: usize = 36;

/// Whether `pid` is an exited process still waiting to be reaped. `proc_pidinfo`
/// returns no record for a zombie, so this reads the kernel process table
/// through `sysctl(KERN_PROC_PID)`, the same source `ps` reports `Z` from.
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
    // kinfo_proc is well under this size; a larger buffer lets the kernel report
    // how many bytes it actually wrote through `len`.
    let mut buf = [0u8; 1024];
    let mut len = buf.len();
    // SAFETY: `mib` names four ints, and `buf`/`len` describe a live writable
    // buffer of `len` bytes. No new value is set (null `newp`), so nothing is written
    // through the kernel's input side.
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
    // A missing process yields success with no bytes written.
    rc == 0 && len > P_STAT_OFFSET && u32::from(buf[P_STAT_OFFSET]) == nix::libc::SZOMB
}

/// The text of `/proc/<pid>/stat` after the parenthesised comm field. comm
/// may contain spaces or ')', so fields are counted from after the last ')';
/// the first of them is the state.
#[cfg(target_os = "linux")]
fn proc_stat_after_comm(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.get(stat.rfind(')')? + 1..)?;
    Some(after_comm.to_string())
}

/// Start-time token for `pid`: `{boot_id}:{starttime}`. `starttime` is field 22
/// of `/proc/<pid>/stat`, counted in clock ticks since boot, so the boot id is
/// what keeps a token from a previous boot from matching a reused pid. An
/// unreadable boot id yields `None`, so an unverifiable process is never signalled.
#[cfg(target_os = "linux")]
pub(super) fn start_token(pid: i64) -> Option<String> {
    let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
    let boot_id = boot_id.trim();
    if boot_id.is_empty() {
        return None;
    }
    let after_comm = proc_stat_after_comm(pid)?;
    // Field 3 (state) is the first field after comm, which makes starttime
    // (field 22) index 19.
    let starttime = after_comm.split_whitespace().nth(19)?;
    Some(format!("{boot_id}:{starttime}"))
}

/// Whether `pid` is an exited process still waiting to be reaped (state `Z`).
#[cfg(target_os = "linux")]
pub(super) fn is_zombie(pid: i64) -> bool {
    proc_stat_after_comm(pid)
        .is_some_and(|after_comm| after_comm.split_whitespace().next() == Some("Z"))
}

/// The (pid, ppid, comm) row for one `/proc/<pid>/stat`, or `None` when the entry
/// vanished or is unreadable. comm sits between the first '(' and the last ')'.
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

/// The process table read from `/proc`, one row per numeric entry that can be
/// read. `None` means `/proc` itself could not be listed.
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

/// Start-time probing is not implemented on this platform.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn start_token(_pid: i64) -> Option<String> {
    None
}

/// Zombie probing is not implemented on this platform, so nothing reads as one.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn is_zombie(_pid: i64) -> bool {
    false
}

/// Process-table probing is not implemented on this platform.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn table() -> Option<Vec<(u32, u32, String)>> {
    None
}

/// Whether a process with this pid exists. A zombie exists here: `kill(pid, 0)`
/// succeeds for an exited but unreaped process. Only ESRCH is positive evidence
/// of death; EPERM and any other error keep the process counted as existing.
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

/// Existence probing is not implemented on this platform, so nothing reads as dead.
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
