use super::SessionStorage;
use ajax_core::registry::OwnerClaim;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Only one Ajax web host process may run chat agents against a given registry.
pub(crate) struct ExecutionOwnership {
    storage: SessionStorage,
    boot_id: String,
    lost: AtomicBool,
    consecutive_heartbeat_failures: AtomicU32,
}

/// How often the runtime task renews the execution owner lease.
pub(crate) const HEARTBEAT_EVERY: Duration = Duration::from_secs(10);
/// A lease older than this is treated as abandoned.
const STALE_AFTER_MS: i64 = 30_000;

impl ExecutionOwnership {
    pub(crate) fn new(storage: SessionStorage) -> Self {
        let boot_id = random_boot_id();
        Self {
            storage,
            boot_id,
            lost: AtomicBool::new(false),
            consecutive_heartbeat_failures: AtomicU32::new(0),
        }
    }

    pub(crate) fn boot_id(&self) -> &str {
        &self.boot_id
    }

    #[cfg(test)]
    pub(crate) fn mark_lost_for_test(&self) {
        self.lost.store(true, Ordering::SeqCst);
    }

    /// Claims the single chat-agent owner lease for this registry database.
    /// Fails only when a live foreign process already holds it. A lost
    /// ownership is not permanent: every acquire attempts recovery, and the
    /// heartbeat does the same each tick (see `heartbeat`).
    pub(crate) fn ensure_owner(&self) -> Result<(), String> {
        self.try_claim()
    }

    /// Attempts to claim (or re-claim) the execution owner lease for this
    /// registry. Fails only when a live foreign process holds it. On success
    /// clears `lost` and resets the heartbeat failure counter, so a host that
    /// lost its lease recovers as soon as the lease is free, stale, or held by
    /// a process that is gone.
    fn try_claim(&self) -> Result<(), String> {
        let store = self.storage.registry_store();
        let pid = std::process::id() as i64;
        match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), STALE_AFTER_MS) {
            Ok(OwnerClaim::Acquired { .. }) => {
                self.lost.store(false, Ordering::SeqCst);
                self.consecutive_heartbeat_failures
                    .store(0, Ordering::SeqCst);
                Ok(())
            }
            Ok(OwnerClaim::HeldBy {
                boot_id: _,
                pid: holder,
            }) if holder == pid || !process_alive(holder) => {
                // Our own previous directory in this process, or an owner that is no
                // longer alive: re-claim with stale_after_ms = -1 so the staleness
                // check always passes and the takeover does not wait STALE_AFTER_MS.
                match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), -1) {
                    Ok(OwnerClaim::Acquired { .. }) => {
                        self.lost.store(false, Ordering::SeqCst);
                        self.consecutive_heartbeat_failures
                            .store(0, Ordering::SeqCst);
                        Ok(())
                    }
                    Ok(OwnerClaim::HeldBy { pid: holder, .. }) => Err(refused(holder)),
                    Err(error) => Err(unavailable(error)),
                }
            }
            Ok(OwnerClaim::HeldBy {
                boot_id: _,
                pid: holder,
            }) => Err(refused(holder)),
            Err(error) => Err(unavailable(error)),
        }
    }

    /// Refuses agent start when this host has lost the execution owner lease.
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.lost.load(Ordering::SeqCst) {
            Err("execution ownership lost; reconnect to retry, or restart this host".to_string())
        } else {
            Ok(())
        }
    }
    /// Renews the lease. If ownership was already lost, each tick attempts a
    /// re-claim instead of doing nothing: recovery happens here (or on the next
    /// acquire) as soon as the lease is free, stale, or held by a process that
    /// is gone; while a live foreign host holds it, `lost` stays set.
    pub(crate) fn heartbeat(&self) {
        if self.lost.load(Ordering::SeqCst) {
            match self.try_claim() {
                Ok(()) => tracing::info!(target: "ajax_web", "chat execution ownership recovered"),
                Err(error) => {
                    tracing::debug!(target: "ajax_web", %error, "chat execution ownership still lost")
                }
            }
            return;
        }
        match self
            .storage
            .registry_store()
            .heartbeat_execution_owner(&self.boot_id, now_unix_ms())
        {
            Ok(true) => {
                self.consecutive_heartbeat_failures
                    .store(0, Ordering::SeqCst);
            }
            Ok(false) => {
                // Another boot owns the lease now.
                self.lost.store(true, Ordering::SeqCst);
                tracing::error!(target: "ajax_web", "chat execution ownership lost to another host");
            }
            Err(error) => {
                let failures = self
                    .consecutive_heartbeat_failures
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                tracing::warn!(
                    target: "ajax_web",
                    %error,
                    failures,
                    "chat execution ownership heartbeat failed"
                );
                if failures >= 3 {
                    self.lost.store(true, Ordering::SeqCst);
                    tracing::error!(target: "ajax_web", %failures, "chat execution ownership lost after repeated heartbeat failures");
                }
            }
        }
    }
}

impl Drop for ExecutionOwnership {
    fn drop(&mut self) {
        let _ = self
            .storage
            .registry_store()
            .release_execution_owner(&self.boot_id);
    }
}

fn refused(holder_pid: i64) -> String {
    format!(
        "another Ajax web host (pid {holder_pid}) is running chat agents for this registry; stop it or wait for it to exit"
    )
}

fn unavailable(error: impl std::fmt::Display) -> String {
    format!("stored session is unreadable: agent execution ownership unavailable: {error}")
}

/// 16 random bytes as hex; falls back to a pid/nanos stamp if entropy fails.
fn random_boot_id() -> String {
    let mut bytes = [0u8; 16];
    match getrandom::fill(&mut bytes) {
        Ok(()) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        Err(_) => format!("{}-{}", std::process::id(), now_unix_ms()),
    }
}

fn process_alive(pid: i64) -> bool {
    // ponytail: shells out to `ps -p`; use /proc or libc if this ever shows up in a profile
    #[cfg(unix)]
    match std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pid="])
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(output) => !holder_is_dead(output.status.code(), output.stdout.is_empty()),
        // The probe itself failed to start: no positive evidence of death, so keep
        // the holder alive (taking over a live owner is the worse failure).
        Err(_) => true,
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// `ps -p` exits 1 with empty stdout when the process does not exist; every
/// other outcome (exit 0, other codes, signals, a probe that never ran) means
/// we have no positive evidence of death, so the holder counts as alive.
pub(super) fn holder_is_dead(exit_code: Option<i32>, stdout_empty: bool) -> bool {
    exit_code == Some(1) && stdout_empty
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
