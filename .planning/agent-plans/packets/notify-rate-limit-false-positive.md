PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Stop false `RateLimited` status from casual pane text containing only
"try again later", and stop attention webhooks for Waiting explanation
`"Rate limited"` (transient wait, not actionable operator input). Keep real
rate-limit phrases (`rate limit`, `too many requests`) classifying as
`RateLimited` for UI/status, but do not phone-ping them.

## Allowed files

- `crates/ajax-core/src/live.rs`
- `crates/ajax-core/src/attention.rs`
- `architecture.md`

## Forbidden changes

- Do not change Waiting confirmation dwell, episode-clear, or metadata keys.
- Do not edit ajax-web / notify.rs / PWA presence (Round B).
- Do not remove `LiveStatusKind::RateLimited` or change its `LiveStatusClass`.
- No renames, formatting sweeps, or unrelated cleanup.

## Context evidence

- Desired: `"I'll try again later"` must not become Waiting/"Rate limited" or
  fire a webhook. `"rate limit exceeded"` may still show Rate limited in UI but
  must not webhook.
- Matcher today (`live.rs:825-829`):
  `contains_any(&lower, &["rate limit", "too many requests", "try again later"])`
  → `PaneEvidence::RateLimited`. The third phrase is the false-positive source.
- Stuck projection (`live.rs:513-531`) surfaces RateLimited from the pane tail
  via `classify_agent_pane` → actionable Waiting.
- Notify gate (`attention.rs:117-127`): Waiting is actionable unless Ready for
  review / delegated. `"Rate limited"` is currently actionable.
- Architecture (`architecture.md:652-656`) lists rate limited among phone-ping
  Waiting evidence — update that bullet to match.

## Code anchors

- `crates/ajax-core/src/live.rs:825` — rate-limit `contains_any` list
- `crates/ajax-core/src/live.rs:1476-1480` — existing fixture
  `"rate limit exceeded; try again later"` (still RateLimited via `"rate limit"`)
- `crates/ajax-core/src/live.rs:1387` — `pane_stuck_states_survive_the_activity_projection`
- `crates/ajax-core/src/attention.rs:117` — `is_actionable_attention`
- `architecture.md:652` — notify actionable list

## Test-first instructions

1. In `live.rs` tests near pane stuck / classify fixtures, add:
   - `try_again_later_alone_is_not_rate_limited`
     - Pane/tail containing only casual text like
       `"Looks stuck — I'll try again later."` (no "rate limit" / "too many
       requests") → `classify_agent_pane` / `project_pane_stuck_status` must
       **not** yield `LiveStatusKind::RateLimited`.
   - Keep proving `"rate limit exceeded"` still → `RateLimited`
     (extend existing table or one focused assert).

2. In `attention.rs` tests:
   - `rate_limited_waiting_does_not_notify`
     - Active task + `apply_observation(RateLimited, "rate limited")` (or
       live observation that derives Waiting/"Rate limited")
     - `take_attention_transition` → `None`, no `LAST_NOTIFIED_STATUS_KEY`.

Red command:

```bash
cargo nextest run -p ajax-core try_again_later_alone_is_not_rate_limited rate_limited_waiting_does_not_notify
```

Expect nonzero exit on those asserts before production edits.

## Edit instructions

1. `live.rs` `pane_evidence`: remove `"try again later"` from the rate-limit
   phrase list. Leave `"rate limit"` and `"too many requests"`.
2. `attention.rs` `is_actionable_attention`: for Waiting, also reject when
   explanation is `"Rate limited"` (same style as Ready for review).
3. `architecture.md`: in the notify actionable Waiting list, remove
   "rate limited" (keep auth/approval/input/context as appropriate).

## Verification commands

```bash
cargo nextest run -p ajax-core try_again_later_alone_is_not_rate_limited rate_limited_waiting_does_not_notify
cargo nextest run -p ajax-core attention live
cargo fmt --check
cargo clippy -p ajax-core --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- Casual "try again later" alone ≠ RateLimited.
- Real rate-limit phrases still classify RateLimited.
- Waiting/"Rate limited" does not webhook or stamp.
- Diff only Allowed files; architecture bullet updated.

## Stop conditions

- Need to change `LiveStatusClass` membership or ui_state precedence.
- Need to touch ajax-web presence code.
- Diff exceeds ~100 lines or spreads outside Allowed files.
