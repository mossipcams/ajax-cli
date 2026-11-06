use crate::slices::web_session::TaskSessionDirectory;
use std::{future::pending, time::Duration};
use tokio::signal::unix::{signal, Signal, SignalKind};

const FLUSH_BUDGET: Duration = Duration::from_secs(5);

pub(super) fn terminate_signal() -> Option<Signal> {
    signal(SignalKind::terminate()).ok()
}

pub(super) async fn stop_requested(terminate: &mut Option<Signal>) {
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

pub(super) async fn flush_sessions(directory: &TaskSessionDirectory) {
    tracing::info!(target: "ajax_web", "web server stopping");
    if tokio::time::timeout(FLUSH_BUDGET, directory.flush_all())
        .await
        .is_err()
    {
        tracing::warn!(target: "ajax_web", "web session flush exceeded the shutdown budget");
    }
}
