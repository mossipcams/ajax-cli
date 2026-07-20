# Suppress notify during drop

## Scope

When a task is dropped, teardown makes tmux/worktree vanish. That missing
substrate projects as `Error` *before* the `Removing → Idle` branch in
`derive_operator_status`, so `take_attention_transition` phone-pings immediately
even though the task is about to disappear or settle.

Fix: do not fire attention webhooks while lifecycle is `Removing` or `Removed`.
`TeardownIncomplete` remains notifiable (durable post-drop failure).

Non-goals: change operator status derivation / UI Error-during-Removing
behavior; change dwell/episode-clear constants; TeardownIncomplete delay;
tidy/Merged/Cleanable notify policy.

## Delegation decision

`Delegation decision: delegated via model-router` → `pi-delegate` / GLM
(backend attention path). Report schema invalid (wrong YAML shape); parent
accepted after independent review + validation of the in-scope diff.

## Tasks

- [x] Test: `Removing` + missing substrate → `take_attention_transition` = `None`
- [x] Test: `TeardownIncomplete` still fires once
- [x] Implement: early return in `take_attention_transition_at` for
      `Removing` / `Removed`
- [x] Validation: focused attention nextest + fmt/clippy as in packet

## Deviations

- Delegate `DELEGATE_REPORT` failed schema validation (wrapped/nonstandard YAML).
  Parent Review Gate: ACCEPT after reading delta + re-running verification.
- Parent lightly extended the module doc comment to mention the drop guard
  (still only `attention.rs`).

## Validation

```bash
cargo nextest run -p ajax-core removing_with_missing_substrate_does_not_notify teardown_incomplete_still_notifies attention
# 56 passed (parent re-run)

cargo fmt --check   # exit 0
cargo clippy -p ajax-core --all-targets --all-features -- -D warnings  # exit 0
```
