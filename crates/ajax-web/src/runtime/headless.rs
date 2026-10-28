use std::sync::Arc;
use std::time::Duration;

use ajax_core::{
    adapters::acp_launch_for_agent,
    commands::CommandContext,
    registry::{InMemoryRegistry, Registry as _},
};

use super::state::WebAppState;
use crate::slices::web_session::{
    adoptable_handles, run_headless_dispatch, HeadlessTarget, SessionStorage, TaskSessionDirectory,
    HEARTBEAT_EVERY,
};

const CLAIM_RETRIES: usize = 6;
const MAX_WAIT_PER_CHAT: Duration = Duration::from_secs(30 * 60);

pub(crate) fn targets_for(
    context: &CommandContext<InMemoryRegistry>,
    handles: &[String],
) -> Vec<HeadlessTarget> {
    let mut targets = Vec::new();
    for handle in handles {
        match context.registry.list_tasks().into_iter().find(|task| {
            task.qualified_handle() == *handle
                && task.skip_interactive_agent()
                && acp_launch_for_agent(task.selected_agent).is_some()
        }) {
            Some(task) => targets.push(HeadlessTarget {
                handle: handle.clone(),
                worktree_path: task.worktree_path.clone(),
                model: task.session_model().unwrap_or("auto").to_string(),
                agent: task.selected_agent,
            }),
            None => tracing::debug!(
                target: "ajax_web",
                handle = %handle,
                "headless dispatch: no resumable task for handle"
            ),
        }
    }
    targets
}

pub(crate) fn spawn_startup_dispatch<C, B>(state: &WebAppState<C, B>)
where
    C: Send + 'static,
    B: Send + 'static,
{
    let directory = Arc::clone(&state.task_session_directory);
    let shared = Arc::clone(&state.shared);
    let state_dir = Arc::clone(&state.state_dir);
    tokio::spawn(async move {
        let Some(claimed) = claim_startup_handles(&directory, HEARTBEAT_EVERY).await else {
            return;
        };
        let storage = SessionStorage::under((*state_dir).clone());
        let adoptable = adoptable_handles(&storage.registry_store(), &storage.state_dir);
        let handles = startup_handles(claimed, adoptable);
        if handles.is_empty() {
            return;
        }

        let targets = {
            let guard = shared.lock().expect("web shared state lock poisoned");
            targets_for(&guard.context, &handles)
        };

        let results = run_headless_dispatch(&directory, targets, MAX_WAIT_PER_CHAT).await;
        let failed = results.iter().filter(|(_, result)| result.is_err()).count();
        tracing::info!(
            target: "ajax_web",
            ok = results.len() - failed,
            failed,
            "headless dispatch finished"
        );
    });
}

fn startup_handles(claimed: Vec<String>, adoptable: Vec<String>) -> Vec<String> {
    let mut handles = claimed;
    for handle in adoptable {
        if !handles.contains(&handle) {
            handles.push(handle);
        }
    }
    handles
}

async fn claim_startup_handles(
    directory: &TaskSessionDirectory,
    pause: Duration,
) -> Option<Vec<String>> {
    let mut handles = Vec::new();
    for attempt in 1..=CLAIM_RETRIES {
        match directory.startup_dispatch_handles() {
            Ok(found) => handles.extend(found),
            Err(error) => {
                if attempt == CLAIM_RETRIES {
                    tracing::warn!(
                        target: "ajax_web",
                        attempt,
                        %error,
                        "headless dispatch: giving up on startup claim"
                    );
                    return None;
                }
                tracing::debug!(
                    target: "ajax_web",
                    attempt,
                    %error,
                    "headless dispatch: startup claim failed, retrying"
                );
                tokio::time::sleep(pause).await;
                continue;
            }
        }

        if directory.headless_scan_complete() {
            return Some(handles);
        }
        if attempt == CLAIM_RETRIES {
            tracing::warn!(
                target: "ajax_web",
                attempts = attempt,
                "headless dispatch: startup scan incomplete after retry cap"
            );
            return Some(handles);
        }

        tokio::time::sleep(pause).await;
    }

    unreachable!("claim loop always returns within its bounded attempts")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ajax_core::models::AgentClient;
    use ajax_core::registry::OwnerClaim;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0)
    }

    fn acp_task_context() -> (String, CommandContext<InMemoryRegistry>) {
        let mut task = crate::test_support::fix_login_task();
        task.set_skip_interactive_agent(true);
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        (handle, context)
    }

    #[test]
    fn targets_for_returns_target_for_acp_task() {
        let (handle, context) = acp_task_context();
        let targets = targets_for(&context, std::slice::from_ref(&handle));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].handle, handle);
        assert_eq!(
            targets[0].worktree_path.to_string_lossy().into_owned(),
            "/repo/web__worktrees/ajax-fix-login"
        );
        assert_eq!(targets[0].model, "auto");
        assert_eq!(targets[0].agent, AgentClient::Codex);
    }

    #[test]
    fn startup_handles_appends_adoptable_handles_once() {
        let handles = startup_handles(
            vec!["web/a".to_owned(), "web/b".to_owned()],
            vec!["web/b".to_owned(), "web/c".to_owned()],
        );
        assert_eq!(
            handles,
            vec!["web/a".to_owned(), "web/b".to_owned(), "web/c".to_owned()]
        );
    }

    #[test]
    fn targets_for_skips_unknown_handle() {
        let (_handle, context) = acp_task_context();
        let targets = targets_for(&context, &["web/nope".to_string()]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_skips_non_acp_task() {
        let mut task = ajax_core::models::Task::new(
            ajax_core::models::TaskId::new("web/other-agent"),
            "web",
            "other-agent",
            "Other agent",
            "ajax/other-agent".to_string(),
            "main",
            "/repo/web__worktrees/ajax-other-agent".to_string(),
            "ajax-web-other-agent".to_string(),
            "task",
            AgentClient::Other,
        );
        task.set_skip_interactive_agent(true);
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let targets = targets_for(&context, &[handle]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_skips_interactive_task() {
        let task = crate::test_support::fix_login_task();
        let handle = task.qualified_handle();
        let context = crate::test_support::context_with_tasks(&["web"], vec![task]);
        let targets = targets_for(&context, &[handle]);
        assert!(targets.is_empty());
    }

    #[test]
    fn targets_for_preserves_input_order() {
        let (a_handle, mut context) = acp_task_context();
        let mut b_task = crate::test_support::task_in("web", "fix-payment", "Fix payment");
        b_task.set_skip_interactive_agent(true);
        let b_handle = b_task.qualified_handle();
        context.registry.create_task(b_task).unwrap();

        let targets = targets_for(&context, &[b_handle.clone(), a_handle.clone()]);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].handle, b_handle);
        assert_eq!(targets[1].handle, a_handle);
    }

    fn ledger_file(dir: &std::path::Path, handle: &str) -> std::path::PathBuf {
        let encoded = handle.replace('%', "%25").replace('/', "%2F");
        dir.join("web-session")
            .join(format!("{encoded}.prompt-ledger.json"))
    }

    fn write_queued_sidecar(dir: &std::path::Path, handle: &str) {
        let file = ledger_file(dir, handle);
        std::fs::create_dir_all(file.parent().unwrap()).expect("create session dir");
        std::fs::write(
            file,
            serde_json::json!({
                "kind": "prompt_ledger",
                "v": 1,
                "entries": [{
                    "clientMessageId": format!("{handle}-p1"),
                    "phase": "queued",
                    "transcriptText": format!("transcript {handle}"),
                    "promptText": format!("queued for {handle}"),
                }],
            })
            .to_string(),
        )
        .expect("write ledger sidecar");
    }

    fn seed_foreign_queued(dir: &std::path::Path, handle: &str) {
        use ajax_core::registry::{ExecutionCommit, NewRun, RunUpdate, SqliteRegistryStore};

        let store = SqliteRegistryStore::new(dir.join("ajax.db"));
        let run_id = format!("web-chat:{handle}");
        store
            .create_run(&NewRun {
                run_id: run_id.clone(),
                task_id: handle.to_owned(),
                harness: "claude".to_owned(),
                model: "auto".to_owned(),
                created_unix_ms: 1,
            })
            .expect("create run");
        store
            .commit_execution(&ExecutionCommit {
                run_id,
                run_update: Some(RunUpdate {
                    owner_boot_id: Some(Some("foreign-boot".to_owned())),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .expect("record foreign owner");

        write_queued_sidecar(dir, handle);
    }

    #[tokio::test]
    async fn an_incomplete_startup_scan_is_rescanned_until_complete() {
        let dir = crate::slices::web_session::test_support::scratch_dir("headless-claim-rescan");
        seed_foreign_queued(&dir, "web/a");
        seed_foreign_queued(&dir, "web/b");

        let broken = ledger_file(&dir, "web/b");
        std::fs::write(&broken, "malformed ledger").expect("corrupt ledger");

        let directory = TaskSessionDirectory::new(
            crate::slices::web_session::SessionStorage::under(dir.clone()),
        );
        tokio::spawn({
            let dir = dir.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                write_queued_sidecar(&dir, "web/b");
            }
        });

        let handles = claim_startup_handles(&directory, Duration::from_millis(5))
            .await
            .expect("lease claimed");

        assert_eq!(handles, vec!["web/a".to_owned(), "web/b".to_owned()]);
        assert!(
            directory.headless_scan_complete(),
            "the loop must stop retrying once the scan is complete"
        );
    }

    #[tokio::test]
    async fn a_never_completing_startup_scan_stops_after_the_claim_cap() {
        let dir = crate::slices::web_session::test_support::scratch_dir("headless-claim-cap");
        seed_foreign_queued(&dir, "web/a");
        seed_foreign_queued(&dir, "web/b");

        let broken = ledger_file(&dir, "web/b");
        std::fs::write(&broken, "malformed ledger").expect("corrupt ledger");

        let directory = TaskSessionDirectory::new(
            crate::slices::web_session::SessionStorage::under(dir.clone()),
        );

        let handles = claim_startup_handles(&directory, Duration::from_millis(1))
            .await
            .expect("lease claimed");

        assert_eq!(handles, vec!["web/a".to_owned()]);
        assert!(!directory.headless_scan_complete());
    }

    #[tokio::test]
    async fn a_failing_startup_claim_is_retried_until_the_lease_frees_up() {
        let dir = crate::slices::web_session::test_support::scratch_dir("headless-claim-retry");
        let child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep helper");
        let foreign_pid = child.id() as i64;
        let claim = crate::slices::web_session::SessionStorage::under(dir.clone())
            .registry_store()
            .claim_execution_owner("other", foreign_pid, now_ms(), 30_000)
            .expect("claim lease");
        assert!(matches!(claim, OwnerClaim::Acquired { .. }));

        let directory = TaskSessionDirectory::new(
            crate::slices::web_session::SessionStorage::under(dir.clone()),
        );
        tokio::spawn({
            let mut child = child;
            async move {
                tokio::time::sleep(Duration::from_millis(30)).await;
                child.kill().expect("kill sleep helper");
                child.wait().expect("wait on sleep helper");
            }
        });

        let handles = claim_startup_handles(&directory, Duration::from_millis(50))
            .await
            .expect("the retried claim must succeed once the foreign host dies");

        assert!(handles.is_empty());
        assert!(directory.headless_scan_complete());
    }
}
