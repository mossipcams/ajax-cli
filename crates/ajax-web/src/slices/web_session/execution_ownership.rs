use super::SessionStorage;
use ajax_core::registry::OwnerClaim;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) struct ExecutionOwnership {
    storage: SessionStorage,
    boot_id: String,
    lost: AtomicBool,
    consecutive_heartbeat_failures: AtomicU32,
    recovered_handles: std::sync::Mutex<Vec<String>>,
}

pub(crate) const HEARTBEAT_EVERY: Duration = Duration::from_secs(10);
const STALE_AFTER_MS: i64 = 30_000;

impl ExecutionOwnership {
    pub(crate) fn new(mut storage: SessionStorage) -> Self {
        let boot_id = random_boot_id();
        storage.owner_fence = Default::default();
        Self {
            storage,
            boot_id,
            lost: AtomicBool::new(false),
            consecutive_heartbeat_failures: AtomicU32::new(0),
            recovered_handles: std::sync::Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn boot_id(&self) -> &str {
        &self.boot_id
    }

    pub(crate) fn storage(&self) -> &SessionStorage {
        &self.storage
    }

    pub(crate) fn take_recovered_handles(&self) -> Vec<String> {
        std::mem::take(&mut *self.recovered_handles.lock().unwrap())
    }

    pub(crate) fn requeue_recovered_handles(&self, handles: Vec<String>) {
        self.recovered_handles.lock().unwrap().extend(handles);
    }

    #[cfg(test)]
    pub(crate) fn mark_lost_for_test(&self) {
        self.lost.store(true, Ordering::SeqCst);
    }

    pub(crate) fn ensure_owner(&self) -> Result<(), String> {
        self.try_claim()
    }

    fn try_claim(&self) -> Result<(), String> {
        let store = self.storage.registry_store();
        let pid = std::process::id() as i64;
        match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), STALE_AFTER_MS) {
            Ok(OwnerClaim::Acquired { .. }) => {
                let _ = self.storage.owner_fence.set(self.boot_id.clone());
                self.lost.store(false, Ordering::SeqCst);
                self.consecutive_heartbeat_failures
                    .store(0, Ordering::SeqCst);
                self.recovered_handles.lock().unwrap().extend(
                    super::orphan_sweep::sweep_once(
                        &self.storage.registry_store(),
                        &self.storage.state_dir,
                        &self.boot_id,
                    )
                    .recovered_handles,
                );
                Ok(())
            }
            Ok(OwnerClaim::HeldBy {
                boot_id: _,
                pid: holder,
            }) if holder == pid || !process_alive(holder) => {
                match store.claim_execution_owner(&self.boot_id, pid, now_unix_ms(), -1) {
                    Ok(OwnerClaim::Acquired { .. }) => {
                        let _ = self.storage.owner_fence.set(self.boot_id.clone());
                        self.lost.store(false, Ordering::SeqCst);
                        self.consecutive_heartbeat_failures
                            .store(0, Ordering::SeqCst);
                        self.recovered_handles.lock().unwrap().extend(
                            super::orphan_sweep::sweep_once(
                                &self.storage.registry_store(),
                                &self.storage.state_dir,
                                &self.boot_id,
                            )
                            .recovered_handles,
                        );
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

    pub(crate) fn check(&self) -> Result<(), String> {
        if self.lost.load(Ordering::SeqCst) {
            Err("execution ownership lost; reconnect to retry, or restart this host".to_string())
        } else {
            Ok(())
        }
    }
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
                self.recovered_handles.lock().unwrap().extend(
                    super::orphan_sweep::sweep_once(
                        &self.storage.registry_store(),
                        &self.storage.state_dir,
                        &self.boot_id,
                    )
                    .recovered_handles,
                );
            }
            Ok(false) => {
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

fn random_boot_id() -> String {
    let mut bytes = [0u8; 16];
    match getrandom::fill(&mut bytes) {
        Ok(()) => bytes.iter().map(|b| format!("{b:02x}")).collect(),
        Err(_) => format!("{}-{}", std::process::id(), now_unix_ms()),
    }
}

fn process_alive(pid: i64) -> bool {
    #[cfg(unix)]
    {
        super::os_process::exists(pid)
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
