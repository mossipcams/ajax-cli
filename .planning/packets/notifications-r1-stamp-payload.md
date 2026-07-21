PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Coarsen attention episode dedup to operator status class only (`Waiting` /
`Error`), add `client` to `AttentionTransition`, and include the agent client
in the webhook body. Same-class explanation churn must not re-fire; class
changes (Waiting→Error) still fire.

## Allowed files

- `crates/ajax-core/src/attention.rs`
- `crates/ajax-cli/src/notify.rs`

## Forbidden changes

- Do not edit architecture.md, README, agent_hooks, agent_event, live_*, or
  runtime_refresh.
- Do not change when Waiting/Error are considered actionable
  (`is_actionable_attention` filters stay).
- Do not fire webhooks from `__agent-event`.
- Do not commit, push, merge, rebase, or change branches.
- No drive-by cleanup outside the listed files.

## Context evidence

- Desired: plan Round 1 — stamp = `status.as_str()` only; body
  `{repo}/{handle}: {status} ({client})` optional ` — {explanation}`;
  client from `task.selected_agent` lowercased (`Claude`→`claude`).
- Anchor: `episode_stamp` at `attention.rs:109-115` currently
  `format!("{}|{}", status, explanation)`.
- Anchor: `AttentionTransition` at `attention.rs:18-24` lacks `client`.
- Anchor: construction at `attention.rs:77-82` and test expect at
  `attention.rs:700-705`.
- Anchor: stamp asserts `Error|CI failed` (`attention.rs:866`) and
  `Waiting|Waiting for input` (`attention.rs:899`) must become class-only.
- Anchor: `distinct_error_reason_fires_again` (`attention.rs:829`) must stop
  re-firing within Error class under the new stamp.
- Anchor: `distinct_attention_reasons_refire_immediately_without_quiet_window`
  (`attention.rs:1032`) must be rewritten: Waiting→Waiting(other explanation)
  does not re-fire; Waiting→Error still fires once.
- Anchor: `webhook_command` `notify.rs:7-22` builds body without client;
  tests at `notify.rs:75-99` and `103-114`.

## Code anchors

- `fn episode_stamp` → return `status.status.as_str().to_string()` only.
- `AttentionTransition { repo, handle, status, explanation, client }`.
- `take_attention_transition_at` sets
  `client: task.selected_agent` as lowercase label (`claude`/`codex`/`other`
  via match or `format!("{:?}", …).to_ascii_lowercase()`).
- `webhook_command` inserts ` ({client})` after status.
- Update every `AttentionTransition { … }` literal in allowed files.

## Test-first instructions

1. Rewrite `distinct_attention_reasons_refire_immediately_without_quiet_window`
   into `waiting_explanation_churn_does_not_refire_within_episode`:
   - apply WaitingForInput → fire once
   - apply WaitingForApproval (still Waiting class) → `take_attention_transition_at` is `None`
   - apply Blocked/Error → fires Error once
2. Change `distinct_error_reason_fires_again` to assert second transition is
   `None` (same Error class).
3. Update stamp string asserts to `Error` / `Waiting`.
4. Update `idle_to_waiting_fires_once` expected struct to include `client`
   (fixture agent is whatever `waiting_task` uses — match it).
5. Update `notify.rs` webhook tests for `(codex)` or whatever client the
   fixture uses; add `client` field to literals.
6. Red command:
   `cargo nextest run -p ajax-core attention -- distinct_attention waiting_explanation distinct_error acknowledgment_stamp acknowledge_silences idle_to_waiting`
   (adjust filter to renamed tests). Expect compile and/or assertion failures
   before production edits.
7. Then edit production; green the same filters plus
   `cargo nextest run -p ajax-cli notify`.

## Edit instructions

1. `episode_stamp`: status class only.
2. Add `client: String` to `AttentionTransition`; populate from
   `task.selected_agent`.
3. `webhook_command`:
   `{repo}/{handle}: {status} ({client})` then optional explanation suffix.
4. Fix all broken tests in the two allowed files.

## Verification commands

```bash
cargo nextest run -p ajax-core attention
cargo nextest run -p ajax-cli notify
cargo nextest run -p ajax-core attention
cargo nextest run -p ajax-cli notify
cargo clippy -p ajax-core --lib -- -D warnings
cargo clippy -p ajax-cli --all-targets -- -D warnings
cargo fmt --check
```

## Acceptance criteria

- Episode metadata stamp is exactly `Waiting` or `Error` (no `|explanation`).
- Webhook body includes lowercase client in parentheses.
- Waiting explanation churn does not re-fire; Waiting→Error still fires.
- Distinct Error reasons within Error class do not re-fire.
- Focused nextest green; clippy/fmt clean on touched crates.

## Stop conditions

- Need to edit files outside Allowed files.
- `is_actionable_attention` semantics must change to make tests pass.
- Patch exceeds ~400 changed lines.
- Delegate tool unavailable or hang with empty output.
