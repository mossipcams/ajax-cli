PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Fix review HIGHs and docs honesty for canonical agent events: stop anon
open-set leaks, stop pane wait re-apply churn, bound socket reads, and align
docs with actual socket/Codex behavior.

## Allowed files

- `crates/ajax-core/src/canonical_agent_event.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `crates/ajax-cli/src/agent_event.rs`
- `crates/ajax-cli/src/agent_event_notify.rs`
- `architecture.md`
- `.planning/agent-plans/canonical-agent-events.md`
- `.planning/agent-plans/canonical-events-review-fixes.md`

## Forbidden changes

- No commits, pushes, merges, rebases, or branch switches
- No new dependencies
- No schema_version bump or new canonical kinds
- No wiring full cockpit wake/fan-out beyond bounded read + doc honesty
- No broad pane classification restore
- No edits outside Allowed files

## Context evidence

Desired behavior (from REVISE):

1. `ActivityStarted` without `activity_id` must not leave forever-open tools that
   keep fold at `working` after `TurnSettled` + legacy `done`.
2. Claude `Stop` with non-empty `background_tasks` should keep the run active
   via `TurnStarted` (no open-set entry), not `ActivityStarted` with `None` id.
3. Pane fallback re-applying the same wait kind every refresh must not dirty
   the registry: skip when `live_status.kind == observation.kind`.
4. `notify.sock` listener must bound line reads; docs must not claim immediate
   status delivery (JSONL on refresh is the durable path).
5. Plan matrix: Codex `SessionClosed` is `SessionEnd` (native) with wrapper
   exit as backup; non-goal “no socket in same round” is stale.

Anchors:

- Fold anon insert: `canonical_agent_event.rs` ~171–207
- Pane skip: `runtime_refresh.rs` ~460–474 (`pane_now = SystemTime::now()`)
- Claude stop: `agent_event.rs` `claude_stop` ~248–264
- activity_id: `activity_id_from_payload` ~279–284
- Listener: `agent_event_notify.rs` ~34–38
- Tests: `claude_stop_with_background_tasks_does_not_settle`;
  `fold_turn_settled_with_open_tool_stays_working`

## Code anchors

```rust
// canonical_agent_event.rs — ActivityStarted currently:
let id = activity_id.clone().unwrap_or_else(|| format!("anon-{index}"));
state.active_tools.insert(id, ());

// runtime_refresh.rs — broken unchanged check:
.is_some_and(|status| status.kind == observation.kind)
&& task.live_status_observed_at.is_some_and(|current| current >= pane_now);

// agent_event.rs claude_stop background branch:
kind: CanonicalEventKind::ActivityStarted,
detail: Some(CanonicalEventDetail::Activity {
    activity: ActivityKind::Tool,
    activity_id: None,
}),
```

## Test-first instructions

Add/adjust tests **before** production edits. Confirm RED.

1. In `canonical_agent_event.rs` tests:
   - `fold_activity_started_without_id_then_settled_is_done`: envelopes
     `ActivityStarted` Tool with `activity_id: None`, then `TurnSettled`
     Completed → `project_snapshot == Some("done")` and `active_tools` empty.
   - Optional: `fold_activity_finished_without_id_clears_legacy_anon` if any
     `anon-*` still exist in fixtures.

2. In `agent_event.rs` tests:
   - Update `claude_stop_with_background_tasks_does_not_settle` to expect
     `CanonicalEventKind::TurnStarted` (still projects `Some("working")`).
   - Keep `translate_claude_stop_with_background_tasks_stays_working`.

3. In `runtime_refresh.rs` tests (or nearest existing pane/refresh test module):
   - Prove that when pane wait observation kind matches existing
     `live_status.kind`, refresh does **not** mark registry dirty / does not
     bump `live_status_observed_at` solely from re-apply. Prefer the smallest
     existing harness; if no harness fits, add a focused unit around the
     unchanged-kind condition by extracting nothing — assert via existing
     refresh test patterns in that file.

RED command:

```bash
cargo test -p ajax-core canonical_agent_event -- --nocapture
cargo test -p ajax-cli agent_event -- --nocapture
```

## Edit instructions

1. **Fold** (`canonical_agent_event.rs`):
   - On `ActivityStarted` Tool: insert into `active_tools` **only** when
     `activity_id` is `Some`. Still set `activity = Tool` and `phase = Active`.
   - On `ActivityFinished` Tool with `activity_id: None`: remove any keys in
     `active_tools` that start with `anon-` (defensive cleanup of old logs).
   - Keep id-bearing finish remove-by-id behavior.

2. **Claude Stop** (`agent_event.rs`):
   - Non-empty `background_tasks` → `TurnStarted` (no Activity detail).
   - Prefer unique ids in `activity_id_from_payload`: try
     `tool_call_id`, `tool_id`, `id`, then `tool_name`, `tool`.

3. **Pane** (`runtime_refresh.rs` pane fallback block only ~465–474):
   - `live_status_unchanged` = kind equal only (remove `observed_at >= pane_now`).
   - Do not change the earlier lifecycle `live_status_unchanged` block that
     compares against `observed_at` from the decision (that path is fine).

4. **Socket** (`agent_event_notify.rs`):
   - Cap line read (e.g. `reader.take(64 * 1024)` then `read_line`, or
     equivalent). Oversized/erratic input → skip, do not panic.

5. **Docs**:
   - `architecture.md` status section: state that `notify.sock` is best-effort;
     listener currently accepts/drains lines; durable status comes from JSONL
     fold on runtime refresh (not immediate fan-out).
   - Plan: Codex SessionClosed cell → `SessionEnd` / wrapper exit; update
     non-goal that forbade same-round socket (note completed in Phase 3).
   - Check off tasks in `canonical-events-review-fixes.md` as done.

## Verification commands

```bash
cargo test -p ajax-core canonical_agent_event -- --nocapture
cargo test -p ajax-core pane_fallback -- --nocapture
cargo test -p ajax-cli agent_event -- --nocapture
cargo test -p ajax-cli agent_event_notify -- --nocapture
cargo test -p ajax-core runtime_refresh -- --nocapture
cargo fmt --check
cargo clippy -p ajax-core -p ajax-cli --all-targets -- -D warnings
```

## Acceptance criteria

- Background Claude Stop never leaves an open anon tool after later settle.
- Same pane wait kind on consecutive refreshes does not force registry dirty.
- Socket read is bounded; architecture does not claim immediate delivery.
- All verification commands exit 0.

## Stop conditions

- Need to change reducer public API or schema_version → STOP
- Need full web wake channel into `WebAppState` → STOP (docs-only for wake)
- Untouched crates start failing for unrelated reasons → report, do not “fix”

## DELEGATE_REPORT schema

Return exactly:

```yaml
DELEGATE_REPORT:
  STATUS: COMPLETE | BLOCKED | FAILED
  SUMMARY: <one sentence>
  FILES_CHANGED: [<paths>]
  TEST_FIRST: PROVEN | NOT_APPLICABLE | NOT_PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED | GREEN | VERIFY | OTHER
      COMMAND: <exact command>
      EXIT_CODE: <integer>
      OUTPUT_EXCERPT: <lines proving the result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
```
