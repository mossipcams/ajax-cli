use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub(crate) struct ShimLog {
    file: File,
    len: u64,
}

impl ShimLog {
    pub(crate) fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }

    pub(crate) fn len(&self) -> u64 {
        self.len
    }

    pub(crate) fn append_line(&mut self, line: &[u8]) -> io::Result<u64> {
        if line.contains(&b'\n') {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shim log line must not contain a newline",
            ));
        }
        let mut record = Vec::with_capacity(line.len() + 1);
        record.extend_from_slice(line);
        record.push(b'\n');
        self.file.write_all(&record)?;
        self.len += record.len() as u64;
        Ok(self.len)
    }
}

pub(crate) fn read_lines_from(path: &Path, offset: u64) -> io::Result<Vec<(u64, Vec<u8>)>> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    if offset > file_len {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("shim log offset {offset} is past end of log ({file_len} bytes)"),
        ));
    }
    if offset > 0 {
        file.seek(SeekFrom::Start(offset - 1))?;
        let mut previous = [0u8; 1];
        file.read_exact(&mut previous)?;
        if previous[0] != b'\n' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("shim log offset {offset} is not at a line boundary"),
            ));
        }
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::new(file);
    let mut position = offset;
    let mut lines = Vec::new();
    loop {
        let mut line = Vec::new();
        let read = reader.read_until(b'\n', &mut line)?;
        if line.last() != Some(&b'\n') {
            break;
        }
        line.pop();
        position += read as u64;
        lines.push((position, line));
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::{read_lines_from, ShimLog};
    use std::path::{Path, PathBuf};

    fn scratch_log(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/session-shim-tests")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("session.shim.log")
    }

    #[test]
    fn session_shim_append_then_replay_from_offset_returns_exact_suffix() {
        let path = scratch_log("append-replay");
        let mut log = ShimLog::open(&path).unwrap();
        let first = log.append_line(b"alpha").unwrap();
        let second = log.append_line(b"").unwrap();
        let third = log.append_line(&[0xff, 0x00, b'z']).unwrap();
        assert_eq!(first, 6);
        assert_eq!(second, 7);
        assert_eq!(third, 11);
        assert_eq!(log.len(), 11);

        let all = read_lines_from(&path, 0).unwrap();
        assert_eq!(
            all,
            vec![
                (6, b"alpha".to_vec()),
                (7, Vec::new()),
                (11, vec![0xff, 0x00, b'z']),
            ]
        );

        let suffix = read_lines_from(&path, first).unwrap();
        assert_eq!(suffix, vec![(7, Vec::new()), (11, vec![0xff, 0x00, b'z'])]);
        assert!(read_lines_from(&path, third).unwrap().is_empty());
    }

    #[test]
    fn session_shim_log_reopen_keeps_length_and_appends_after_it() {
        let path = scratch_log("reopen");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"one").unwrap();
        drop(log);

        let mut reopened = ShimLog::open(&path).unwrap();
        assert_eq!(reopened.len(), 4);
        assert_eq!(reopened.append_line(b"two").unwrap(), 8);
        assert_eq!(
            read_lines_from(&path, 0).unwrap(),
            vec![(4, b"one".to_vec()), (8, b"two".to_vec())]
        );
    }

    #[test]
    fn session_shim_read_skips_trailing_partial_line() {
        let path = scratch_log("partial-tail");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"done").unwrap();
        drop(log);
        std::fs::write(&path, b"done\npartial").unwrap();

        assert_eq!(
            read_lines_from(&path, 0).unwrap(),
            vec![(5, b"done".to_vec())]
        );
    }

    #[test]
    fn session_shim_append_rejects_embedded_newline() {
        let path = scratch_log("embedded-newline");
        let mut log = ShimLog::open(&path).unwrap();
        let error = log.append_line(b"a\nb").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn session_shim_replay_rejects_misaligned_offset() {
        let path = scratch_log("misaligned");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"alpha").unwrap();
        log.append_line(b"beta").unwrap();

        let error = read_lines_from(&path, 3).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn session_shim_replay_rejects_offset_past_eof() {
        let path = scratch_log("past-eof");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"alpha").unwrap();

        let error = read_lines_from(&path, 7).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
