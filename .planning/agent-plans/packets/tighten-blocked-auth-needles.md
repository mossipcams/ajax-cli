PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Casual agent prose containing only `"blocked"` or `"authenticate"` must not
classify as stuck `Blocked` / `AuthRequired`. Real stuck phrases
(`"cannot continue"`, `"manual intervention required"`, `"please login"`,
`"auth required"`, etc.) must still classify.

## Allowed files

- `crates/ajax-core/src/live.rs`

## Forbidden changes

- Do not change attention notify stamps, dwell gates, or ui_state.
- Do not remove `"cannot continue"` / `"manual intervention required"` /
  login / `"auth required"` needles.
- Do not edit ajax-web / ajax-cli / architecture.md.
- No renames or formatting sweeps beyond the touched lists/tests.

## Context evidence

- Desired: `"The deploy path is blocked by the existing lockfile."` and
  `"Next we authenticate the webhook signature..."` →
  `project_pane_stuck_status` = `None`.
- Still stuck: `"this is blocked, cannot continue"` → `Blocked` (via
  `"cannot continue"`); `"Please login to continue"` / `"auth required"` →
  `AuthRequired`.
- Matcher (`live.rs:811-837`): Auth list includes bare `"authenticate"`;
  Blocked list includes bare `"blocked"`.
- Characterization today (`live.rs:1508-1525`):
  `broad_stuck_needles_currently_match_casual_agent_prose` asserts `Some(...)`.
  Flip to desired `None` and rename.
- Real fixtures already use `"this is blocked, cannot continue"`
  (`live.rs:1387`, `:1467`, `:1477`) — survive without bare `"blocked"`.

## Code anchors

- `crates/ajax-core/src/live.rs:811` — AuthRequired `contains_any`
- `crates/ajax-core/src/live.rs:833` — Blocked `contains_any`
- `crates/ajax-core/src/live.rs:1508` — characterization test to flip
- `crates/ajax-core/src/live.rs:1387` — real Blocked fixture with cannot continue
- `crates/ajax-core/src/live.rs:1538` — `"Please login to continue"` still Auth

## Test-first instructions

1. Rename `broad_stuck_needles_currently_match_casual_agent_prose` →
   `broad_stuck_needles_do_not_match_casual_agent_prose`.
2. Change both asserts from `Some(Blocked/AuthRequired)` to `None`.
3. Add/keep asserts that real phrases still match, either in this test or
   rely on existing `pane_stuck_states_survive_the_activity_projection` /
   `pane_classifier_detects_agent_attention_states` (must stay green).

Red:

```bash
cargo nextest run -p ajax-core broad_stuck_needles_do_not_match_casual_agent_prose
```

Expect nonzero exit (still matching Some today) before production edit.

## Edit instructions

In `pane_evidence`:

1. AuthRequired list: remove `"authenticate"` only. Keep
   `please login`, `please log in`, `log in to`, `login to continue`,
   `auth required`.
2. Blocked list: remove `"blocked"` only. Keep
   `cannot continue`, `manual intervention required`.

No other needle changes.

## Verification commands

```bash
cargo nextest run -p ajax-core broad_stuck_needles_do_not_match_casual_agent_prose
cargo nextest run -p ajax-core pane_stuck pane_classifier_detects try_again_later
cargo fmt --check
cargo clippy -p ajax-core --all-targets --all-features -- -D warnings
```

## Acceptance criteria

- Casual blocked/authenticate prose → `None`.
- `"this is blocked, cannot continue"` still → `Blocked`.
- `"Please login to continue"` / auth-required phrasing still → `AuthRequired`.
- Diff only `live.rs`.

## Stop conditions

- Existing stuck fixtures fail and need production changes beyond the two
  needle removals.
- Scope spreads outside `live.rs`.
