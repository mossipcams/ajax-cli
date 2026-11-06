use crate::slices::web_session::TaskSessionDirectory;
use std::{future::pending, time::Duration};
use tokio::signal::unix::{signal, Signal, SignalKind};

const FLUSH_BUDGET: Duration = Duration::from_secs(5);

pub(super) fn terminate_signal() -> Option<Signal> {
    match signal(SignalKind::terminate()) {
        Ok(stream) => Some(stream),
        Err(error) => {
            tracing::warn!(target: "ajax_web", %error, "SIGTERM handler unavailable; stop will not flush sessions");
            None
        }
    }
}

pub(super) async fn stop_and_flush(
    terminate: &mut Option<Signal>,
    directory: &TaskSessionDirectory,
) {
    next_stop(terminate).await;
    tracing::info!(target: "ajax_web", "web server stopping");
    tokio::select! {
        () = flush_sessions(directory) => {}
        () = next_stop(terminate) => {
            tracing::warn!(target: "ajax_web", "second stop signal; exiting before the session flush finished");
            std::process::exit(0);
        }
    }
}

async fn next_stop(terminate: &mut Option<Signal>) {
    let terminated = async {
        match terminate {
            Some(stream) => {
                stream.recv().await;
            }
            None => pending::<()>().await,
        }
    };
    tokio::select! {
        () = terminated => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}

async fn flush_sessions(directory: &TaskSessionDirectory) {
    if tokio::time::timeout(FLUSH_BUDGET, directory.flush_all())
        .await
        .is_err()
    {
        tracing::warn!(target: "ajax_web", "web session flush exceeded the shutdown budget");
    }
}
