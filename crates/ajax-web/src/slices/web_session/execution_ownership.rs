use super::SessionStorage;
use ajax_core::registry::OwnerClaim;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Only one Ajax web host process may run chat agents against a given registry.
pub(crate) struct ExecutionOwnership {
    storage: SessionStorage,
    boot_id: String,
}

/// How often the runtime task renews the execution owner lease.
pub(crate) const HEARTBEAT_EVERY: Duration = Duration::from_secs(10);
/// A lease older than this is treated as abandoned.
const STALE_AFTER_MS: i64 = 30_000;

impl ExecutionOwnership {
    pub(crate) fn new(storage: SessionStorage) -> Self {
        let boot_id = random_boot_id();
        Self { storage, boot_id }
    }

    #[cfg(test)]
    pub(crate) fn boot_id(&self) -> &str {
        &self.boot_id
    }

    /// Claims the single chat-agent owner lease for this registry database.
    /// Fails only when a live foreign process already holds it.
    pub(crate) fn ensure_owner(&self) -> Result<(), String> {
        let store = self.storage.registry_store();
        let pid = std::process::id() as i64;
        match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), STALE_AFTER_MS) {
            Ok(OwnerClaim::Acquired { .. }) => Ok(()),
            Ok(OwnerClaim::HeldBy {
                boot_id: _,
                pid: holder,
            }) if holder == pid || !process_alive(holder) => {
                // Our own previous directory in this process, or an owner that is no
                // longer alive: re-claim with stale_after_ms = -1 so the staleness
                // check always passes and the takeover does not wait STALE_AFTER_MS.
                match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), -1) {
                    Ok(OwnerClaim::Acquired { .. }) => Ok(()),
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

    /// Renews the lease; a lost lease is logged, not fatal for an in-flight session.
    pub(crate) fn heartbeat(&self) {
        if let Err(error) = self
            .storage
            .registry_store()
            .heartbeat_execution_owner(&self.boot_id, now_unix_ms())
        {
            tracing::warn!(target: "ajax_web", %error, "chat execution ownership heartbeat failed");
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
    // ponytail: shells out to kill -0; use libc::kill if this ever shows up in a profile
    #[cfg(unix)]
    match std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(_) => false,
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}
