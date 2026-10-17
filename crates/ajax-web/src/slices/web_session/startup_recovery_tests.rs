//! Tests for startup recovery of prompts cut off by a host restart. Each
//! nextest case runs in its own process, so scratch directories cannot collide
//! between tests.

use std::path::{Path, PathBuf};

use ajax_core::registry::SqliteRegistryStore;

use super::startup_recovery::{queued_prompt_handles, recover_interrupted_prompts};
use crate::adapters::web_session_store::prompt_ledger::{self, PromptLedger, PromptPhase};

/// A scratch state dir plus a fresh registry store inside it.
fn scratch_store(label: &str) -> (PathBuf, SqliteRegistryStore) {
    let dir = std::env::temp_dir().join(format!(
        "ajax-startup-recovery-{}-{label}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    let store = SqliteRegistryStore::new(dir.join("registry.db"));
    (dir, store)
}

/// Set the run row's owning boot id exactly as a live session would.
fn record_owner(store: &SqliteRegistryStore, state_dir: &Path, handle: &str, owner_boot_id: &str) {
    crate::adapters::web_session_store::record_child_identity(
        store,
        state_dir,
        handle,
        "acp",
        None,
        owner_boot_id,
    )
    .expect("record child identity");
}

/// Persist a ledger with `dispatching` prompts in the Dispatching phase and
/// `queued` prompts in the Queued phase.
fn persist_ledger(
    store: &SqliteRegistryStore,
    state_dir: &Path,
    handle: &str,
    dispatching: &[&str],
    queued: &[&str],
) {
    let mut ledger = PromptLedger::default();
    for id in dispatching {
        ledger.upsert_queued(
            (*id).to_owned(),
            format!("transcript {id}"),
            format!("prompt {id}"),
            Vec::new(),
        );
        assert!(ledger.mark_dispatching(id));
    }
    for id in queued {
        ledger.upsert_queued(
            (*id).to_owned(),
            format!("transcript {id}"),
            format!("prompt {id}"),
            Vec::new(),
        );
    }
    prompt_ledger::try_persist(store, state_dir, handle, &ledger).expect("persist ledger");
}

fn interruption_note_count(store: &SqliteRegistryStore, state_dir: &Path, handle: &str) -> usize {
    let session = crate::adapters::web_session_store::try_load::<serde_json::Value>(
        store, state_dir, handle, "acp",
    )
    .expect("load transcript");
    session
        .events
        .iter()
        .filter(|event| {
            serde_json::to_string(event)
                .ok()
                .is_some_and(|text| text.contains("was interrupted by host restart"))
        })
        .count()
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn foreign_boot_dispatching_prompt_is_interrupted_with_note() {
    let (dir, store) = scratch_store("dispatching");
    let handle = "chat-foreign";
    record_owner(&store, &dir, handle, "other-boot");
    persist_ledger(&store, &dir, handle, &["p1"], &["p2"]);

    let recovered = recover_interrupted_prompts(&store, &dir, "this-boot");
    assert!(recovered.complete);
    assert_eq!(recovered.recovered_handles, vec![handle.to_string()]);

    let ledger = prompt_ledger::try_load(&store, &dir, handle).expect("load ledger");
    assert_eq!(ledger.entry("p1").unwrap().phase, PromptPhase::Interrupted);
    assert_eq!(ledger.entry("p2").unwrap().phase, PromptPhase::Queued);

    let session = crate::adapters::web_session_store::try_load::<serde_json::Value>(
        &store, &dir, handle, "acp",
    )
    .expect("load transcript");
    let expected = "Prompt p1 was interrupted by host restart and was not retried.";
    assert!(session.events.iter().any(|event| {
        serde_json::to_string(event)
            .ok()
            .is_some_and(|text| text.contains(expected))
    }));
    assert_eq!(interruption_note_count(&store, &dir, handle), 1);

    cleanup(&dir);
}

#[test]
fn recover_is_idempotent() {
    let (dir, store) = scratch_store("idempotent");
    let handle = "chat-foreign";
    record_owner(&store, &dir, handle, "other-boot");
    persist_ledger(&store, &dir, handle, &["p1"], &["p2"]);

    assert_eq!(
        recover_interrupted_prompts(&store, &dir, "this-boot").recovered_handles,
        vec![handle.to_string()]
    );
    assert_eq!(
        recover_interrupted_prompts(&store, &dir, "this-boot").recovered_handles,
        Vec::<String>::new()
    );
    assert_eq!(interruption_note_count(&store, &dir, handle), 1);

    cleanup(&dir);
}

#[test]
fn current_boot_run_is_untouched() {
    let (dir, store) = scratch_store("current");
    let handle = "chat-live";
    record_owner(&store, &dir, handle, "this-boot");
    persist_ledger(&store, &dir, handle, &["p1"], &[]);

    assert_eq!(
        recover_interrupted_prompts(&store, &dir, "this-boot").recovered_handles,
        Vec::<String>::new()
    );

    let ledger = prompt_ledger::try_load(&store, &dir, handle).expect("load ledger");
    assert_eq!(ledger.entry("p1").unwrap().phase, PromptPhase::Dispatching);
    assert_eq!(interruption_note_count(&store, &dir, handle), 0);

    cleanup(&dir);
}

#[test]
fn foreign_boot_queued_prompt_is_listed() {
    let (dir, store) = scratch_store("queued-foreign");
    let handle = "chat-foreign";
    record_owner(&store, &dir, handle, "other-boot");
    persist_ledger(&store, &dir, handle, &[], &["p1"]);

    assert_eq!(
        queued_prompt_handles(&store, &dir, "this-boot"),
        vec![handle.to_string()]
    );

    cleanup(&dir);
}

#[test]
fn foreign_boot_completed_only_is_not_listed() {
    let (dir, store) = scratch_store("queued-completed");
    let handle = "chat-foreign";
    record_owner(&store, &dir, handle, "other-boot");
    let mut ledger = PromptLedger::default();
    ledger.upsert_queued(
        "p1".to_owned(),
        "transcript p1".to_owned(),
        "prompt p1".to_owned(),
        Vec::new(),
    );
    assert!(ledger.mark_dispatching("p1"));
    assert!(ledger.mark_completed("p1"));
    prompt_ledger::try_persist(&store, &dir, handle, &ledger).expect("persist ledger");

    assert_eq!(
        queued_prompt_handles(&store, &dir, "this-boot"),
        Vec::<String>::new()
    );

    cleanup(&dir);
}

#[test]
fn own_boot_queued_prompt_is_not_listed() {
    let (dir, store) = scratch_store("queued-own");
    let handle = "chat-live";
    record_owner(&store, &dir, handle, "this-boot");
    persist_ledger(&store, &dir, handle, &[], &["p1"]);

    assert_eq!(
        queued_prompt_handles(&store, &dir, "this-boot"),
        Vec::<String>::new()
    );

    cleanup(&dir);
}

#[test]
fn queued_prompt_handles_are_sorted() {
    let (dir, store) = scratch_store("queued-sorted");
    record_owner(&store, &dir, "chat-z", "other-boot");
    persist_ledger(&store, &dir, "chat-z", &[], &["pz"]);
    record_owner(&store, &dir, "chat-a", "other-boot");
    persist_ledger(&store, &dir, "chat-a", &[], &["pa"]);

    assert_eq!(
        queued_prompt_handles(&store, &dir, "this-boot"),
        vec!["chat-a".to_string(), "chat-z".to_string()]
    );

    cleanup(&dir);
}

#[test]
fn sweep_once_retries_ledger_load_failure_issue_1326() {
    recovery_retries_failure(false);
}

#[test]
fn sweep_once_retries_ledger_persist_failure_issue_1326() {
    recovery_retries_failure(true);
}

fn recovery_retries_failure(persist_failure: bool) {
    let (dir, store) = scratch_store("recovery-retry");
    for handle in ["chat-a", "chat-b"] {
        record_owner(&store, &dir, handle, "other-boot");
        persist_ledger(&store, &dir, handle, &["p1"], &["p2"]);
    }
    let broken_ledger = prompt_ledger::ledger_path(&dir, "chat-b");
    let failure = if persist_failure {
        Some(prompt_ledger::ForcePersistFailGuard::enable())
    } else {
        std::fs::create_dir_all(broken_ledger.parent().unwrap()).unwrap();
        std::fs::write(&broken_ledger, "malformed ledger").unwrap();
        assert!(prompt_ledger::try_load(&store, &dir, "chat-b").is_err());
        None
    };
    let first = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    drop(failure);
    assert!(!first.complete);
    if !persist_failure {
        std::fs::remove_file(broken_ledger).unwrap();
        assert_eq!(first.recovered_handles, vec!["chat-a".to_string()]);
    } else {
        assert!(first.recovered_handles.is_empty());
    }
    let pending = prompt_ledger::try_load(&store, &dir, "chat-b").unwrap();
    assert_eq!(pending.entry("p1").unwrap().phase, PromptPhase::Dispatching);
    assert_eq!(interruption_note_count(&store, &dir, "chat-b"), 0);

    let second = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    assert!(second.complete);
    let mut recovered = first.recovered_handles;
    recovered.extend(second.recovered_handles);
    assert_eq!(recovered, vec!["chat-a".to_string(), "chat-b".to_string()]);
    for handle in ["chat-a", "chat-b"] {
        let ledger = prompt_ledger::try_load(&store, &dir, handle).unwrap();
        assert_eq!(ledger.entry("p1").unwrap().phase, PromptPhase::Interrupted);
        assert_eq!(ledger.entry("p2").unwrap().phase, PromptPhase::Queued);
        assert_eq!(interruption_note_count(&store, &dir, handle), 1);
    }
    let third = super::orphan_sweep::sweep_once(&store, &dir, "this-boot");
    assert!(third.complete);
    assert!(third.recovered_handles.is_empty());
    cleanup(&dir);
}
