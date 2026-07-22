PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Turn-settled `Done` evidence projects Waiting `"Response ready"` (Cursor `stop`,
Claude/Codex/Pi settle). Keep that UI status, but do **not** phone-ping or stamp
a notify episode — mirror `rate_limited_waiting_does_not_notify` /
`"Ready for review"`.

## Allowed files

- `crates/ajax-core/src/attention.rs`

## Forbidden changes

- Do not edit agent_event.rs, agent_hooks.rs, ui_state.rs, live_application.rs,
  architecture.md, or notify.rs.
- Do not suppress Error-class notify (including `failed` / CommandFailed).
- Do not change episode stamp / quiet-clear timing.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- Desired: Cursor `stop` → legacy `done` → Waiting `"Response ready"` must not
  webhook. Architecture already says actionable Waiting is wait/ask from
  structured hooks; Cursor has no native wait/ask.
- FP path: `agent_event.rs` `cursor_stop` → TurnSettled → `done`;
  `ui_state.rs:167` `Done => "Response ready"`; `ui_state.rs:122-123`
  agent Done → `"Response ready"`; `attention.rs:115-125`
  `is_actionable_attention` filters `"Ready for review"` and `"Rate limited"`
  but **not** `"Response ready"`.
- Pattern to copy: `rate_limited_waiting_does_not_notify` at
  `attention.rs:996-1019`.

## Code anchors

1. `is_actionable_attention` Waiting arm (`attention.rs:115-125`): add
   `explanation != "Response ready"`.
2. New test next to `rate_limited_waiting_does_not_notify`: apply
   `LiveStatusKind::Done` / `"done"`, assert status Waiting + explanation
   `"Response ready"`, `take_attention_transition` = None, metadata empty.

## Test-first instructions

1. Add `response_ready_waiting_does_not_notify` that fails because Done currently
   fires Waiting notify. Red:
   `cargo nextest run -p ajax-core attention -- response_ready_waiting_does_not_notify`
2. Implement the one-line filter. Green: same command.

## Edit instructions

1. In `is_actionable_attention`, Waiting branch, add
   `&& explanation != "Response ready"` alongside the existing Ready for review
   / Rate limited checks.
2. Add the test cloning `rate_limited_waiting_does_not_notify` but using
   `LiveObservation::new(LiveStatusKind::Done, "done")` and expecting
   explanation `"Response ready"`.

## Verification commands

```bash
cargo nextest run -p ajax-core attention -- response_ready rate_limited Ready
cargo fmt --check
```

## Acceptance criteria

- Done / Response ready Waiting never returns Some from take_attention_transition
  and does not write notify metadata.
- Waiting for input / approval and Error still notify (existing suite stays green).
- No files outside Allowed files changed.

## Stop conditions

- Need to change ui_state, agent_event, or lifecycle semantics.
- Need to suppress CommandFailed / failed notify.
