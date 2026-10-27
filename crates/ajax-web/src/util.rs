use std::{
    fs,
    path::{Path, PathBuf},
};

/// Write through a temp file and rename, so a crash mid-write leaves the
/// previous file intact instead of a truncated one. The file holds the VAPID
/// private key or subscription keys, so the temp file is always a new
/// owner-only file: a leftover or planted one at that path is removed, never
/// reused, and `create_new` refuses to write through a symlink.
pub(crate) fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let failed = |error: std::io::Error| format!("write {}: {error}", path.display());
    match fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(failed(error)),
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp).map_err(failed)?;
    file.write_all(bytes).map_err(failed)?;
    file.sync_all().map_err(failed)?;
    fs::rename(&tmp, path).map_err(failed)
}
