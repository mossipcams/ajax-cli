use super::*;
use crate::adapters::web_session_store::prompt_ledger::{
    PromptLedger, PromptLedgerEntry, PromptPhase,
};

fn prompt(key: &str, state: ExecutionState, content_json: Option<&str>) -> RunPrompt {
    RunPrompt {
        idempotency_key: key.to_string(),
        task_exec_id: format!("run-1:prompt:{key}"),
        content_json: content_json.map(str::to_string).unwrap_or_else(|| {
            entry_content_json(&entry(key, PromptPhase::Queued, "t", "p", Vec::new()))
        }),
        state,
    }
}

fn entry(
    key: &str,
    phase: PromptPhase,
    transcript_text: &str,
    prompt_text: &str,
    content_blocks: Vec<serde_json::Value>,
) -> PromptLedgerEntry {
    PromptLedgerEntry {
        client_message_id: key.to_string(),
        phase,
        transcript_text: transcript_text.to_string(),
        prompt_text: prompt_text.to_string(),
        content_blocks,
    }
}

fn ledger(entries: Vec<PromptLedgerEntry>) -> PromptLedger {
    PromptLedger { entries }
}

#[test]
fn new_queued_prompt_creates_task_and_prompt() {
    let desired = ledger(vec![entry("m1", PromptPhase::Queued, "t", "p", Vec::new())]);
    let commit = plan_commit("run-1", &[], &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.run_id, "run-1");
    assert_eq!(commit.new_tasks.len(), 1);
    assert_eq!(commit.new_tasks[0].task_exec_id, "run-1:prompt:m1");
    assert_eq!(commit.new_tasks[0].kind, "prompt");
    assert!(commit.new_tasks[0].parent_id.is_none());
    assert_eq!(commit.prompts.len(), 1);
    assert_eq!(commit.prompts[0].idempotency_key, "m1");
    assert_eq!(commit.prompts[0].task_exec_id, "run-1:prompt:m1");
    assert!(commit.transitions.is_empty());
    assert!(commit.checkpoint_intents.is_empty());
}

#[test]
fn new_prompt_already_dispatching_transitions_and_sets_intent() {
    let current = Vec::<RunPrompt>::new();
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Dispatching,
        "t",
        "p",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.new_tasks.len(), 1);
    assert_eq!(commit.prompts.len(), 1);
    assert_eq!(commit.transitions.len(), 1);
    assert_eq!(commit.transitions[0].to, ExecutionState::Running);
    assert_eq!(commit.transitions[0].task_exec_id, "run-1:prompt:m1");
    assert_eq!(commit.checkpoint_intents.len(), 1);
    assert_eq!(
        commit.checkpoint_intents[0].operation,
        Operation::PromptDispatch
    );
    assert_eq!(
        commit.checkpoint_intents[0].replay_policy,
        ReplayPolicy::Unsafe
    );
}

#[test]
fn queued_to_dispatching_adds_intent() {
    let current = vec![prompt("m1", ExecutionState::Pending, None)];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Dispatching,
        "t",
        "p",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert!(commit.new_tasks.is_empty());
    assert_eq!(commit.transitions.len(), 1);
    assert_eq!(commit.transitions[0].to, ExecutionState::Running);
    assert_eq!(commit.checkpoint_intents.len(), 1);
}

#[test]
fn dispatching_to_completed_adds_result_two_transitions_and_a_clear() {
    let current = vec![prompt("m1", ExecutionState::Running, None)];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Completed,
        "t",
        "p",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.transitions.len(), 2);
    assert_eq!(commit.transitions[0].to, ExecutionState::Completing);
    assert_eq!(commit.transitions[1].to, ExecutionState::Completed);
    assert_eq!(commit.checkpoint_results.len(), 1);
    assert_eq!(commit.checkpoint_results[0].task_exec_id, "run-1:prompt:m1");
    assert_eq!(commit.checkpoint_results[0].result_json, "{}");
    assert!(commit.checkpoint_intents.is_empty());
    assert_eq!(commit.prompt_content_clears, vec!["m1".to_string()]);
}

#[test]
fn dispatching_to_interrupted() {
    let current = vec![prompt("m1", ExecutionState::Running, None)];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Interrupted,
        "t",
        "p",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.transitions.len(), 1);
    assert_eq!(commit.transitions[0].to, ExecutionState::Interrupted);
    assert!(commit.checkpoint_intents.is_empty());
    assert!(commit.checkpoint_results.is_empty());
}

#[test]
fn interrupted_to_queued() {
    let current = vec![prompt("m1", ExecutionState::Interrupted, None)];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Queued,
        "t2",
        "p2",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.transitions.len(), 1);
    assert_eq!(commit.transitions[0].to, ExecutionState::Pending);
    // Content changed and target is Pending: content update.
    assert_eq!(commit.prompt_content_updates.len(), 1);
    assert_eq!(commit.prompt_content_updates[0].idempotency_key, "m1");
}

#[test]
fn requeue_after_failed_begin_adds_two_transitions_and_content_update() {
    let current = vec![prompt("m1", ExecutionState::Running, Some("{}"))];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Queued,
        "t2",
        "p2",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.transitions.len(), 2);
    assert_eq!(commit.transitions[0].to, ExecutionState::Interrupted);
    assert_eq!(commit.transitions[1].to, ExecutionState::Pending);
    assert_eq!(commit.prompt_content_updates.len(), 1);
}

#[test]
fn edit_of_queued_prompt_adds_only_content_update() {
    let current = vec![prompt("m1", ExecutionState::Pending, Some("{}"))];
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Queued,
        "t2",
        "p2",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &current, &desired)
        .unwrap()
        .expect("commit");
    assert!(commit.transitions.is_empty());
    assert_eq!(commit.prompt_content_updates.len(), 1);
    assert!(commit.checkpoint_intents.is_empty());
}

#[test]
fn removal_of_queued_prompt_cancels_it_and_completed_removal_does_nothing() {
    let current = vec![
        prompt("m1", ExecutionState::Pending, None),
        prompt("m2", ExecutionState::Completed, Some("{}")),
    ];
    assert!(plan_commit("run-1", &current, &ledger(Vec::new()))
        .unwrap()
        .is_some());

    let commit = plan_commit("run-1", &current, &ledger(Vec::new()))
        .unwrap()
        .expect("commit");
    assert_eq!(commit.transitions.len(), 1);
    assert_eq!(commit.transitions[0].task_exec_id, "run-1:prompt:m1");
    assert_eq!(commit.transitions[0].to, ExecutionState::Cancelled);
    assert_eq!(commit.prompt_content_clears, vec!["m1".to_string()]);

    let current_completed_only = vec![prompt("m2", ExecutionState::Completed, Some("{}"))];
    assert!(
        plan_commit("run-1", &current_completed_only, &ledger(Vec::new()))
            .unwrap()
            .is_none()
    );
}

#[test]
fn new_prompt_admitted_as_completed_stores_empty_content() {
    let desired = ledger(vec![entry(
        "m1",
        PromptPhase::Completed,
        "t",
        "p",
        Vec::new(),
    )]);
    let commit = plan_commit("run-1", &[], &desired)
        .unwrap()
        .expect("commit");
    assert_eq!(commit.prompts.len(), 1);
    assert_eq!(commit.prompts[0].content_json, "{}");
    assert!(commit.prompt_content_clears.is_empty());
}

#[test]
fn completed_prompt_with_stale_content_yields_only_a_clear() {
    let current = vec![prompt("m1", ExecutionState::Completed, None)];
    let commit = plan_commit("run-1", &current, &ledger(Vec::new()))
        .unwrap()
        .expect("commit with a clear for pre-fix content");
    assert!(commit.transitions.is_empty());
    assert!(commit.prompts.is_empty());
    assert_eq!(commit.prompt_content_clears, vec!["m1".to_string()]);
}

#[test]
fn completed_prompt_with_empty_content_yields_none() {
    let current = vec![prompt("m1", ExecutionState::Completed, Some("{}"))];
    assert!(plan_commit("run-1", &current, &ledger(Vec::new()))
        .unwrap()
        .is_none());
}

#[test]
fn identical_state_returns_none() {
    let current = vec![prompt(
        "m1",
        ExecutionState::Pending,
        Some(&entry_content_json(&entry(
            "m1",
            PromptPhase::Queued,
            "t",
            "p",
            Vec::new(),
        ))),
    )];
    let desired = ledger(vec![entry("m1", PromptPhase::Queued, "t", "p", Vec::new())]);
    assert!(plan_commit("run-1", &current, &desired).unwrap().is_none());
}

#[test]
fn readding_cancelled_key_is_an_error() {
    let current = vec![prompt("m1", ExecutionState::Cancelled, None)];
    let desired = ledger(vec![entry("m1", PromptPhase::Queued, "t", "p", Vec::new())]);
    assert!(plan_commit("run-1", &current, &desired).is_err());

    let current_failed = vec![prompt("m1", ExecutionState::Failed, None)];
    assert!(plan_commit("run-1", &current_failed, &desired).is_err());
}

#[test]
fn completed_to_queued_is_an_error() {
    let current = vec![prompt("m1", ExecutionState::Completed, None)];
    let desired = ledger(vec![entry("m1", PromptPhase::Queued, "t", "p", Vec::new())]);
    assert!(plan_commit("run-1", &current, &desired).is_err());
}

#[test]
fn ledger_from_prompts_round_trip() {
    let prompts = vec![
        prompt(
            "m1",
            ExecutionState::Pending,
            Some(&entry_content_json(&entry(
                "m1",
                PromptPhase::Queued,
                "t1",
                "p1",
                Vec::new(),
            ))),
        ),
        prompt(
            "m2",
            ExecutionState::Running,
            Some(&entry_content_json(&entry(
                "m2",
                PromptPhase::Dispatching,
                "t2",
                "p2",
                vec![serde_json::json!({"a": 1})],
            ))),
        ),
        prompt(
            "m3",
            ExecutionState::Interrupted,
            Some(&entry_content_json(&entry(
                "m3",
                PromptPhase::Interrupted,
                "t3",
                "p3",
                Vec::new(),
            ))),
        ),
        prompt(
            "m4",
            ExecutionState::Completed,
            Some(&entry_content_json(&entry(
                "m4",
                PromptPhase::Completed,
                "hidden-t",
                "hidden-p",
                Vec::new(),
            ))),
        ),
        prompt("m5", ExecutionState::Cancelled, None),
        prompt("m6", ExecutionState::Failed, None),
    ];

    let ledger = ledger_from_prompts(&prompts);
    assert_eq!(ledger.entries.len(), 4);
    assert_eq!(ledger.entries[0].client_message_id, "m1");
    assert_eq!(ledger.entries[0].phase, PromptPhase::Queued);
    assert_eq!(ledger.entries[0].transcript_text, "t1");
    assert_eq!(ledger.entries[0].prompt_text, "p1");

    assert_eq!(ledger.entries[1].client_message_id, "m2");
    assert_eq!(ledger.entries[1].phase, PromptPhase::Dispatching);
    assert_eq!(
        ledger.entries[1].content_blocks,
        vec![serde_json::json!({"a": 1})]
    );

    assert_eq!(ledger.entries[2].phase, PromptPhase::Interrupted);

    let completed = &ledger.entries[3];
    assert_eq!(completed.client_message_id, "m4");
    assert_eq!(completed.phase, PromptPhase::Completed);
    assert_eq!(completed.transcript_text, "");
    assert_eq!(completed.prompt_text, "");
    assert!(completed.content_blocks.is_empty());

    // Malformed content falls back to empty text fields.
    let malformed = prompt("m7", ExecutionState::Pending, Some("not json"));
    let ledger = ledger_from_prompts(&[malformed]);
    assert_eq!(ledger.entries[0].transcript_text, "");
    assert_eq!(ledger.entries[0].prompt_text, "");
    assert!(ledger.entries[0].content_blocks.is_empty());
}
