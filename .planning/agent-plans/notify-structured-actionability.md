# Notify: structured actionability (follow-on to native-hook status PR)

## Scope

Remove the last status-string arbitration in the notification path. Today
`attention::is_actionable_attention` decides whether to phone-ping by
string-matching the operator **explanation** (`"Waiting for input" |
"Waiting for approval"`). That duplicates knowledge the projector already has,
and silently breaks if a projector explanation string ever changes — the exact
anti-pattern the native-hook PR removed elsewhere.

Make actionability a **structured field** the projector sets from the evidence;
the notifier reads the field, never a string.

Behavior-preserving: the same tasks ping as before.

Delegation decision: not delegated — tightly coupled to the `derive_operator_status`
projector reworked in this PR; parent holds full context and is the reviewer.

## Non-goals

- No change to which tasks notify (pure refactor).
- No change to the dwell/episode/dedup machinery or `notify.rs` delivery.
- No new field on `TaskCard`/web output — `actionable` stays core-internal.

## Changes

1. `ui_state::OperatorStatus` gains `pub actionable: bool`.
2. `derive_task_status` returns actionability per branch via small self-documenting
   constructors: `err` (Error, actionable), `run` (Running), `ping` (actionable
   Waiting), `soft` (non-actionable Waiting), `idle`, `unknown`.
   - `canonical_waiting_explanation` returns `Option<(&str, bool)>` so the
     actionable split (WaitingForApproval/Input) vs soft (Auth/Rate/Context/
     Done-response-ready) lives in one table.
3. `attention::is_actionable_attention(status)` → `status.actionable`. Delete the
   hardcoded string match. All `Error` is actionable; `Waiting` per the field;
   `Running`/`Idle`/`Unknown` are not.

## Tests

Existing notify tests (`attention` module) are the regression suite and must
pass unchanged — they assert the real ping/no-ping outcomes (delegated, rate
limited, response-ready, auth, ready-for-review → no ping; input/approval/error
→ ping). Add one test asserting `derive_operator_status(...).actionable` matches
expectation for a representative actionable (WaitingForApproval) and soft
(RateLimited / Ready-for-review) case, so the field itself is covered.

## Validation

`cargo fmt --check` · `cargo clippy --all-targets --all-features -D warnings` ·
`cargo nextest run --all-features`.
