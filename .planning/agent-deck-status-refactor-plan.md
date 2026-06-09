# Agent-Deck Status Logic Refactor Plan

Date: 2026-06-09

## Goal

Refactor Ajax agent-status derivation to adopt the strongest parts of
agent-deck's status model while preserving Ajax's lifecycle/runtime authority
model and current public entry points.

The behavioral reference is agent-deck commit
`a30a47e6af21052cccbff5724b3b5e2bc2845af2`, principally:

- `internal/sessionstatus/sessionstatus.go`
- `internal/session/instance.go` (`UpdateStatus`)
- `internal/tmux/tmux.go` (`GetStatus`)
- `internal/tmux/detector.go`

No file under a `tests/` directory will be modified. All tests named below are
module-local `#[cfg(test)]` tests in the listed source files.

The numbered tasks are thematic checkpoints. During execution, each named test
or tightly coupled test pair is one 5-15 minute TDD unit: write it, run the
focused failure, implement only enough to pass, rerun it green, then continue.

## Verified Baseline and Holes

The implementation must account for these facts from the current Ajax code:

1. `crates/ajax-cli/src/agent_status_cache.rs` does **not** discard hook entries
   after 30 seconds. It returns them with `fresh: false`. The problem is that
   the adapter applies one 30-second freshness judgment before core knows the
   selected agent or hook state. The refactor must move hook freshness policy
   into core without describing this as a retention bug.
2. `runtime_refresh.rs` currently selects the newest `fresh` entry before
   parsing it. A newer malformed value can therefore hide an older valid value.
3. Equal timestamps currently have no documented source or state tie-breaker.
   Vector order must not decide visible status.
4. `classify_pane` checks the Codex idle composer before the rest of the pane.
   It is generic, while `slices::pane` already receives `AgentClient` and then
   separately overlays structured prompts.
5. `events.rs` also calls the generic pane classifier for agent messages and
   tool names. An agent-aware entry point needs compatibility wrappers and
   caller-level tests, not only classifier unit tests.
6. `mark_task_opened` is currently a no-op after validating the task exists.
   Acknowledgment must not change lifecycle.
7. Reusing `ShellIdle` for acknowledged waiting would be wrong. The current
   `live_application` reducer can turn a waiting agent into `Dead`, and it does
   not clear `SideFlag::NeedsInput`. Acknowledgment needs an explicit reducer
   that clears actionable attention without fabricating shell or process state.
8. Raw waiting state is consumed in `ui_state.rs`, `attention.rs`, command
   projections, and cached annotations. Tests must cover all of those surfaces
   so an acknowledged task cannot remain in the inbox through a stale side flag
   or cached annotation.
9. Adding a timestamp to `Task` requires SQLite migration, serde compatibility,
   and an explicit concurrent-save result. This plan treats simultaneous
   acknowledgment and live-status edits to the same task as incompatible and
   requires a visible revision conflict rather than silent overwrite.
10. Existing tests already cover basic cache reads, stale marking, wrapper/task
    merging, wrapper completion beating a stale hook, stable-cache pane skips,
    pane probe failure, and runtime command budgets. New tests must extend those
    contracts instead of duplicating them.

## Required Decision Table

Core will implement and test this order. A later row may only run when every
earlier row is absent or ineligible.

| Priority | Evidence | Rule |
| --- | --- | --- |
| 1 | Missing worktree/session/task window | Preserve the missing-substrate observation and dead runtime state. |
| 2 | Runtime-wrapper `done` / `failed` | Trusted terminal evidence applies regardless of age. |
| 3 | Runtime-wrapper `starting` / `working` | Apply only through the existing 30-second heartbeat window. |
| 4 | Hook evidence for selected agent | Codex `working` uses 20 seconds; Codex `wait`/`ask` uses 120 seconds; Claude hook states use 120 seconds; `AgentClient::Other` ignores hooks. |
| 5 | Agent-aware pane capture | Busy indicators first, then actionable prompts, explicit completion/failure, passive/unknown fallback. |
| 6 | Prior credible observation | Unknown, malformed, stale, or failed probes preserve prior credible state. |

Freshness windows are inclusive: evidence exactly at 20 or 120 seconds is
eligible; evidence one nanosecond beyond the boundary is stale. A future
timestamp is treated as fresh, matching the current clock-skew behavior and the
agent-deck reference. For equal timestamps, source priority follows the table;
within hook evidence, active/busy beats waiting, waiting beats approval, and a
malformed value never participates in the tie.

For acknowledgment:

- Opening a Claude task records `attention_acknowledged_at`.
- If its current state is waiting for input/approval, acknowledgment clears the
  waiting live state, `AgentRuntimeStatus::Waiting`, and
  `SideFlag::NeedsInput` through a core reducer, leaving lifecycle unchanged.
- A Claude waiting hook at or before the acknowledgment is ignored.
- Claude waiting evidence after the acknowledgment becomes actionable again.
- Codex waiting remains actionable despite acknowledgment while the hook is
  fresh; once stale, normal pane fallback decides the current state.
- Running, failed, missing-substrate, reviewable, or merged state is not erased
  merely because the task was opened.

## Task 1: Add the Pure Hook Freshness Contract

### Failing behavior tests to write

Add these tests to `crates/ajax-core/src/live.rs`:

- `codex_working_hook_is_fresh_through_twenty_seconds`
  - Use `now = UNIX_EPOCH + 1_000s` and a Codex hook value `working` observed at
    exactly `now - 20s`.
  - Assert the decision applies `LiveStatusKind::AgentRunning`.
- `codex_working_hook_is_stale_after_twenty_seconds`
  - Use `observed_at = now - 20s - 1ns`.
  - Assert the hook is not applied and a prior `WaitingForInput` observation is
    preserved.
- `codex_wait_hook_is_fresh_through_two_minutes`
  - Test Ajax value `wait` at exactly 120 seconds.
  - Assert `WaitingForInput` is applied.
- `codex_wait_hook_is_stale_after_two_minutes`
  - Test `wait` at 120 seconds plus one nanosecond.
  - Assert the decision falls through without replacing the prior observation.
- `claude_working_hook_uses_two_minute_window`
  - Assert Claude `working` applies at 120 seconds and is stale at 120 seconds
    plus one nanosecond.
- `hook_future_timestamp_is_treated_as_fresh`
  - Use `observed_at = now + 5s` and assert the valid hook applies.
- `other_agent_ignores_hook_values`
  - Use `AgentClient::Other` with fresh `working`, `wait`, and `ask` entries.
  - Assert `applied == false` and the prior observation is unchanged.
- `malformed_hook_values_preserve_prior_observation`
  - Cover `""`, whitespace, `"unknown"`, and `"WAIT"`.
  - Assert none clear or replace a prior `Done` observation.

### Code to implement

- Add a value-typed hook-decision input/result in `ajax-core::live` carrying
  selected agent, prior observation, hook value, observed time, acknowledgment
  time, and injected `now`.
- Keep lifecycle and task mutation outside this pure decision.
- Preserve the existing `classify_agent_status_value` public function as a
  compatibility parser.

### Verification

- `rtk cargo nextest run -p ajax-core codex_working_hook`
- `rtk cargo nextest run -p ajax-core codex_wait_hook`
- `rtk cargo nextest run -p ajax-core claude_working_hook_uses_two_minute_window`
- `rtk cargo nextest run -p ajax-core malformed_hook_values_preserve_prior_observation`

## Task 2: Define Multi-Source Precedence and Deterministic Ties

### Failing behavior tests to write

Add these tests to `crates/ajax-core/src/live.rs`:

- `missing_substrate_outranks_wrapper_and_hook_activity`
  - Prior observation is `TmuxMissing`; candidates contain wrapper `working`,
    wrapper `done`, and hook `working`.
  - Assert `TmuxMissing` is preserved and no candidate is marked applied.
- `old_wrapper_completion_outranks_newer_hook_working`
  - Wrapper `done` is one hour old; hook `working` is one second old.
  - Assert `Done` is selected as trusted wrapper evidence.
- `old_wrapper_failure_outranks_newer_hook_waiting`
  - Assert old wrapper `failed` selects `CommandFailed` over fresh hook `wait`.
- `stale_wrapper_running_falls_through_to_fresh_hook`
  - Wrapper `working` is older than 30 seconds and hook `wait` is fresh.
  - Assert `WaitingForInput` is selected from the hook.
- `wrapper_running_is_fresh_through_thirty_seconds`
  - Assert wrapper `working` applies at exactly 30 seconds and falls through at
    30 seconds plus one nanosecond.
- `runtime_wrapper_wins_equal_timestamp_tie_with_hook`
  - Give wrapper `done` and hook `working` the same timestamp.
  - Assert wrapper `Done` wins regardless of input vector order.
- `busy_hook_wins_equal_timestamp_tie_with_waiting_hook`
  - Give hook `working`, `wait`, and `ask` the same timestamp in every relevant
    ordering.
  - Assert `AgentRunning` wins deterministically.
- `newest_malformed_entry_does_not_hide_older_valid_entry`
  - Newest hook is `garbage`; an older, still-fresh hook is `wait`.
  - Assert the valid waiting observation is selected.
- `all_ineligible_candidates_preserve_prior_credible_state`
  - Use stale heartbeats, stale hooks, and malformed values with prior `Done`,
    `WaitingForInput`, and `AgentRunning` cases.
  - Assert each prior credible observation is preserved.

### Code to implement

- Add a pure candidate selector in `ajax-core::live` with explicit source and
  state ranking.
- Filter malformed and ineligible candidates before selecting by timestamp.
- Return the selected source and whether stronger evidence was applied so
  `runtime_refresh` can choose ordinary, authoritative, or trusted application.

### Verification

- `rtk cargo nextest run -p ajax-core wrapper_`
- `rtk cargo nextest run -p ajax-core timestamp_tie`
- `rtk cargo nextest run -p ajax-core newest_malformed_entry_does_not_hide_older_valid_entry`

## Task 3: Make the Cache Adapter Evidence-Preserving

### Failing behavior tests to write

Add or revise tests in `crates/ajax-cli/src/agent_status_cache.rs`:

- `hook_snapshot_retains_entry_past_legacy_thirty_second_window`
  - Write a `working` status file with mtime `now - 119s`.
  - Assert the entry is returned with the exact value, timestamp, and
    `AgentStatusCacheSource::Hook`; core, not the adapter, will decide whether
    the selected agent can use it.
- `hook_snapshot_retains_stale_entry_for_core_fallback_decision`
  - Write a hook at `now - 121s`.
  - Assert it remains present rather than being discarded.
- `runtime_snapshot_keeps_old_terminal_exit_but_expires_old_running_heartbeat`
  - Create two runtime JSON files observed ten minutes ago: one
    `exited_success`, one `running`.
  - Assert terminal evidence remains eligible and the running heartbeat is
    marked stale/ineligible according to the retained entry contract.
- `merged_snapshot_preserves_source_and_timestamp_for_each_entry`
  - Merge one session hook and one task runtime snapshot with distinct fixed
    timestamps.
  - Assert both exact timestamps and source identities survive
    `status_entries_for_task` unchanged.
- `snapshot_does_not_choose_a_winner_between_hook_files`
  - Provide session and pane hook files with different values/timestamps.
  - Assert both entries are returned; winner selection belongs to core.

Revise the existing `newest_fresh_agent_status_wins_over_older_working_value`
test because that assertion encodes policy in the adapter. Keep the existing
single-snapshot and wrapper/task merge coverage.

### Code to implement

- Preserve one filesystem scan per refresh and the existing
  `AgentStatusCache` port.
- Keep raw hook entries long enough for core's two-minute windows.
- Do not add agent-specific policy or winner selection to the CLI adapter.
- Preserve terminal wrapper eligibility and the 30-second active-wrapper
  heartbeat behavior.

### Verification

- `rtk cargo nextest run -p ajax-cli 'agent_status_cache::tests'`

## Task 4: Add Agent-Aware Busy-First Pane Classification

### Failing behavior tests to write

Add these tests to `crates/ajax-core/src/live.rs`:

- `claude_busy_indicator_beats_stale_permission_prompt`
  - Pane contains `Run this command?`, `❯ Yes`, and a later
    `ctrl+c to interrupt` line.
  - Assert `AgentRunning`, not `WaitingForApproval`.
- `claude_spinner_beats_stale_idle_prompt`
  - Pane contains an old `❯` prompt and a recent `✢ Clauding… (53s · ↓ 749
    tokens)` line.
  - Assert `AgentRunning`.
- `claude_permission_dialog_is_waiting_for_approval`
  - Pane contains `Run this command?`, `❯ Yes`, `No`, and `Esc to cancel`
    without a busy indicator.
  - Assert `WaitingForApproval`.
- `claude_standalone_prompt_is_waiting_for_input`
  - Final meaningful line is `❯` with no busy indicator.
  - Assert `WaitingForInput`.
- `codex_working_status_beats_visible_composer_prompt`
  - Pane contains `› Fix the tests`, `Working (12s)`, and the Codex model/path
    footer.
  - Assert `AgentRunning`.
- `codex_idle_composer_is_waiting_for_input`
  - Reuse the existing Codex composer shape without a working indicator.
  - Assert `WaitingForInput`.
- `agent_specific_prompt_markers_do_not_cross_classify`
  - Classify a standalone Claude `❯` as Codex and a Codex `›` composer as
    Claude.
  - Assert neither becomes the other agent's waiting state.
- `ambiguous_redraw_returns_unknown_for_reducer_fallback`
  - Use box borders, status chrome, and unrelated prose with no valid marker.
  - Assert `Unknown`; prior-state preservation remains the reducer's job.

Keep existing tests for completion, CI failure, merge conflict, Cursor JSON,
shell prompts, negative done phrasing, and stale-history ordering.

### Code to implement

- Add `classify_agent_pane(agent, pane)` while preserving `classify_pane(pane)`
  as the generic compatibility wrapper.
- Apply recent busy indicators before prompts, then explicit failure/completion,
  then passive/unknown fallback.
- Reuse structured parsing helpers instead of adding a second independent
  prompt vocabulary where `agent_prompt` already owns structured choices.

### Verification

- `rtk cargo nextest run -p ajax-core claude_busy_indicator`
- `rtk cargo nextest run -p ajax-core claude_permission_dialog`
- `rtk cargo nextest run -p ajax-core codex_working_status`
- `rtk cargo nextest run -p ajax-core agent_specific_prompt_markers_do_not_cross_classify`

## Task 5: Pass Agent Identity Through Pane and Event Callers

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/slices/pane.rs`:

- `pane_snapshot_keeps_claude_busy_when_permission_text_remains_visible`
  - Build a Claude `PaneSnapshot` from the mixed pane in Task 4.
  - Assert `state.kind == AgentRunning`, `prompt == None`, and no approval
    choices are exposed from stale text.
- `pane_snapshot_exposes_live_claude_permission_prompt`
  - Use a prompt-only Claude pane.
  - Assert `WaitingForApproval`, parsed command/choices, confidence, and
    fingerprint remain populated by `agent_prompt`.
- `pane_snapshot_keeps_existing_codex_composer_contract`
  - Preserve the existing ANSI/unicode Codex composer assertions after routing
    through the agent-aware classifier.

Add tests to `crates/ajax-core/src/events.rs`:

- `claude_message_busy_indicator_beats_stale_prompt_text`
  - Apply an `AgentEvent::Message` to a Claude task.
  - Assert the task becomes `AgentRunning` and does not gain `NeedsInput`.
- `codex_message_uses_codex_specific_prompt_detection`
  - Apply a Codex composer message to a Codex task.
  - Assert `WaitingForInput`.
- `generic_live_observation_wrapper_preserves_existing_behavior`
  - Call the current public `live_observation_from_event` without task context.
  - Assert existing generic mappings for Started, Completed, Failed, and
    process events remain unchanged.

### Code to implement

- Route `slices::pane::classify_state` through `classify_agent_pane`.
- Add an agent-aware event mapping used by task/registry application while
  keeping `live_observation_from_event` as a compatibility wrapper.
- Do not move structured prompt parsing into `live.rs`.

### Verification

- `rtk cargo nextest run -p ajax-core 'slices::pane::tests'`
- `rtk cargo nextest run -p ajax-core 'events::tests'`

## Task 6: Add and Apply Attention Acknowledgment

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/live_application.rs`:

- `acknowledging_claude_waiting_clears_actionable_state_without_marking_dead`
  - Start with an Active Claude task whose live status is `WaitingForInput`,
    agent status is `Waiting`, and side flags contain `NeedsInput`.
  - Acknowledge at a fixed timestamp.
  - Assert live waiting is cleared, agent status is `NotStarted`, `NeedsInput`
    and `AgentDead` are absent, and lifecycle remains Active.
- `acknowledging_nonwaiting_state_does_not_erase_runtime_evidence`
  - Cover `AgentRunning`, `CommandFailed`, `Done`, and `TmuxMissing`.
  - Assert only the timestamp changes; status, flags, and lifecycle remain.
- `acknowledging_codex_waiting_records_time_but_keeps_attention`
  - Assert Codex waiting live status, `Waiting`, and `NeedsInput` remain.

Add tests to `crates/ajax-core/src/models.rs`:

- `new_task_has_no_attention_acknowledgment`
  - Assert `Task::new(...).attention_acknowledged_at == None`.
- `task_attention_acknowledgment_uses_latest_timestamp`
  - Acknowledge twice with fixed increasing times and assert the later value is
    retained.

Add tests to `crates/ajax-core/src/commands.rs` around `mark_task_opened`:

- `mark_task_opened_acknowledges_claude_attention_without_changing_lifecycle`
  - Use the existing `context_with_tasks` fixture converted to Claude waiting.
  - Call a clock-injected helper at a fixed time.
  - Assert the exact acknowledgment timestamp and unchanged lifecycle.
- `mark_task_opened_does_not_clear_codex_waiting`
  - Assert Codex remains actionable after the same command.
- Keep `mark_task_opened_preserves_existing_lifecycle` and
  `mark_task_opened_reports_missing_task` passing.

### Code to implement

- Add `pub attention_acknowledged_at: Option<SystemTime>` to `Task` with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- Add a core acknowledgment reducer in `live_application`; do not reproduce
  flag/status mutation in CLI, TUI, or Web code.
- Make public `mark_task_opened` call the reducer with `SystemTime::now()` and
  add a testable clock-injected internal helper.
- Refresh cached annotations after acknowledgment.

### Verification

- `rtk cargo nextest run -p ajax-core acknowledging_`
- `rtk cargo nextest run -p ajax-core attention_acknowledgment`
- `rtk cargo nextest run -p ajax-core mark_task_opened`

## Task 7: Prove Acknowledgment Reaches Operator Projections

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/ui_state.rs`:

- `acknowledged_claude_waiting_projects_idle`
  - Arrange via the acknowledgment reducer, not by manually deleting fields.
  - Assert `derive_operator_status` returns `OperatorStatusKind::Idle` with
    label `idle` while lifecycle stays Active.
- `new_claude_waiting_after_acknowledgment_projects_needs_input`
  - Apply waiting evidence newer than the acknowledgment.
  - Assert `NeedsInput` and `waiting for input`.
- `acknowledgment_does_not_hide_failure_or_missing_substrate`
  - Assert `CommandFailed` remains Failed and `TmuxMissing` remains Failed.

Add tests to `crates/ajax-core/src/attention.rs`:

- `acknowledged_claude_waiting_has_no_needs_me_annotation`
  - Assert `annotate(task)` contains no `AnnotationKind::NeedsMe` after the
    reducer clears waiting state and stale flags.
- `new_waiting_after_acknowledgment_restores_needs_me_annotation`
  - Assert newer waiting produces one collapsed `NeedsMe` annotation.
- `acknowledgment_does_not_remove_broken_or_reviewable_annotations`
  - Cover merge conflict and Reviewable lifecycle.

Add tests to `crates/ajax-core/src/commands.rs`:

- `cockpit_inbox_excludes_acknowledged_claude_waiting_task`
  - Build the cockpit projection after acknowledgment.
  - Assert the task stays visible in its repo but is absent from the inbox and
    `needs_attention == false`.
- `cockpit_inbox_reincludes_task_after_new_waiting_evidence`
  - Apply newer waiting and assert the task returns to the inbox exactly once.

### Code to implement

- Prefer making the acknowledgment reducer produce internally consistent task
  state so `ui_state` and `attention` do not need timestamp-specific duplicate
  policy.
- Change projection code only if the failing tests expose a remaining stale
  annotation or side-flag path.

### Verification

- `rtk cargo nextest run -p ajax-core acknowledged_claude_waiting`
- `rtk cargo nextest run -p ajax-core new_waiting_after_acknowledgment`
- `rtk cargo nextest run -p ajax-core cockpit_inbox_excludes_acknowledged`

## Task 8: Persist Acknowledgment and Define Save Conflicts

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/registry/sqlite.rs`:

- `sqlite_registry_round_trips_attention_acknowledged_at`
  - Save a task with a fixed timestamp containing non-zero nanoseconds.
  - Reload and assert exact `SystemTime` equality.
- `sqlite_registry_migrates_v5_with_null_attention_acknowledgment`
  - Build a typed v5 database using the existing migration fixtures.
  - Open through `SqliteRegistryStore`, assert schema version 6, both new
    columns exist, and the loaded task field is `None`.
- `sqlite_registry_migration_preserves_existing_v5_task_state`
  - Assert lifecycle, live status, side flags, runtime projection, events, and
    step receipts remain unchanged across the migration.
- `sqlite_registry_rejects_half_present_acknowledgment_timestamp`
  - Insert seconds without nanoseconds (and the reverse if the schema permits).
  - Assert an explicit snapshot error instead of silently normalizing corrupt
    data.
- Extend `sqlite_registry_store_uses_typed_columns_not_json_payloads`
  - Assert the two acknowledgment columns are typed columns.

Add tests to `crates/ajax-core/src/output.rs`:

- `task_json_without_acknowledgment_remains_backward_compatible`
  - Serialize an existing task/output fixture with `None` and assert the new
    optional field is omitted.
- `task_json_deserializes_without_acknowledgment_field`
  - Deserialize a pre-change Task fixture and assert the field defaults to
    `None`.

Add a test to `crates/ajax-cli/src/context.rs`:

- `save_context_reports_conflict_for_concurrent_ack_and_live_status_change`
  - Load the same baseline into two tracked contexts.
  - Persist a live-status change from one and an acknowledgment change from the
    other.
  - Assert the second save returns the existing explicit same-task facts
    conflict and reloading preserves the first writer; no last-writer-wins
    overwrite is allowed.

### Code to implement

- Raise `SQLITE_SCHEMA_VERSION` from 5 to 6.
- Add nullable seconds/nanoseconds columns and `migrate_v5_to_v6`.
- Read and write the optional timestamp with strict pair validation.
- Keep the existing optimistic revision contract; do not add a broad
  field-by-field merge in this refactor.

### Verification

- `rtk cargo nextest run -p ajax-core attention_acknowledged_at`
- `rtk cargo nextest run -p ajax-core task_json_`
- `rtk cargo nextest run -p ajax-cli save_context_reports_conflict_for_concurrent_ack_and_live_status_change`

## Task 9: Integrate Decisions into Runtime Refresh

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/runtime_refresh.rs` using the existing
`context_with_active_task`, `HealthyRefreshRunner`, `GitSkippingRunner`, and
command recording patterns:

- `missing_session_skips_wrapper_hook_and_pane_status_application`
  - Supply wrapper `done`, hook `working`, and pane output indicating work.
  - Assert `TmuxMissing`, `AgentRuntimeStatus::Dead`, and no capture-pane call.
- `old_wrapper_completion_beats_new_hook_during_refresh`
  - Extend the existing wrapper completion test so terminal evidence is old and
    hook activity is newer/fresh.
  - Assert `Done` and Reviewable lifecycle.
- `stale_codex_working_falls_through_to_agent_aware_pane_capture`
  - Codex hook is 20 seconds plus one nanosecond old; pane shows active Codex.
  - Assert pane capture occurs and final status is `AgentRunning`.
- `fresh_codex_wait_skips_pane_capture`
  - Codex `wait` is 119 seconds old.
  - Assert `WaitingForInput`, `NeedsInput`, and zero capture-pane commands.
- `stale_codex_wait_uses_pane_fallback`
  - Codex `wait` is older than 120 seconds; pane shows active Codex.
  - Assert `AgentRunning` and one pane capture.
- `unsupported_agent_hook_is_ignored_and_pane_is_captured`
  - Selected agent is `Other`, hook is fresh `working`.
  - Assert hook does not skip pane capture.
- `malformed_newest_hook_does_not_hide_older_valid_wait`
  - Provide newest `garbage` plus older fresh `wait`.
  - Assert waiting is applied and pane is skipped.
- `pane_capture_failure_preserves_prior_credible_live_status`
  - Extend the existing probe-failure test with prior `Done` and prior waiting
    cases.
  - Assert neither becomes `CommandFailed` or Unknown; only the runtime probe
    error changes.
- `acknowledged_old_claude_wait_stays_idle_without_pane_capture`
  - Acknowledgment is newer than the hook.
  - Assert the task remains non-actionable and the old hook is not reapplied.
- `new_claude_wait_after_acknowledgment_restores_attention`
  - Hook timestamp is one nanosecond newer than acknowledgment.
  - Assert waiting status, `NeedsInput`, and refreshed annotations.
- `codex_wait_ignores_acknowledgment_while_fresh`
  - Assert fresh Codex wait remains actionable after opening.
- `status_decision_preserves_steady_state_command_budget`
  - Re-run the existing `RefreshTier::Live` budget assertions: zero git
    worktree lists, zero pane captures for stable strong evidence, and no more
    than list-sessions plus list-windows.

### Code to implement

- Replace `latest_fresh_status` with the pure core candidate selector.
- Pass `task_snapshot.selected_agent`, prior status, acknowledgment timestamp,
  and one captured `now` into the decision.
- Apply Hook through `apply_authoritative_observation`, RuntimeWrapper through
  `apply_trusted_observation`, and pane fallback through ordinary observation.
- Continue probing pane only when stronger evidence is absent or stale.
- Do not turn stale status cache entries into a probe failure merely because
  they are stale; a successful pane fallback is a successful observation.

### Verification

- `rtk cargo nextest run -p ajax-core stale_codex_`
- `rtk cargo nextest run -p ajax-core acknowledged_old_claude_wait`
- `rtk cargo nextest run -p ajax-core old_wrapper_completion`
- `rtk cargo nextest run -p ajax-core status_decision_preserves_steady_state_command_budget`

## Task 10: Verify Resume Operations Acknowledge Only After Success

### Failing behavior tests to write

Add tests to `crates/ajax-core/src/task_operations.rs` near
`resume_and_review_task_operations_execute_in_core_with_reducers`:

- `successful_resume_records_attention_acknowledgment`
  - Arrange an Active Claude waiting task and a successful resume command.
  - Assert acknowledgment is `Some`, lifecycle remains Active, and waiting
    attention is cleared.
- `failed_resume_does_not_acknowledge_attention`
  - Make the external resume command fail.
  - Assert acknowledgment remains `None` and waiting attention remains visible.
- `review_operation_does_not_acknowledge_attention`
  - Execute Review against the same task.
  - Assert the timestamp is unchanged because Review does not enter the task.

Keep the current CLI/Cockpit call sites delegating to core reducers. Do not add
browser acknowledgment: Web Cockpit intentionally rejects terminal resume.

### Code to implement

- Preserve the current operation boundary: Resume calls `mark_task_opened` only
  after successful external execution.
- Adjust call ordering only if the failing tests prove an existing path records
  acknowledgment before a failed entry.

### Verification

- `rtk cargo nextest run -p ajax-core successful_resume_records_attention_acknowledgment`
- `rtk cargo nextest run -p ajax-core failed_resume_does_not_acknowledge_attention`
- `rtk cargo nextest run -p ajax-core review_operation_does_not_acknowledge_attention`

## Task 11: Update Architecture Documentation and Run Full Validation

### Documentation to update

Update `architecture.md` to state:

- Hook freshness is selected-agent and state specific in core.
- Runtime-wrapper terminal evidence outranks hooks and pane text, but explicit
  missing substrate remains authoritative.
- Pane fallback is agent-aware and busy-first.
- Opening a task persists an attention acknowledgment without changing
  lifecycle; Claude waiting at/before that timestamp is acknowledged, while
  Codex waiting remains actionable until stale and pane fallback runs.
- Agent-deck inspired the behavior, but Ajax retains its own lifecycle,
  substrate, task-operation, and operator-projection boundaries.

### Verification

- Read the edited Live Status, Runtime Refresh, Registry, and Cockpit sections
  for consistency with the implementation.
- `rtk cargo fmt --check`
- `rtk cargo check --all-targets --all-features`
- `rtk cargo clippy --all-targets --all-features -- -D warnings`
- `rtk cargo nextest run --all-features`
- `rtk cargo doc --no-deps --all-features`
- `RUSTDOCFLAGS="-D warnings" rtk cargo doc --no-deps --all-features`

## Expected Files

- `.planning/agent-deck-status-refactor-plan.md`
- `architecture.md`
- `crates/ajax-core/src/live.rs`
- `crates/ajax-core/src/live_application.rs`
- `crates/ajax-core/src/models.rs`
- `crates/ajax-core/src/ui_state.rs`
- `crates/ajax-core/src/attention.rs`
- `crates/ajax-core/src/commands/open.rs`
- `crates/ajax-core/src/commands.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `crates/ajax-core/src/registry/sqlite.rs`
- `crates/ajax-core/src/slices/pane.rs`
- `crates/ajax-core/src/events.rs`
- `crates/ajax-core/src/output.rs`
- `crates/ajax-core/src/task_operations.rs`
- `crates/ajax-cli/src/agent_status_cache.rs`
- `crates/ajax-cli/src/context.rs`

The caller list may shrink if compatibility wrappers avoid production edits.
It must not expand into unrelated modules without revising and re-approving the
plan. Every code task begins with the named failing tests, runs the focused
failure, implements the minimum behavior, and reruns the focused command to
green before the next task starts.
