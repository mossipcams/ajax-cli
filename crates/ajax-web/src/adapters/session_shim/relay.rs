use super::log::read_lines_from;
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

const REPLAY_PREFIX: &[u8] = b"REPLAY ";
const SHUTDOWN_LINE: &[u8] = b"SHUTDOWN";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RelayError {
    MissingReplayPrefix,
    InvalidOffset,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ClientLine {
    Shutdown,
    Inbound(Vec<u8>),
}

pub(crate) fn parse_client_hello(line: &[u8]) -> Result<u64, RelayError> {
    let digits = line
        .strip_prefix(REPLAY_PREFIX)
        .ok_or(RelayError::MissingReplayPrefix)?;
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(RelayError::InvalidOffset);
    }
    digits
        .iter()
        .try_fold(0u64, |offset, digit| {
            offset.checked_mul(10)?.checked_add(u64::from(digit - b'0'))
        })
        .ok_or(RelayError::InvalidOffset)
}

pub(crate) fn classify_client_line(line: &[u8]) -> ClientLine {
    if line == SHUTDOWN_LINE {
        ClientLine::Shutdown
    } else {
        ClientLine::Inbound(line.to_vec())
    }
}

pub(crate) fn replay_to<W: Write>(log_path: &Path, offset: u64, out: &mut W) -> io::Result<u64> {
    let mut next = offset;
    for (end, line) in read_lines_from(log_path, offset)? {
        out.write_all(&line)?;
        out.write_all(b"\n")?;
        next = end;
    }
    out.flush()?;
    Ok(next)
}

#[derive(Default)]
pub(crate) struct ClientSlot {
    writer: Mutex<Option<Box<dyn Write + Send>>>,
}

impl ClientSlot {
    pub(crate) fn attach(&self, writer: Box<dyn Write + Send>) {
        *self.lock() = Some(writer);
    }

    pub(crate) fn detach(&self) {
        *self.lock() = None;
    }

    pub(crate) fn deliver(&self, line: &[u8]) -> bool {
        let mut slot = self.lock();
        let Some(writer) = slot.as_mut() else {
            return false;
        };
        let mut record = Vec::with_capacity(line.len() + 1);
        record.extend_from_slice(line);
        record.push(b'\n');
        let delivered = writer
            .write_all(&record)
            .and_then(|()| writer.flush())
            .is_ok();
        if !delivered {
            *slot = None;
        }
        delivered
    }

    fn lock(&self) -> MutexGuard<'_, Option<Box<dyn Write + Send>>> {
        self.writer.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_client_line, parse_client_hello, replay_to, ClientLine, ClientSlot, RelayError,
    };
    use crate::adapters::session_shim::log::ShimLog;
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    fn scratch_log(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/session-shim-tests/relay")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("session.shim.log")
    }

    #[derive(Clone, Default)]
    struct SharedSink(Arc<Mutex<Vec<u8>>>);

    impl SharedSink {
        fn bytes(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }
    }

    impl Write for SharedSink {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn replay_request_streams_from_offset_then_live_lines() {
        let path = scratch_log("replay-then-live");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"one").unwrap();
        let replay_from = log.append_line(b"two").unwrap();
        assert_eq!(replay_from, 8);

        let offset = parse_client_hello(b"REPLAY 4").unwrap();
        assert_eq!(offset, 4);
        let mut replayed = Vec::new();
        let next = replay_to(&path, offset, &mut replayed).unwrap();
        assert_eq!(replayed, b"two\n");
        assert_eq!(next, replay_from);

        let client = SharedSink::default();
        let slot = ClientSlot::default();
        slot.attach(Box::new(client.clone()));
        let live_end = log.append_line(b"three").unwrap();
        assert_eq!(live_end, 14);
        assert!(slot.deliver(b"three"));

        let mut client_stream = replayed;
        client_stream.extend_from_slice(&client.bytes());
        assert_eq!(client_stream, b"two\nthree\n");
        assert_eq!(std::fs::read(&path).unwrap(), b"one\ntwo\nthree\n");
    }

    #[test]
    fn session_shim_relay_replay_from_end_writes_nothing() {
        let path = scratch_log("replay-from-end");
        let mut log = ShimLog::open(&path).unwrap();
        let end = log.append_line(b"only").unwrap();

        let mut replayed = Vec::new();
        assert_eq!(replay_to(&path, end, &mut replayed).unwrap(), end);
        assert!(replayed.is_empty());
    }

    #[test]
    fn session_shim_relay_replay_rejects_misaligned_offset_from_hello() {
        let path = scratch_log("replay-misaligned");
        let mut log = ShimLog::open(&path).unwrap();
        log.append_line(b"alpha").unwrap();
        log.append_line(b"beta").unwrap();

        let offset = parse_client_hello(b"REPLAY 3").unwrap();
        let mut replayed = Vec::new();
        let error = replay_to(&path, offset, &mut replayed).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(replayed.is_empty());
    }

    #[test]
    fn session_shim_relay_hello_accepts_decimal_offsets() {
        assert_eq!(parse_client_hello(b"REPLAY 0"), Ok(0));
        assert_eq!(parse_client_hello(b"REPLAY 42"), Ok(42));
        assert_eq!(
            parse_client_hello(b"REPLAY 18446744073709551615"),
            Ok(u64::MAX)
        );
    }

    #[test]
    fn session_shim_relay_hello_rejects_missing_prefix() {
        assert_eq!(
            parse_client_hello(b""),
            Err(RelayError::MissingReplayPrefix)
        );
        assert_eq!(
            parse_client_hello(b"REPLAY"),
            Err(RelayError::MissingReplayPrefix)
        );
        assert_eq!(
            parse_client_hello(b"replay 0"),
            Err(RelayError::MissingReplayPrefix)
        );
        assert_eq!(
            parse_client_hello(b"SHUTDOWN"),
            Err(RelayError::MissingReplayPrefix)
        );
    }

    #[test]
    fn session_shim_relay_hello_rejects_non_numeric_offsets() {
        for line in [
            &b"REPLAY "[..],
            b"REPLAY x",
            b"REPLAY -1",
            b"REPLAY +1",
            b"REPLAY 1 ",
            b"REPLAY 0x10",
            b"REPLAY 18446744073709551616",
        ] {
            assert_eq!(
                parse_client_hello(line),
                Err(RelayError::InvalidOffset),
                "{}",
                String::from_utf8_lossy(line)
            );
        }
    }

    #[test]
    fn session_shim_relay_shutdown_is_control_and_everything_else_is_inbound() {
        assert_eq!(classify_client_line(b"SHUTDOWN"), ClientLine::Shutdown);
        assert_eq!(classify_client_line(b""), ClientLine::Inbound(Vec::new()));
        assert_eq!(
            classify_client_line(b"shutdown"),
            ClientLine::Inbound(b"shutdown".to_vec())
        );
        assert_eq!(
            classify_client_line(b"SHUTDOWN "),
            ClientLine::Inbound(b"SHUTDOWN ".to_vec())
        );
        assert_eq!(
            classify_client_line(b"SHUTDOWN\r"),
            ClientLine::Inbound(b"SHUTDOWN\r".to_vec())
        );
        assert_eq!(
            classify_client_line(b"{\"prompt\":\"hi\"}"),
            ClientLine::Inbound(b"{\"prompt\":\"hi\"}".to_vec())
        );
    }

    #[test]
    fn session_shim_relay_deliver_without_client_reports_undelivered() {
        let slot = ClientSlot::default();
        assert!(!slot.deliver(b"orphan"));
    }

    #[test]
    fn session_shim_relay_detach_stops_delivery() {
        let client = SharedSink::default();
        let slot = ClientSlot::default();
        slot.attach(Box::new(client.clone()));
        slot.detach();
        assert!(!slot.deliver(b"after-detach"));
        assert!(client.bytes().is_empty());
    }

    #[test]
    fn session_shim_relay_client_write_failure_detaches_without_panicking() {
        let slot = ClientSlot::default();
        slot.attach(Box::new(BrokenWriter));
        assert!(!slot.deliver(b"first"));
        assert!(!slot.deliver(b"second"));

        let replacement = SharedSink::default();
        slot.attach(Box::new(replacement.clone()));
        assert!(slot.deliver(b"third"));
        assert_eq!(replacement.bytes(), b"third\n");
    }
}
