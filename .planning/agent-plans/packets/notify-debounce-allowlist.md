PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Attention webhooks must (1) fire only for wait/ask Waiting explanations
(`Waiting for input`, `Waiting for approval`) plus any Error, and (2) debounce
15s of sustained actionable attention before the first delivery.

## Allowed files

- `crates/ajax-core/src/attention.rs`
- `crates/ajax-core/src/runtime_refresh.rs` (only the attention assert that uses
  `take_attention_transition` with wall clock — adjust to dwell-aware)
- `crates/ajax-cli/src/notify.rs` (tests only: seed candidate or dwell)
- `crates/ajax-cli/src/web_backend.rs` (tests only: lifecycle wait notify e2e)
- `architecture.md` (one short paragraph on allowlist + debounce)

## Forbidden changes

- Do not edit agent_event.rs, agent_hooks.rs, ui_state.rs, live_application.rs
  production paths.
- Do not add config knobs.
- Do not suppress Error notifies.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- Desired: Cursor `stop`→`done`→`Response ready` already excluded. Broader:
  only structured wait/ask Waiting phone-pings; Auth/Context/Ready for
  review/Rate limited/delegated stay silent. Debounce stops instant hook
  flaps from pinging.
- Architecture: `architecture.md` ~437–445 and ~701–718 — actionable Waiting
  is wait/ask; Cursor has no wait/ask.
- Detector: `attention.rs` `take_attention_transition_at` ~43–90;
  `is_actionable_attention` ~115–126 (currently denylist).
- Existing dwell pattern: `EPISODE_CLEAR_DWELL` + `NOTIFY_QUIET_SINCE_KEY`.
- Tests that assume immediate fire: `idle_to_waiting_fires_once`, episode
  clear tests, `error_within_episode_still_fires`, `waiting_to_error_fires`,
  explanation churn, `notify.rs::notifies_once_and_reports_state_change`,
  `web_refresh_cockpit_lifecycle_wait_notifies_once`,
  `runtime_refresh` CI failed attention assert ~2260.

## Code anchors

1. Add `NOTIFY_CANDIDATE_SINCE_KEY` and
   `NOTIFY_CONFIRMATION_DWELL = Duration::from_secs(15)` (pub const for tests).
2. Flip `is_actionable_attention` Waiting to allowlist:
   `matches!(explanation, "Waiting for input" | "Waiting for approval")`
   (keep delegated filter unnecessary if allowlist-only; Error stays true).
3. In `take_attention_transition_at` Waiting|Error branch after actionable
   check and before stamp compare:
   - if stamp already notified → None (unchanged)
   - read/write candidate: first sight → insert unix now, return None;
     same pending < dwell → None; elapsed ≥ dwell → clear candidate, stamp,
     return Some
   - clear candidate on Running/Idle path and in `silence_notify_episode`
4. Tests: helper `fn confirm_at(task, t) { take_at(t); take_at(t+15) }` or
   explicit timestamps; new tests:
   - `auth_required_waiting_does_not_notify`
   - `notify_debounce_holds_then_fires_once`
   - `debounce_clears_when_returns_to_running`
5. Cross-crate wall-clock tests: seed
   `NOTIFY_CANDIDATE_SINCE_KEY` to `now_secs - 20` before calling notify, or
   after first refresh backdate candidate then refresh again for e2e.

## Test-first instructions

1. Add `notify_debounce_holds_then_fires_once` expecting None at t=1000 and Some
   at t=1015 for Waiting for input. Red until dwell exists.
2. Add `auth_required_waiting_does_not_notify` (AuthRequired live). Red until
   allowlist.
3. Update existing fire-once tests to use t / t+15. Green after implement.

## Edit instructions

Implement allowlist + dwell as above; update every attention test that expected
immediate Some; fix notify.rs and web_backend e2e and runtime_refresh CI
attention assert with candidate seeding or confirm_at; update architecture.md
notify paragraph to name allowlist + 15s confirmation dwell.

## Verification commands

```bash
cargo nextest run -p ajax-core attention -- response_ready rate_limited debounce auth_required idle_to_waiting waiting_cycle error_within
cargo nextest run -p ajax-core attention
cargo nextest run -p ajax-cli notify web_refresh_cockpit_lifecycle_wait
cargo nextest run -p ajax-core runtime_refresh -- github_failed_check
cargo fmt --check
```

## Acceptance criteria

- Waiting for input/approval: None before 15s sustained, then one fire.
- Auth required / Context / Response ready / Ready for review / Rate limited:
  never notify.
- Error still notifies after debounce (or class change gets its own 15s).
- Episode clear + acknowledge still work.
- Only Allowed files changed.

## Stop conditions

- Need clock injection into notify_attention_transitions.
- Need to change ui_state explanation strings.
- Need config knob.
