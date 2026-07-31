PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

Restore seeded terminal-load landing at the CLI input: when `is-seed-pending`
clears, always snap the xterm viewport and interaction wrap to the bottom.
Also force live-follow before `applyOutput` on seed-pending writes so mid-parse
`onScroll` cannot leave `followLive` false and skip the snap.

## Scope

### Allowed

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/restore-seed-reveal-bottom-snap.md` (checklist only)

### Forbidden

- Do not commit, push, merge, rebase, or change branches.
- Do not edit `terminalScrollSync.ts`, server/PTY, CSS, or e2e unless a focused
  unit test cannot express the contract (prefer source assertions).
- Do not hand-edit `dist/*`; rebuild with `npm run web:build`.
- Do not change quiet/cap timer values, reconnect/`seed=0` policy, or remove
  the seed-pending hide behavior.
- Do not edit files outside Allowed.

## Acceptance

1. `revealSeed` always calls `setFollowLive(true)` then snaps
   (`setSyncingScroll(true)` → `scrollToBottom` → `scrollInteractionToBottom` →
   `setSyncingScroll(false)` → `refreshFollow`) before removing
   `is-seed-pending`. It must **not** gate the snap on `isFollowingLive()`.
2. In the `onOutput` write callback, while seed-pending, call
   `setFollowLive(true)` before `applyOutput()`.
3. Existing seeded-reveal source contract test still passes; extend it to
   assert (1) and (2) explicitly (no `isFollowingLive` gate in `revealSeed`).
4. `npm run web:build` updates `dist/terminal.js` after the source change.

## Constraints

- Keep `SEED_REVEAL_QUIET_MS` / `SEED_REVEAL_MAX_MS` and defer/begin/cancel helpers.
- Keep opacity hide via `.is-seed-pending` on host/spacer.
- Smallest diff; no new abstractions.

## Code anchors

- `TaskTerminal.tsx` `revealSeed` (~693–708): remove `if (scrollSync.isFollowingLive())` gate; force follow + always snap.
- `TaskTerminal.tsx` `onOutput` write callback (~1245–1249): before `applyOutput`, if `isSeedPending()` then `scrollSync.setFollowLive(true)`.
- `TaskTerminal.test.tsx` describe `TaskTerminal seeded history reveal` (~448–507): tighten `revealBody` / `onOutputBody` assertions.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
      expected: seeded reveal suite passes; new assertions green
    - type: build
      command: npm run web:build
      expected: dist/terminal.js rebuilt with force-follow / ungated snap
  broader_checks: []
  reason: Source-contract tests already pin this surface; build keeps served bundle in sync.
```

## Stop if

- Snap still needs `terminalScrollSync` API changes.
- Quiet timer (not followLive gate) is the proven regression.
- Diff grows beyond TaskTerminal + its test + dist rebuild.
- Verification fails and the fix is not obvious in one resume.
