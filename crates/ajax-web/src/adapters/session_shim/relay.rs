use super::log::ShimLog;
use std::io::{self, Write};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClientToken(u64);

struct AttachedClient {
    token: ClientToken,
    writer: Box<dyn Write + Send>,
}

struct RelayState {
    log: ShimLog,
    client: Option<AttachedClient>,
    next_token: u64,
}

impl RelayState {
    fn deliver(&mut self, record: &[u8]) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        if client
            .writer
            .write_all(record)
            .and_then(|()| client.writer.flush())
            .is_err()
        {
            self.client = None;
        }
    }
}

pub(crate) struct ShimRelay {
    state: Mutex<RelayState>,
}

impl ShimRelay {
    pub(crate) fn new(log: ShimLog) -> Self {
        Self {
            state: Mutex::new(RelayState {
                log,
                client: None,
                next_token: 0,
            }),
        }
    }

    pub(crate) fn append_and_deliver(&self, line: &[u8]) -> io::Result<u64> {
        let mut state = self.lock();
        let end = state.log.append_line(line)?;
        let mut record = Vec::with_capacity(line.len() + 1);
        record.extend_from_slice(line);
        record.push(b'\n');
        state.deliver(&record);
        Ok(end)
    }

    pub(crate) fn attach(
        &self,
        offset: u64,
        mut writer: Box<dyn Write + Send>,
    ) -> io::Result<ClientToken> {
        let mut state = self.lock();
        for (_, line) in state.log.lines_from(offset)? {
            writer.write_all(&line)?;
            writer.write_all(b"\n")?;
        }
        writer.flush()?;
        let token = ClientToken(state.next_token);
        state.next_token += 1;
        state.client = Some(AttachedClient { token, writer });
        Ok(token)
    }

    pub(crate) fn detach(&self, token: ClientToken) {
        let mut state = self.lock();
        if state
            .client
            .as_ref()
            .is_some_and(|client| client.token == token)
        {
            state.client = None;
        }
    }

    fn lock(&self) -> MutexGuard<'_, RelayState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_client_line, parse_client_hello, ClientLine, RelayError, ShimRelay};
    use crate::adapters::session_shim::log::{read_lines_from, ShimLog};
    use std::io::{self, Write};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::thread;

    fn scratch_log(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/session-shim-tests/relay")
            .join(name);
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("nested").join("session.shim.log")
    }

    fn relay_at(path: &Path) -> ShimRelay {
        ShimRelay::new(ShimLog::open(path).unwrap())
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
        let relay = relay_at(&path);
        relay.append_and_deliver(b"one").unwrap();
        let replay_from = relay.append_and_deliver(b"two").unwrap();
        assert_eq!(replay_from, 8);

        let offset = parse_client_hello(b"REPLAY 4").unwrap();
        assert_eq!(offset, 4);
        let client = SharedSink::default();
        relay.attach(offset, Box::new(client.clone())).unwrap();
        let live_end = relay.append_and_deliver(b"three").unwrap();
        assert_eq!(live_end, 14);

        assert_eq!(client.bytes(), b"two\nthree\n");
        assert_eq!(std::fs::read(&path).unwrap(), b"one\ntwo\nthree\n");
    }

    #[test]
    fn session_shim_relay_replay_handoff_delivers_each_line_once_under_concurrent_append() {
        let path = scratch_log("concurrent-handoff");
        let relay = Arc::new(relay_at(&path));
        for index in 0..5 {
            relay
                .append_and_deliver(format!("pre-{index}").as_bytes())
                .unwrap();
        }
        let attach_at = relay.append_and_deliver(b"pre-end").unwrap();

        let appender = {
            let relay = Arc::clone(&relay);
            thread::spawn(move || {
                for index in 0..500 {
                    relay
                        .append_and_deliver(format!("live-{index}").as_bytes())
                        .unwrap();
                }
            })
        };
        let client = SharedSink::default();
        relay.attach(attach_at, Box::new(client.clone())).unwrap();
        appender.join().unwrap();

        let expected: Vec<u8> = (0..500)
            .flat_map(|index| format!("live-{index}\n").into_bytes())
            .collect();
        assert_eq!(client.bytes(), expected);
    }

    #[test]
    fn session_shim_relay_replay_from_end_writes_nothing() {
        let path = scratch_log("replay-from-end");
        let relay = relay_at(&path);
        let end = relay.append_and_deliver(b"only").unwrap();

        let client = SharedSink::default();
        relay.attach(end, Box::new(client.clone())).unwrap();
        assert!(client.bytes().is_empty());
    }

    #[test]
    fn session_shim_relay_replay_rejects_misaligned_offset_from_hello() {
        let path = scratch_log("replay-misaligned");
        let relay = relay_at(&path);
        relay.append_and_deliver(b"alpha").unwrap();
        relay.append_and_deliver(b"beta").unwrap();

        let offset = parse_client_hello(b"REPLAY 3").unwrap();
        let client = SharedSink::default();
        let error = relay.attach(offset, Box::new(client.clone())).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        relay.append_and_deliver(b"gamma").unwrap();
        assert!(client.bytes().is_empty());
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
    fn session_shim_relay_append_without_client_only_logs() {
        let path = scratch_log("append-without-client");
        let relay = relay_at(&path);
        assert_eq!(relay.append_and_deliver(b"orphan").unwrap(), 7);
        assert_eq!(
            read_lines_from(&path, 0).unwrap(),
            vec![(7, b"orphan".to_vec())]
        );
    }

    #[test]
    fn session_shim_relay_detach_stops_delivery() {
        let path = scratch_log("detach-stops");
        let relay = relay_at(&path);
        let client = SharedSink::default();
        let token = relay.attach(0, Box::new(client.clone())).unwrap();
        relay.detach(token);
        relay.append_and_deliver(b"after-detach").unwrap();
        assert!(client.bytes().is_empty());
    }

    #[test]
    fn session_shim_relay_stale_detach_keeps_newer_client() {
        let path = scratch_log("stale-detach");
        let relay = relay_at(&path);
        let stale_client = SharedSink::default();
        let stale = relay.attach(0, Box::new(stale_client.clone())).unwrap();
        let current_client = SharedSink::default();
        let current = relay.attach(0, Box::new(current_client.clone())).unwrap();

        relay.detach(stale);
        relay.append_and_deliver(b"live").unwrap();
        assert_eq!(current_client.bytes(), b"live\n");
        assert!(stale_client.bytes().is_empty());

        relay.detach(current);
        relay.append_and_deliver(b"after").unwrap();
        assert_eq!(current_client.bytes(), b"live\n");
    }

    #[test]
    fn session_shim_relay_client_write_failure_detaches_without_panicking() {
        let path = scratch_log("write-failure");
        let relay = relay_at(&path);
        relay.attach(0, Box::new(BrokenWriter)).unwrap();
        relay.append_and_deliver(b"first").unwrap();
        let end = relay.append_and_deliver(b"second").unwrap();

        let replacement = SharedSink::default();
        relay.attach(end, Box::new(replacement.clone())).unwrap();
        relay.append_and_deliver(b"third").unwrap();
        assert_eq!(replacement.bytes(), b"third\n");
    }
}
