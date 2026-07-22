use std::{
    fs::{self, OpenOptions},
    io,
    path::Path,
    sync::OnceLock,
};

use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

const DEFAULT_FILTER: &str =
    "ajax=info,ajax_cli=info,ajax_web=info,ajax_core=info,ajax_supervisor=info";

static LOGGING_INIT: OnceLock<()> = OnceLock::new();

/// Install a global tracing subscriber that appends to `{logs_dir}/ajax.log`.
///
/// Safe to call more than once; only the first successful call installs the subscriber.
/// IO failures are reported once to stderr and otherwise ignored; failed attempts may be retried.
pub fn init_to_logs_dir(logs_dir: &Path) {
    install_to_logs_dir_once(&LOGGING_INIT, logs_dir);
}

fn install_to_logs_dir_once(init_lock: &OnceLock<()>, logs_dir: &Path) {
    if init_lock.get().is_some() {
        return;
    }
    if init_to_logs_dir_inner(logs_dir).is_ok() {
        let _ = init_lock.set(());
    }
}

fn init_to_logs_dir_inner(logs_dir: &Path) -> Result<(), ()> {
    if let Err(error) = fs::create_dir_all(logs_dir) {
        eprintln!(
            "ajax logging: failed to create logs directory {}: {error}",
            logs_dir.display()
        );
        return Err(());
    }

    let log_path = logs_dir.join("ajax.log");
    let file = match open_append_log(&log_path) {
        Some(file) => file,
        None => return Err(()),
    };

    let filter = build_env_filter();
    let stderr_mirror = std::env::var("AJAX_LOG_STDERR").as_deref() == Ok("1");
    let registry = tracing_subscriber::registry().with(filter);

    let result = if stderr_mirror {
        let file_layer = fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_target(true);
        let stderr_layer = fmt::layer()
            .with_writer(io::stderr)
            .with_ansi(true)
            .with_target(true);
        registry.with(file_layer).with(stderr_layer).try_init()
    } else {
        let file_layer = fmt::layer()
            .with_writer(file)
            .with_ansi(false)
            .with_target(true);
        registry.with(file_layer).try_init()
    };

    if let Err(error) = result {
        eprintln!("ajax logging: failed to install subscriber: {error}");
        return Err(());
    }

    Ok(())
}

pub(crate) fn open_append_log(path: &Path) -> Option<std::fs::File> {
    match OpenOptions::new().create(true).append(true).open(path) {
        Ok(file) => Some(file),
        Err(error) => {
            eprintln!(
                "ajax logging: failed to open log file {}: {error}",
                path.display()
            );
            None
        }
    }
}

fn build_env_filter() -> EnvFilter {
    let spec = std::env::var("AJAX_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_else(|_| DEFAULT_FILTER.to_string());
    EnvFilter::try_new(spec).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_logs_dir(name: &str) -> std::path::PathBuf {
        let n = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ajax_logging_{name}_{n}_{}", std::process::id()))
    }

    #[test]
    fn init_writes_info_event_to_ajax_log_and_is_idempotent() {
        if LOGGING_INIT.get().is_some() {
            return;
        }

        let bad_base = unique_logs_dir("bad_init");
        let _ = fs::remove_dir_all(&bad_base);
        fs::write(&bad_base, b"not-a-directory").unwrap();

        init_to_logs_dir(&bad_base.join("nested"));
        assert!(
            LOGGING_INIT.get().is_none(),
            "failed init must not mark logging complete"
        );

        let logs_dir = unique_logs_dir("smoke");
        let _ = fs::remove_dir_all(&logs_dir);

        init_to_logs_dir(&logs_dir);
        tracing::info!(target: "ajax_core", "logging_smoke");
        init_to_logs_dir(&logs_dir);

        let log_path = logs_dir.join("ajax.log");
        let contents = fs::read_to_string(&log_path).expect("ajax.log should exist");
        assert!(
            contents.contains("logging_smoke"),
            "expected logging_smoke in log file, got: {contents}"
        );

        let _ = fs::remove_dir_all(&bad_base);
        let _ = fs::remove_dir_all(&logs_dir);
    }

    #[test]
    fn failed_init_does_not_mark_once_lock_complete() {
        static RETRY_LOCK: OnceLock<()> = OnceLock::new();
        let bad_base = unique_logs_dir("retry_lock_bad");
        let _ = fs::remove_dir_all(&bad_base);
        fs::write(&bad_base, b"not-a-directory").unwrap();

        install_to_logs_dir_once(&RETRY_LOCK, &bad_base.join("nested"));
        assert!(
            RETRY_LOCK.get().is_none(),
            "failed init must not mark logging complete"
        );

        let _ = fs::remove_dir_all(&bad_base);
    }

    #[test]
    fn open_append_log_returns_none_for_invalid_parent() {
        let base = unique_logs_dir("bad_parent");
        let _ = fs::remove_dir_all(&base);
        fs::write(&base, b"not-a-directory").unwrap();

        let log_path = base.join("nested/ajax.log");
        assert!(open_append_log(&log_path).is_none());
        let _ = fs::remove_dir_all(&base);
    }
}
