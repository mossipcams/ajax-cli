//! PTY-backed tmux attach for the browser task terminal bridge.

mod attach;
mod bridge;

pub use attach::*;
pub use bridge::*;

#[cfg(test)]
pub(crate) async fn simulate_terminal_disconnect_cleanup_for_tests(
    wait_timeout: std::time::Duration,
) {
    let (child, _release) = tests::MockChild::gated();
    cleanup_spawned_child_async_with_timeout(child, wait_timeout).await;
}

#[cfg(test)]
mod tests;
