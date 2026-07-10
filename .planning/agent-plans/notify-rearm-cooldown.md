# Notify re-arm cooldown

## Scope

Field report (2026-07-10): ~20 ntfy pings in one day, mostly `ajax-cli/notification-engine`,
spaced minutes apart. Diagnosis: the pings track agent turn boundaries — each genuine
`Waiting → Running → Waiting` cycle re-arms the rising-edge detector immediately
(`take_attention_transition` removes `last_notified_status` the moment status returns to
Running/Idle), so an actively driven session pings once per agent turn. The dwell gate
(PR #427) only suppresses sub-4s flaps; it does not touch turn-boundary re-fires.

Fix: delay re-arm. When status returns to Running/Idle, only clear the dedup stamp if the
last delivery is older than a cooldown (5 min). Within the cooldown the stamp survives the
Running interlude, so the next Waiting of the same episode does not re-fire. A status
*change* (e.g. Waiting → Error) still fires immediately because it never matches the stamp.

Non-goals: per-event config, config knob for the cooldown (constant first), operator
activity detection (tmux client_activity gating — the upgrade path if a ping per 5 min is
still too chatty), any change to the dwell gate or web tick.

## Execution

Delegation: local implementation under packet constraints, per the recorded fallback
(opencode-go/glm-5.2 lane hung twice on this repo; see status-notification-redesign.md
deviations). Branch `ajax/notify-cooldown` stacked on `ajax/notification-engine`
(PR base = that branch; retargets to main when #427 merges).

## Tasks

- [x] Test: fired Waiting → Running within cooldown keeps the stamp; Waiting again → no
      second fire; Running after cooldown clears both keys; Waiting after that → fires.
      (`waiting_cycle_within_cooldown_fires_once`)
- [x] Test: Waiting → Error within cooldown still fires (status differs from stamp).
      (`error_within_cooldown_still_fires`)
- [x] Update `waiting_then_idle_then_waiting_fires_again` to explicit timestamps past the
      cooldown (renamed `waiting_then_idle_past_cooldown_then_waiting_fires_again`; same
      invariant, clock-aware API).
- [x] Implement: `take_attention_transition_at(task, now)`; no-arg wrapper keeps
      `SystemTime::now()`. New metadata key `last_notified_at` (unix secs) written on fire;
      Running/Idle branch clears stamps only when `now - last_notified_at >= 300s`
      (missing/malformed value clears as before).
- [x] Validation: attention-filtered nextest 41/41; `cargo fmt --check` clean;
      `cargo clippy --all-targets --all-features -- -D warnings` clean;
      `cargo nextest run --all-features` 1570/1570 passed.

## Ledger

- 2026-07-10: implemented locally (recorded delegate-lane fallback, no retry). All tasks
  done; only production diff is `crates/ajax-core/src/attention.rs`.
