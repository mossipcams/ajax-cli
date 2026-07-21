use std::path::PathBuf;

#[cfg(unix)]
pub(crate) fn start_agent_event_notify_listener(events_dir: PathBuf) -> std::io::Result<()> {
    spawn_notify_listener_with_sink(events_dir, None)
}

#[cfg(not(unix))]
pub(crate) fn start_agent_event_notify_listener(_events_dir: PathBuf) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn spawn_notify_listener_with_sink(
    events_dir: PathBuf,
    sink: Option<std::sync::mpsc::Sender<String>>,
) -> std::io::Result<()> {
    use std::fs;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::thread;

    use crate::agent_event::notify_socket_path;

    fs::create_dir_all(&events_dir)?;
    let sock_path = notify_socket_path(&events_dir);
    let _ = fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;

    thread::spawn(move || loop {
        let Ok((stream, _)) = listener.accept() else {
            continue;
        };
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            continue;
        }
        if let Some(ref tx) = sink {
            let _ = tx.send(line);
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn listener_accepts_writer_line() {
        use std::io::Write;
        use std::os::unix::net::UnixStream;
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = std::path::PathBuf::from(format!(
            "/tmp/ajax-notify-listener-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (tx, rx) = mpsc::channel();
        spawn_notify_listener_with_sink(dir.clone(), Some(tx)).unwrap();

        let socket_path = crate::agent_event::notify_socket_path(&dir);
        let mut stream = UnixStream::connect(&socket_path).unwrap();
        stream.write_all(b"{\"schema_version\":1}\n").unwrap();

        let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(received.trim(), "{\"schema_version\":1}");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
