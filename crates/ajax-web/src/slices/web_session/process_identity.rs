use super::os_process;

pub(super) fn start_token(pid: i64) -> Option<String> {
    if pid <= 0 {
        return None;
    }
    os_process::start_token(pid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Verdict {
    Gone,
    Reused,
    Same,
}

#[allow(dead_code)]
pub(super) fn judge(recorded: &str, live: Option<&str>) -> Verdict {
    match live {
        None => Verdict::Gone,
        Some(live) if live == recorded => Verdict::Same,
        Some(_) => Verdict::Reused,
    }
}

pub(super) fn foreign_live_child(
    child: Option<(i64, i64)>,
    child_started: Option<&str>,
    owner_boot_id: Option<&str>,
    host_boot_id: &str,
) -> Option<i64> {
    let (pid, _) = child?;
    let owner = owner_boot_id?;
    if owner == host_boot_id {
        return None;
    }
    let recorded = child_started?;
    if judge(recorded, start_token(pid).as_deref()) == Verdict::Same {
        Some(pid)
    } else {
        None
    }
}
