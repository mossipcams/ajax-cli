# Plan: Restore-safe detach (#1181)

## Problem

#1179 made pin mismatch stop tearing down a live child, and fail-closed
restore stopped silent `session/new`. Restore can still **fail because the
host caused it**: idle LRU evicts any idle+unbusy slot (`Shutdown { close:
false }`) without checking that a session id is persisted and that the
harness advertised `session/resume` or `loadSession`.

That is a host-caused `RestoreFailure::Unsupported` (or a fresh session if
the id was never saved). Agent-side `Rejected` (unknown session) is
different and stays fail-closed.

## Decision

**Host-caused restore failure is a defect.** The host may only need restore
after a **restore-safe detach**:

1. ACP session id is already persisted.
2. This slot advertised `session/resume` or `loadSession`.
3. Teardown is detach, not `session/close`.

Idle LRU evicts only restore-safe slots. If restore is not possible, keep
the live child (lease) even under cap pressure. Memory growth of
unrestorable disconnected chats is the honest tradeoff vs destroying model
context.

Process crash/restart still restores from disk when (1)+(2) held at spawn
(`persist-then-install`). If the harness never advertised restore, restart
fail-closes; that is inherent, not an eviction bug.

## Scope

- `docs/architecture/web-session-behavior.md` (continuity + idle eviction)
- `docs/architecture/web-cockpit.md` (idle detach / restore)
- `SpawnReport` / slot: remember restore advertised
- `EvictionSnapshot.evictable` includes restore-safe
- idle eviction tests
- keep #1179 lease/restore-on-mismatch behavior

## Non-goals

- Automatic Retry of `TimedOut` / `TransportLost` (follow-up; still
  fail-closed to the operator today).
- Changing grace duration, `MAX_IDLE_SESSIONS`, Drop/Switch/`/clear`.
- Reconstructing model context when the agent destroyed the session.
- Browser cursor/replay.

## Approval

User requested immediate implementation (2026-09-17): “We need to also
architecturally address preventing restore failures.”

## Tasks

- [x] T1 — Document restore-safe detach and “host-caused restore failure is
      a defect” next to the #1179 continuity invariant. Idle LRU only evicts
      restore-safe slots.
- [x] T2 — Carry `restore_advertised` (resume or load) from initialize onto
      `SpawnReport` and the live `AcpSlot`.
- [x] T3 — `evictable` iff idle, not busy, persisted `acp_session_id` is
      present, and `restore_advertised`.
- [x] T4 — Regression #1181: harness without resume/load is not idle-evicted;
      child pid survives cap pressure; no context-reset note.
- [x] T5 — Existing restore-safe idle eviction still reclaims and later
      acquire restores (existing finished-disconnected test stays green).
- [x] T6 — Align `web-cockpit.md` idle/session-close bullets with T1.

## Validation

```bash
rtk cargo test -p ajax-web --lib -- --test-threads=1 issue_1179 issue_1181 idle_eviction session_close task_session_restore slot_must_replace
```

## Results

Implemented 2026-09-17. `restore_advertised` on `SpawnReport` / `AcpSlot`;
`EvictionSnapshot.evictable` requires restore-safe detach. Regression
`issue_1181_non_restore_harness_survives_idle_cap_pressure` added.

## Deviations

None.
