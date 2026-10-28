use crate::adapters::web_session_store::{encode_handle, WEB_SESSION_DIR};
use std::path::{Path, PathBuf};

#[cfg_attr(not(test), allow(dead_code))]
mod log;
mod process;
#[cfg(test)]
mod process_tests;
#[cfg_attr(not(test), allow(dead_code))]
mod relay;
mod spawn;

#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use log::answered_request_ids;
pub use process::{run_session_shim, ShimArgs};
pub use spawn::{enable, shim_spawn, without_shim, ShimSpawn};

#[derive(Debug, PartialEq, Eq)]
pub struct ShimPaths {
    pub log: PathBuf,
    pub socket: PathBuf,
    pub stderr: PathBuf,
}

pub fn shim_paths(state_dir: &Path, handle: &str) -> ShimPaths {
    let dir = state_dir.join(WEB_SESSION_DIR);
    let encoded = encode_handle(handle);
    ShimPaths {
        log: dir.join(format!("{encoded}.shim.log")),
        socket: dir.join(format!("s-{:016x}.sock", fnv1a_64(handle.as_bytes()))),
        stderr: dir.join(format!("{encoded}.shim.stderr")),
    }
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::{fnv1a_64, shim_paths};
    use std::path::Path;

    #[test]
    fn session_shim_socket_hash_matches_fnv1a_golden_vectors() {
        assert_eq!(fnv1a_64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a_64(b"foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(fnv1a_64(b"repo/handle"), 0xa07b_4bdd_2bdc_4870);
    }

    #[test]
    fn session_shim_paths_use_encoded_log_and_hashed_socket() {
        let paths = shim_paths(Path::new("/state"), "repo/handle");
        assert_eq!(
            paths.log,
            Path::new("/state/web-session/repo%2Fhandle.shim.log")
        );
        assert_eq!(
            paths.socket,
            Path::new("/state/web-session/s-a07b4bdd2bdc4870.sock")
        );
    }

    #[test]
    fn session_shim_socket_path_stays_short_for_long_handles() {
        let handle = "a/".repeat(200);
        let paths = shim_paths(Path::new("/state"), &handle);
        let file_name = paths.socket.file_name().unwrap().to_str().unwrap();
        assert_eq!(file_name.len(), "s-".len() + 16 + ".sock".len());
    }
}
