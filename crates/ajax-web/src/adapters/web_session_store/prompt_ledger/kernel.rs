//! Pure mapping between the Ajax Chat prompt ledger and the durable execution
//! kernel. No I/O, no database calls: these functions translate ledger state
//! into a single [`ExecutionCommit`] and back.

use ajax_core::execution::{ExecutionState, Operation, ReplayPolicy};
use ajax_core::registry::{
    CheckpointIntent, CheckpointResult, ExecutionCommit, NewExecutionTask, NewPrompt,
    PromptContentUpdate, RunPrompt, TaskTransition,
};

use super::{PromptLedger, PromptLedgerEntry, PromptPhase};

fn task_exec_id(run_id: &str, key: &str) -> String {
    format!("{run_id}:prompt:{key}")
}

/// Serialize a ledger entry into the kernel `content_json` envelope.
#[allow(dead_code)] // Nothing calls this yet; wiring lands in packet 4b-A2.
pub(super) fn entry_content_json(entry: &PromptLedgerEntry) -> String {
    serde_json::json!({
        "transcript_text": entry.transcript_text,
        "prompt_text": entry.prompt_text,
        "content_blocks": entry.content_blocks,
    })
    .to_string()
}

fn content_fields(content_json: &str) -> (String, String, Vec<serde_json::Value>) {
    let object = serde_json::from_str::<serde_json::Value>(content_json)
        .ok()
        .and_then(|value| value.as_object().cloned());
    match object {
        Some(object) => (
            object
                .get("transcript_text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            object
                .get("prompt_text")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            object
                .get("content_blocks")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        ),
        None => (String::new(), String::new(), Vec::new()),
    }
}

/// Rebuild a ledger from the stored prompts, preserving order. Failed and
/// Cancelled prompts are omitted; Completed entries have their text cleared.
#[allow(dead_code)] // Nothing calls this yet; wiring lands in packet 4b-A2.
pub(super) fn ledger_from_prompts(prompts: &[RunPrompt]) -> PromptLedger {
    let mut entries = Vec::with_capacity(prompts.len());
    for prompt in prompts {
        if matches!(
            prompt.state,
            ExecutionState::Failed | ExecutionState::Cancelled
        ) {
            continue;
        }
        let (transcript_text, prompt_text, content_blocks) =
            if prompt.state == ExecutionState::Completed {
                (String::new(), String::new(), Vec::new())
            } else {
                content_fields(&prompt.content_json)
            };
        let phase = match prompt.state {
            ExecutionState::Pending => PromptPhase::Queued,
            ExecutionState::Running | ExecutionState::Waiting | ExecutionState::Completing => {
                PromptPhase::Dispatching
            }
            ExecutionState::Interrupted => PromptPhase::Interrupted,
            ExecutionState::Completed => PromptPhase::Completed,
            // Failed and Cancelled are skipped above.
            _ => unreachable!("failed/cancelled prompts are omitted"),
        };
        entries.push(PromptLedgerEntry {
            client_message_id: prompt.idempotency_key.clone(),
            phase,
            transcript_text,
            prompt_text,
            content_blocks,
        });
    }
    PromptLedger { entries }
}

fn target_state(phase: PromptPhase) -> ExecutionState {
    match phase {
        PromptPhase::Queued => ExecutionState::Pending,
        PromptPhase::Dispatching => ExecutionState::Running,
        PromptPhase::Interrupted => ExecutionState::Interrupted,
        PromptPhase::Completed => ExecutionState::Completed,
    }
}

/// Ordered intermediate states (inclusive of the target) that move `from` to
/// `to`, or an error pair for unsupported changes. An empty path means the
/// state is already correct.
fn transition_path(
    from: ExecutionState,
    to: ExecutionState,
) -> Result<Vec<ExecutionState>, (ExecutionState, ExecutionState)> {
    use ExecutionState::*;
    if from == to {
        return Ok(Vec::new());
    }
    let steps = match (from, to) {
        (Pending, Running) => vec![Running],
        (Pending, Interrupted) => vec![Running, Interrupted],
        (Pending, Completed) => vec![Running, Completing, Completed],
        (Running, Pending) => vec![Interrupted, Pending],
        (Running, Interrupted) => vec![Interrupted],
        (Running, Completed) => vec![Completing, Completed],
        (Interrupted, Pending) => vec![Pending],
        (Interrupted, Running) => vec![Pending, Running],
        (Interrupted, Completed) => vec![Pending, Running, Completing, Completed],
        _ => return Err((from, to)),
    };
    Ok(steps)
}

/// Compute the single kernel commit that makes the stored prompts match the
/// desired ledger. Returns `Ok(None)` when nothing would change.
#[allow(dead_code)] // Nothing calls this yet; wiring lands in packet 4b-A2.
pub(super) fn plan_commit(
    run_id: &str,
    current: &[RunPrompt],
    desired: &PromptLedger,
) -> Result<Option<ExecutionCommit>, String> {
    let mut commit = ExecutionCommit::default();

    for entry in &desired.entries {
        let key = entry.client_message_id.as_str();
        let exec_id = task_exec_id(run_id, key);
        let mut existing_content_json: Option<String> = None;
        match current.iter().find(|prompt| prompt.idempotency_key == key) {
            None => {
                commit.new_tasks.push(NewExecutionTask {
                    task_exec_id: exec_id.clone(),
                    parent_id: None,
                    kind: "prompt".into(),
                });
                commit.prompts.push(NewPrompt {
                    idempotency_key: key.to_string(),
                    task_exec_id: exec_id.clone(),
                    content_json: entry_content_json(entry),
                });
            }
            Some(prompt) => {
                if matches!(
                    prompt.state,
                    ExecutionState::Failed | ExecutionState::Cancelled
                ) {
                    return Err(format!("prompt {key} was already withdrawn"));
                }
                existing_content_json = Some(prompt.content_json.clone());
            }
        }

        // New prompts start in Pending.
        let start = current
            .iter()
            .find(|prompt| prompt.idempotency_key == key)
            .map(|prompt| prompt.state)
            .unwrap_or(ExecutionState::Pending);
        let target = target_state(entry.phase);
        let steps = transition_path(start, target).map_err(|(from, to)| {
            format!("unsupported prompt state change {from:?} -> {to:?} for {key}")
        })?;
        if !steps.is_empty() && matches!(steps.last(), Some(ExecutionState::Running)) {
            commit.checkpoint_intents.push(CheckpointIntent {
                task_exec_id: exec_id.clone(),
                operation: Operation::PromptDispatch,
                replay_policy: ReplayPolicy::Unsafe,
                external_operation_id: None,
                intent_json: "{}".into(),
            });
        }
        if start == ExecutionState::Running && target == ExecutionState::Completed {
            commit.checkpoint_results.push(CheckpointResult {
                task_exec_id: exec_id.clone(),
                result_json: "{}".into(),
            });
        }
        for step in steps {
            commit.transitions.push(TaskTransition {
                task_exec_id: exec_id.clone(),
                to: step,
                outcome_json: None,
            });
        }

        if let Some(current_content) = existing_content_json {
            if target == ExecutionState::Pending && entry_content_json(entry) != current_content {
                commit.prompt_content_updates.push(PromptContentUpdate {
                    idempotency_key: key.to_string(),
                    content_json: entry_content_json(entry),
                });
            }
        }
    }

    let desired_keys = desired
        .entries
        .iter()
        .map(|entry| entry.client_message_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for prompt in current {
        if !desired_keys.contains(prompt.idempotency_key.as_str()) && !prompt.state.is_terminal() {
            commit.transitions.push(TaskTransition {
                task_exec_id: task_exec_id(run_id, &prompt.idempotency_key),
                to: ExecutionState::Cancelled,
                outcome_json: None,
            });
        }
    }

    let empty = commit.new_tasks.is_empty()
        && commit.prompts.is_empty()
        && commit.transitions.is_empty()
        && commit.cancel_requests.is_empty()
        && commit.checkpoint_intents.is_empty()
        && commit.checkpoint_results.is_empty()
        && commit.prompt_content_updates.is_empty()
        && commit.events.is_empty();
    if empty {
        Ok(None)
    } else {
        commit.run_id = run_id.to_string();
        Ok(Some(commit))
    }
}

#[cfg(test)]
mod tests {
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
    fn dispatching_to_completed_adds_result_and_two_transitions() {
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
            prompt("m2", ExecutionState::Completed, None),
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

        let current_completed_only = vec![prompt("m2", ExecutionState::Completed, None)];
        assert!(
            plan_commit("run-1", &current_completed_only, &ledger(Vec::new()))
                .unwrap()
                .is_none()
        );
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
}
