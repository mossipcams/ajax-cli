PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

Fix seeded terminal open load-scroll after #723: restore
`SEED_REVEAL_QUIET_MS = 120` so seed→attach gaps stay hidden, suppress
mid-parse scroll sync while `is-seed-pending`, call `syncSpacer()` before
reveal snap, and keep #723’s force-follow + always-snap so the surface lands
at the CLI with no visible screenful scroll.

## Scope

### Allowed

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/fix-seed-reveal-load-scroll.md` (checklist only)

### Forbidden

- Do not commit, push, merge, rebase, or change branches.
- Do not edit `terminalScrollSync.ts`, server/PTY, or CSS.
- Do not hand-edit `dist/*`; rebuild with `npm run web:build`.
- Do not change reconnect/`seed=0` policy or remove seed-pending hide.
- Do not edit files outside Allowed.

## Acceptance

1. `SEED_REVEAL_QUIET_MS = 120` and `SEED_REVEAL_MAX_MS = 2000`. Comment
   describes ~7 bridge batches (not ~3).
2. While `is-seed-pending`, term `onScroll` returns before
   `scrollSync.onTermScroll()`, and wrap scroll returns before
   `onInteractionScroll()` (pinned-scroll restore still runs first).
3. `revealSeed` calls `scrollSync.syncSpacer()` before `setFollowLive(true)` /
   snap / remove `is-seed-pending`. Keep always-snap (no `isFollowingLive`
   gate) and pending-write force-follow before `applyOutput`.
4. E2E seeded-open waits **80ms** after seed (under 120, above broken 48) and
   still has `is-seed-pending`; after attach chunk, pending clears and surface
   is at bottom.
5. `npm run web:build` updates `dist/terminal.js`.

## Constraints

- Smallest diff; no new abstractions.
- Keep opacity hide via `.is-seed-pending` on host/spacer.
- Keep defer/begin/cancel helpers and 2s cap.

## Code anchors

- `TaskTerminal.tsx` `SEED_REVEAL_QUIET_MS` (~35): `48` → `120`; fix comment.
- `TaskTerminal.tsx` `revealSeed` (~764–774): insert `scrollSync.syncSpacer()`
  before `setFollowLive(true)`.
- `TaskTerminal.tsx` scroll wiring (~1286–1291): wrap `onTermScroll` /
  `onInteractionScroll` with `if (isSeedPending()) return`.
- `TaskTerminal.test.tsx` describe `TaskTerminal seeded history reveal`:
  assert quiet `= 120`, pending scroll guards, `syncSpacer` before snap.
- `e2e/terminal-behavior.test.ts` seeded open test (~2992–2995): wait 80ms.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
      expected: seeded reveal suite passes with quiet=120 / suppress / syncSpacer asserts
    - type: build
      command: npm run web:build
      expected: dist/terminal.js rebuilt with quiet 120
  broader_checks: []
  reason: Source-contract + e2e wait pin the regression; build keeps served bundle in sync.
```

## Stop if

- Fix needs `terminalScrollSync` API changes or PTY/server work.
- Diff grows beyond Allowed scope.
- Verification fails and the fix is not obvious in one resume.
