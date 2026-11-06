use crate::slices::web_session::TaskSessionDirectory;
use std::{future::pending, time::Duration};
use tokio::signal::unix::{signal, SignalKind};

const FLUSH_BUDGET: Duration = Duration::from_secs(5);

pub(super) async fn termination_signal() {
    let terminate = async {
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => pending::<()>().await,
        }
    };
    tokio::select! {
        () = terminate => {}
        _ = tokio::signal::ctrl_c() => {}
    }
}

pub(super) async fn flush_sessions(directory: &TaskSessionDirectory) {
    tracing::info!(target: "ajax_web", "web server stopping");
    if tokio::time::timeout(FLUSH_BUDGET, directory.detach_all())
        .await
        .is_err()
    {
        tracing::warn!(target: "ajax_web", "web session flush exceeded the shutdown budget");
    }
}
