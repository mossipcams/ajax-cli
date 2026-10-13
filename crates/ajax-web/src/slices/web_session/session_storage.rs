use ajax_core::registry::SqliteRegistryStore;
use std::path::PathBuf;

/// Storage locations shared by every Chat session in a companion.
#[derive(Clone)]
pub struct SessionStorage {
    pub state_dir: PathBuf,
    pub registry_db: PathBuf,
}

impl SessionStorage {
    pub fn under(state_dir: PathBuf) -> Self {
        Self {
            registry_db: state_dir.join("ajax.db"),
            state_dir,
        }
    }

    /// Construct a store without opening the database.
    pub fn registry_store(&self) -> SqliteRegistryStore {
        SqliteRegistryStore::new(&self.registry_db)
    }
}

impl From<PathBuf> for SessionStorage {
    fn from(state_dir: PathBuf) -> Self {
        Self::under(state_dir)
    }
}
