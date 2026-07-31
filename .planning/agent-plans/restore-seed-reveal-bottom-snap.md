# Restore seed-reveal bottom snap on terminal load

## Scope

Fix Web Cockpit task terminal so a seeded open again lands at the CLI input
(bottom) when `is-seed-pending` clears — without a visible load scroll.

Root cause: `#672` still hides until quiet and still has `revealSeed`, but the
snap is gated on `scrollSync.isFollowingLive()`. While the seed/`write` parses,
xterm fires `onScroll` with `syncingScroll === false`, which can flip
`followLive` to false before the write callback. Reveal then drops opacity
without snapping — user opens mid-history / not at the CLI.

## Non-goals

- Reconnect / `seed=0` policy
- Scroll sync math rewrite
- Quiet/cap timer tuning unless a test proves the gap is wrong
- Architecture changes

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: BUILD_PACKET
  LANE: tdd-implementation-packet
  MODE: build
  MODEL: NONE
  PACKET_STATUS: BLOCKED
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE:
    - crates/ajax-web/web/src/features/task/TaskTerminal.tsx
    - crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
    - crates/ajax-web/web/dist/terminal.js
    - .planning/packets/restore-seed-reveal-bottom-snap.md
  REASON: Evidence complete; build READY packet then MiniMax implement.
  ESCALATE_IF:
    - quiet timer itself is the regression (not followLive gate)
    - fix needs server/PTY changes
```

## Approval

User reported the past load-scroll/bottom-landing fix was lost — authorized.

## Task checklist

### Task 1: Pin reveal always snaps (test)

- [x] Update `TaskTerminal.test.tsx` seeded-reveal source contract so
      `revealSeed` always snaps (no `isFollowingLive()` gate), and seed-pending
      writes force live follow before `applyOutput`
- [x] Confirm RED against current source (or document already-red assertion)

### Task 2: Implement

- [x] In `revealSeed`, force `setFollowLive(true)` and always snap before
      removing `is-seed-pending`
- [x] In `onOutput` write callback while seed-pending, `setFollowLive(true)`
      before `applyOutput` (so mid-parse scroll cannot leave follow off)
- [x] Rebuild `web/dist/terminal.js` if that is the repo convention for this
      surface

### Task 3: Verify

- [x] Focused vitest for TaskTerminal seeded reveal
- [x] Parent re-runs validation and reviews diff

### Task 4: Speed seed→CLI reveal (user follow-up)

- [x] Drop `SEED_REVEAL_QUIET_MS` 120 → 48 (~3 bridge batches; still covers seed→attach gap)
- [x] Align e2e mid-gap wait below the new quiet floor
- [x] Rebuild dist; re-run focused vitest

## Deviations

- MiniMax round 1: monthly usage limit → DISCARD / escalate to Cursor.
- Cursor round 2: correct source+test+dist delta, but `DELEGATE_REPORT` failed
  schema extraction (`INVALID_STRUCTURED_REPORT`). Parent accepts on reviewed
  diff + verification_results exit 0, not the broken YAML.
- Quiet-timer speedup done parent-local (tiny constant + e2e wait); not re-delegated.

## Validation ledger

- `npm run web:test -- --run …/TaskTerminal.test.tsx` — EXIT 0 (24 passed)
- `npm run web:build` — EXIT 0; dist has `Gp=48,Jp=2e3`
- Review gate: **ACCEPT** (Cursor delta + parent quiet speedup)
