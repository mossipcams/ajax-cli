use super::*;

/// T18: prove orchestration context survives viewer WebSocket leave/return.
#[test]
fn axum_session_socket_remembers_acp_context_across_disconnect_reconnect_and_cold_replay() {
    use crate::adapters::web_session_acp::{with_test_acp_extra_args, with_test_acp_program};
    use std::sync::Arc;

    let worktree = scratch_dir("session-socket-continuity-worktree");
    let context = provisioned_cursor_session_context(&worktree);
    let script =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_acp.js");
    let session_path = "/api/tasks/web%2Ffix-login/session?model=auto";
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("test runtime");

    with_test_acp_program(&script, || {
        with_test_acp_extra_args(&["--resume", "--remember-context"], || {
            rt.block_on(async {
                let state = super::WebAppState::new(
                    context,
                    OkRunner,
                    TestBridge::default(),
                    scratch_dir("session-socket-continuity-state"),
                );
                let cookie = browser_session_cookie(&state);
                let directory = Arc::clone(&state.task_session_directory);
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                    .await
                    .expect("bind session socket test server");
                let address = listener.local_addr().expect("local addr");
                let server = tokio::spawn(async move {
                    axum::serve(listener, super::axum_app(state))
                        .await
                        .expect("serve");
                });

                let nonce = format!("continuity-nonce-{}", std::process::id());
                let resume_cursor = std::thread::spawn({
                    let path = session_path.to_string();
                    let cookie = cookie.clone();
                    let nonce = nonce.clone();
                    let worktree = worktree.clone();
                    move || {
                        let mut first = BlockingSessionSocket::connect(address, &cookie, &path);
                        let snapshot = first.wait_snapshot(Duration::from_secs(15));
                        assert_eq!(snapshot["type"], "snapshot");
                        assert_eq!(snapshot["contextState"], "live");
                        first.send_prompt(&format!("remember:{nonce}"));
                        first.wait_agent_text(&format!("stored:{nonce}"), Duration::from_secs(15));
                        assert_eq!(
                            std::fs::read_to_string(worktree.join(".fake-acp-context-memory"))
                                .expect("context memory"),
                            nonce
                        );
                        first.resume_cursor()
                    }
                })
                .join()
                .expect("initial session socket thread");

                assert!(
                    directory.has_live_entry("web/fix-login"),
                    "viewer disconnect must keep the live ACP child"
                );

                let reconnect_path = format!("{session_path}&cursor={resume_cursor}");
                std::thread::spawn({
                    let cookie = cookie.clone();
                    let nonce = nonce.clone();
                    move || {
                        std::thread::sleep(Duration::from_millis(100));
                        let mut second =
                            BlockingSessionSocket::connect(address, &cookie, &reconnect_path);
                        let snapshot = second.wait_snapshot(Duration::from_secs(15));
                        assert_eq!(snapshot["reset"], false);
                        assert_eq!(snapshot["turnState"], "idle");
                        assert_eq!(snapshot["contextState"], "live");
                        second.drain_replay(Duration::from_secs(2));
                        second.send_prompt("recall");
                        second
                            .wait_agent_text(&format!("recalled:{nonce}"), Duration::from_secs(15));
                    }
                })
                .join()
                .expect("in-page reconnect socket thread");

                std::thread::spawn({
                    let path = session_path.to_string();
                    move || {
                        std::thread::sleep(Duration::from_millis(100));
                        let mut third = BlockingSessionSocket::connect(address, &cookie, &path);
                        let snapshot = third.wait_snapshot(Duration::from_secs(15));
                        assert!(
                            snapshot["cursor"].as_u64().unwrap_or(0) >= resume_cursor as u64,
                            "cold attach must replay persisted transcript cursor"
                        );
                        third.drain_replay(Duration::from_secs(2));
                        third.send_prompt("recall");
                        third
                            .wait_agent_text(&format!("recalled:{nonce}"), Duration::from_secs(15));
                    }
                })
                .join()
                .expect("cold replay socket thread");

                server.abort();
            });
        });
    });

    let _ = std::fs::remove_dir_all(&worktree);
}
