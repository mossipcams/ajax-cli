use ajax_core::execution::{ExecutionState, Operation, ReplayPolicy};
use ajax_core::registry::{
    CheckpointIntent, CheckpointResult, ExecutionCommit, NewExecutionTask, NewPrompt,
    PromptContentUpdate, RunPrompt, TaskTransition,
};

use super::{PromptLedger, PromptLedgerEntry, PromptPhase};

fn task_exec_id(run_id: &str, key: &str) -> String {
    format!("{run_id}:prompt:{key}")
}

#[allow(dead_code)]
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
                    content_json: if entry.phase == PromptPhase::Completed {
                        "{}".to_string()
                    } else {
                        entry_content_json(entry)
                    },
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

    let mut to_clear: Vec<&str> = desired
        .entries
        .iter()
        .filter(|entry| {
            target_state(entry.phase) == ExecutionState::Completed
                && current.iter().any(|prompt| {
                    prompt.idempotency_key == entry.client_message_id && prompt.content_json != "{}"
                })
        })
        .map(|entry| entry.client_message_id.as_str())
        .collect();
    to_clear.extend(
        current
            .iter()
            .filter(|prompt| {
                !desired_keys.contains(prompt.idempotency_key.as_str())
                    && prompt.content_json != "{}"
            })
            .map(|prompt| prompt.idempotency_key.as_str()),
    );
    commit.prompt_content_clears = to_clear.into_iter().map(str::to_string).collect();

    let empty = commit.new_tasks.is_empty()
        && commit.prompts.is_empty()
        && commit.transitions.is_empty()
        && commit.cancel_requests.is_empty()
        && commit.checkpoint_intents.is_empty()
        && commit.checkpoint_results.is_empty()
        && commit.prompt_content_updates.is_empty()
        && commit.prompt_content_clears.is_empty()
        && commit.events.is_empty();
    if empty {
        Ok(None)
    } else {
        commit.run_id = run_id.to_string();
        Ok(Some(commit))
    }
}

#[cfg(test)]
#[path = "kernel_tests.rs"]
mod tests;
