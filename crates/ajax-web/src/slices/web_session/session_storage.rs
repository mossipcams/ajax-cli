use ajax_core::registry::SqliteRegistryStore;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Storage locations shared by every Chat session in a companion.
#[derive(Clone)]
pub struct SessionStorage {
    pub state_dir: PathBuf,
    pub registry_db: PathBuf,
    pub(crate) owner_fence: Arc<OnceLock<String>>,
}

impl SessionStorage {
    pub fn under(state_dir: PathBuf) -> Self {
        Self {
            registry_db: state_dir.join("ajax.db"),
            state_dir,
            owner_fence: Default::default(),
        }
    }

    /// Construct a store without opening the database.
    pub fn registry_store(&self) -> SqliteRegistryStore {
        let store = SqliteRegistryStore::new(&self.registry_db);
        match self.owner_fence.get() {
            Some(boot_id) => store.with_owner_fence(boot_id),
            None => store,
        }
    }
}

impl From<PathBuf> for SessionStorage {
    fn from(state_dir: PathBuf) -> Self {
        Self::under(state_dir)
    }
}
