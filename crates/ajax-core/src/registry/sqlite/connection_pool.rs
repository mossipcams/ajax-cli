use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock, PoisonError},
};

use rusqlite::Connection;

const MAX_IDLE_PER_PATH: usize = 4;
const MAX_PATHS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FileIdentity {
    dev: u64,
    ino: u64,
}

#[cfg(unix)]
pub(super) fn file_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|meta| FileIdentity {
        dev: meta.dev(),
        ino: meta.ino(),
    })
}

#[cfg(not(unix))]
pub(super) fn file_identity(path: &Path) -> Option<FileIdentity> {
    path.exists().then_some(FileIdentity { dev: 0, ino: 0 })
}

struct IdleConnection {
    identity: FileIdentity,
    connection: Connection,
}

type Pool = HashMap<PathBuf, Vec<IdleConnection>>;

fn pool() -> MutexGuard<'static, Pool> {
    static POOL: OnceLock<Mutex<Pool>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn take_idle(path: &Path, identity: FileIdentity) -> Option<Connection> {
    let mut pool = pool();
    let idle = pool.get_mut(path)?;
    idle.retain(|entry| entry.identity == identity);
    let connection = idle.pop().map(|entry| entry.connection);
    if idle.is_empty() {
        pool.remove(path);
    }
    connection
}

pub(super) fn discard(path: &Path) {
    pool().remove(path);
}

fn give_back(path: &Path, identity: FileIdentity, connection: Connection) {
    let mut pool = pool();
    if !pool.contains_key(path) && pool.len() >= MAX_PATHS {
        return;
    }
    let idle = pool.entry(path.to_path_buf()).or_default();
    if idle.len() < MAX_IDLE_PER_PATH {
        idle.push(IdleConnection {
            identity,
            connection,
        });
    }
}

pub(super) struct PooledConnection {
    path: PathBuf,
    identity: FileIdentity,
    connection: Option<Connection>,
}

impl PooledConnection {
    pub(super) fn new(path: PathBuf, identity: FileIdentity, connection: Connection) -> Self {
        Self {
            path,
            identity,
            connection: Some(connection),
        }
    }
}

impl Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        self.connection
            .as_ref()
            .expect("pooled connection is present until drop")
    }
}

impl DerefMut for PooledConnection {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection
            .as_mut()
            .expect("pooled connection is present until drop")
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            if connection.is_autocommit() {
                give_back(&self.path, self.identity, connection);
            }
        }
    }
}

#[cfg(test)]
static FRESH_OPENS: Mutex<Option<HashMap<PathBuf, usize>>> = Mutex::new(None);

#[cfg(test)]
pub(super) fn record_fresh_open(path: &Path) {
    let mut opens = FRESH_OPENS.lock().unwrap_or_else(PoisonError::into_inner);
    *opens
        .get_or_insert_with(HashMap::new)
        .entry(path.to_path_buf())
        .or_default() += 1;
}

#[cfg(test)]
pub(super) fn fresh_opens(path: &Path) -> usize {
    FRESH_OPENS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .and_then(|opens| opens.get(path).copied())
        .unwrap_or(0)
}

#[cfg(test)]
pub(super) fn idle_count(path: &Path) -> usize {
    pool().get(path).map_or(0, Vec::len)
}
