# Packet: canonical event envelope + dual-write (Phase 1a)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Introduce schema_version-1 canonical agent-event kinds and envelopes in
`agent_event.rs`. Adapters translate native `(client, event, payload)` into a
kind (facts only). Persist each accepted event as one JSONL line under the
events dir, and **dual-write** the existing `AgentEventSnapshot` `{stem}.json`
via a pure `project_legacy_value(kind) → working|wait|ask|done|failed` so
`agent_status_cache` keeps working unchanged. No open-set reducer yet; no
installer changes; no ajax-core edits.

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`

## Forbidden changes

- Do not edit `agent_status_cache.rs`, `agent_hooks.rs`, `live.rs`,
  `agent_status.rs`, `cli.rs`, `lib.rs`, or any other crate/file.
- Do not remove or rename `AgentEventSnapshot` fields (readers depend on them).
- Do not add dependencies.
- Do not implement Unix socket delivery.
- Do not implement open-set RunSnapshot fold.
- No commits/pushes/branch changes.

## Context evidence

- Desired behavior: plan
  `.planning/agent-plans/canonical-agent-events.md` Phase 1a; architecture.md
  agent-status section (canonical facts before reducer).
- Current translate collapses to status strings at
  `crates/ajax-cli/src/agent_event.rs:60-98` (`translate_agent_event`).
- Current write is atomic latest-only at
  `write_agent_event` `agent_event.rs:100-126` → `{stem}.json`.
- Cache reads `AgentEventSnapshot` only from `*.json` in
  `agent_status_cache.rs` `read_agent_event_entry` (~212).
- Existing tests in `agent_event.rs` assert legacy string values; keep those
  assertions green via projection, and add envelope/JSONL tests.

## Code anchors

1. Replace `translate_agent_event(...) -> Option<&'static str>` with:
   - `translate_native_event(client, event, payload) -> Option<CanonicalAgentEvent>`
     where `CanonicalAgentEvent` carries `kind` (+ small detail enums).
   - `project_legacy_value(kind) -> &'static str` mapping:
     - TurnStarted / ActivityStarted → `"working"`
     - AttentionRequested { Permission } → `"ask"`
     - AttentionRequested { other } → `"wait"`
     - AttentionCleared → `"working"` (cleared wait resumes activity signal for
       legacy snapshot only)
     - TurnSettled { Completed | Interrupted | Unknown } → `"done"`
     - TurnSettled { Failed } → `"failed"`
     - SessionOpened → `"working"`
     - SessionClosed → `"done"`
     - ChildStarted → `"working"`
     - ChildSettled → `"done"` if no better signal (legacy last-write only)
     - Heartbeat → return None from translate path (do not write)
     - ActivityFinished → `"working"` if we cannot know idle yet (legacy
       last-write; open-set is Phase 2) — OR skip write when only ActivityFinished;
       prefer **skip write** (return None from the public write path) for
       ActivityFinished and Heartbeat so PostToolUse does not stomp ask/wait.
       **Clarification for this packet:** ActivityFinished must still emit a
       JSONL envelope but **must not** update `{stem}.json` legacy snapshot.
       ActivityStarted / TurnStarted / Attention* / TurnSettled / Session* /
       Child* update both JSONL and legacy snapshot.

2. Native mapping (keep current coverage, structured as kinds):
   - claude UserPromptSubmit → TurnStarted
   - claude PreToolUse → ActivityStarted { Tool, activity_id from payload tool
     name/id if present }
   - claude PostToolUse → ActivityFinished { Tool, … }
   - claude Notification permission → AttentionRequested { Permission }
   - claude Notification else → AttentionRequested { Question }
   - claude Stop + non-empty background_tasks → ActivityStarted { Tool } (or
     TurnStarted) — **must project legacy `"working"`**, must NOT TurnSettled
   - claude Stop else → TurnSettled { Completed }
   - codex UserPromptSubmit / PreToolUse / PostToolUse — same as claude working
     pattern (TurnStarted / ActivityStarted / ActivityFinished)
   - codex PermissionRequest → AttentionRequested { Permission }
   - codex Stop → TurnSettled { Completed }
   - cursor beforeSubmitPrompt → TurnStarted
   - cursor preToolUse / postToolUse → ActivityStarted / ActivityFinished Tool
   - cursor stop → TurnSettled { Failed } if payload.status == "error", else
     { Interrupted } if "aborted", else { Completed }
   - pi before_agent_start → TurnStarted
   - pi agent_settled → TurnSettled { Completed }
   - unknown → None

3. Envelope written to JSONL (`{stem}.jsonl`, append one line per event):
   ```json
   {
     "schema_version": 1,
     "event_id": "<unique>",
     "task_id": "...",
     "run_id": "...",
     "parent_run_id": null,
     "client": "claude",
     "native_event": "Stop",
     "kind": "turn_settled",
     "detail": { "outcome": "completed" },
     "occurred_at_unix_millis": 0,
     "received_at_unix_millis": 0,
     "source": "native_hook"
   }
   ```
   Use snake_case serde for kind/detail. `event_id` = 
   `{received_at_unix_millis}-{pid}-{monotonic}` string is fine (no new deps).
   Optional fields (`client_version`, session/turn ids) may be omitted.

4. `run_agent_event`: translate → if None return Ok; else append JSONL; if kind
   should update legacy snapshot, call existing atomic write of
   `AgentEventSnapshot` with `project_legacy_value`.

5. Keep `translate_agent_event` as a thin wrapper
   `translate_native_event(...).and_then(|e| project_legacy_for_snapshot(e))`
   **or** update tests to call the new APIs — either is fine if all existing
   tests stay meaningful and green.

## Test-first instructions

Red command: `cargo test -p ajax-cli agent_event -- --nocapture`

Add tests (fail before implementation):

1. `cursor_stop_error_projects_failed` — payload `{"status":"error"}` → legacy
   `"failed"` (and/or kind TurnSettled Failed).
2. `claude_stop_with_background_tasks_does_not_settle` — still legacy
   `"working"` (existing test may already cover; keep it).
3. `write_appends_jsonl_and_updates_legacy_snapshot` — after TurnStarted-like
   event, `{stem}.jsonl` has one line with `schema_version: 1` and
   `kind: turn_started` (or matching), and `{stem}.json` still has
   `value: "working"`.
4. `activity_finished_appends_jsonl_without_clobbering_ask_snapshot` — write
   AttentionRequested Permission first (legacy `ask`), then ActivityFinished;
   JSONL has both lines; `{stem}.json` value remains `"ask"`.

Then implement until green.

## Edit instructions

Only `agent_event.rs`. Introduce compact enums/structs with serde; keep module
private (`pub(crate)`). Prefer matching existing style (no new traits). Wire
`run_agent_event` through the new path. Preserve noop without identity.

## Verification commands

```bash
cargo test -p ajax-cli agent_event
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt -p ajax-cli -- --check
```

## Acceptance criteria

- All `agent_event` tests green including the four above.
- Legacy `{stem}.json` shape unchanged for status-updating events.
- JSONL append-only; ActivityFinished does not overwrite ask/wait snapshot.
- No other files changed.

## Stop conditions

- Need to edit `agent_status_cache` or hooks installers.
- Patch would exceed ~400 changed lines.
- Would require a new crate dependency.
- Anchor mismatch on existing snapshot field names.
