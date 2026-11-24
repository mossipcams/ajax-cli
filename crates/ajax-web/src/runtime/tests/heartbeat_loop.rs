use super::*;
use crate::runtime::spawn_heartbeat_loop;
use crate::slices::web_session::SessionActivity;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

#[test]
fn blocked_heartbeat_does_not_hold_the_runtime_worker() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("test runtime");
    let (entered_tx, entered_rx) = mpsc::channel::<()>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let entered_tx = Mutex::new(entered_tx);
    let release_rx = Mutex::new(release_rx);

    rt.block_on(async {
        spawn_heartbeat_loop(Duration::from_millis(1), move || {
            let _ = entered_tx.lock().unwrap().send(());
            let _ = release_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5));
        });
    });
    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("heartbeat should run");

    let (progress_tx, progress_rx) = mpsc::channel::<()>();
    rt.spawn(async move {
        let _ = progress_tx.send(());
    });
    let progressed = progress_rx.recv_timeout(Duration::from_secs(1)).is_ok();

    drop(release_tx);
    rt.shutdown_timeout(Duration::from_secs(1));
    assert!(
        progressed,
        "a blocked heartbeat must not prevent other tasks from running"
    );
}

#[test]
fn heartbeats_never_overlap() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");
    let in_flight = Arc::new(AtomicUsize::new(0));
    let max_in_flight = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let heartbeat = {
        let (in_flight, max_in_flight, calls) = (
            Arc::clone(&in_flight),
            Arc::clone(&max_in_flight),
            Arc::clone(&calls),
        );
        move || {
            let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            max_in_flight.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(5));
            in_flight.fetch_sub(1, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
        }
    };

    rt.block_on(async {
        spawn_heartbeat_loop(Duration::from_millis(1), heartbeat);
        for _ in 0..500 {
            if calls.load(Ordering::SeqCst) >= 5 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    });
    rt.shutdown_timeout(Duration::from_secs(1));

    assert!(
        calls.load(Ordering::SeqCst) >= 5,
        "heartbeat should keep ticking"
    );
    assert_eq!(max_in_flight.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recovered_turn_report_from_blocking_pool_does_not_panic() {
    let state = state_with_bridge_and_task(TestBridge::default());
    let result = tokio::task::spawn_blocking(move || {
        state.report_task_session_activity("web/fix-login", SessionActivity::TurnFailed)
    })
    .await;
    assert!(
        result.is_ok(),
        "block_in_place must not panic on the blocking pool"
    );
}
