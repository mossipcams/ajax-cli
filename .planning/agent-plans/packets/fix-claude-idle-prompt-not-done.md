PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Stop Claude `Notification:idle_prompt` from projecting Done / FullyCompleted.

Today `idle_prompt` and `agent_completed` share one arm that emits
`turn_settled(Completed)` (`agent_event.rs` ~131-132). Idle-prompt chrome is a
parked input/question surface (`pane_fallback` WaitingQuestion); treating it as
Completed fights pane reconcile and shows false “Response ready”.

Map `Notification:idle_prompt` to `attention_requested(Question)` (same as
`agent_needs_input` / elicitation_dialog). Keep `agent_completed` as
`turn_settled(Completed)`.

## Allowed files

- `crates/ajax-cli/src/agent_event.rs`

## Forbidden changes

- PaneEvidence reducer rewrite / runtime_refresh reconcile changes
- Hook installer / ACP paths
- Commits, pushes, branch changes

## Acceptance

1. `translate_native_event("claude", "Notification:idle_prompt", …)` yields
   `AttentionRequested` + `AttentionReason::Question`.
2. `Notification:agent_completed` still yields `TurnSettled` + `Completed`.
3. Existing combined test that asserts both are TurnSettled is split/updated.
4. Focused `cargo test -p ajax-cli` covering translate_native_event / idle_prompt passes.

## Constraints

- One match-arm split; reuse `attention_requested(AttentionReason::Question)`.
- Estimated scope ≤ ~40 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-cli idle_prompt -- --nocapture
      expected: idle_prompt attention test passes; agent_completed still TurnSettled
    - type: test
      command: cargo test -p ajax-cli --lib agent_event -- --nocapture
      expected: agent_event unit tests pass
  reason: Proves idle_prompt no longer settles the turn as Completed.
```

## Stop if

- Mapping idle_prompt to Question conflicts with a documented Claude contract that requires Completed
- Patch would exceed ~400 changed lines
