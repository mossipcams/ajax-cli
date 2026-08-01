# Fix seeded open load-scroll (post-#723)

## Scope

Restore a quiet window long enough to bridge seed→attach, suppress mid-parse
scroll sync while `is-seed-pending`, and keep #723’s always-snap / force-follow
so seeded opens land at the CLI without a visible screenful scroll.

## Non-goals

- Runtime scrollback yank while reading live output
- Reconnect / `seed=0` policy, PTY capture, scroll-sync math rewrite
- Architecture changes

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE:
    - crates/ajax-web/web/src/features/task/TaskTerminal.tsx
    - crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
    - crates/ajax-web/web/e2e/terminal-behavior.test.ts
    - crates/ajax-web/web/dist/terminal.js
    - .planning/agent-plans/fix-seed-reveal-load-scroll.md
  REASON: Frontend seed-reveal fix spanning source+test+e2e+dist exceeds MiniMax 2-file bound.
  ESCALATE_IF:
    - quiet alone insufficient and attach gap needs multi-frame gate
    - fix needs PTY/server changes
```

## Approval

User confirmed symptom (1): open/load still lands mid-history or scrolls a
screenful. Authorized implement of attached plan.

## Task checklist

### Task 1: Persistent plan + packet

- [x] Plan file current
- [x] READY packet at `.planning/packets/fix-seed-reveal-load-scroll.md`

### Task 2: Failing tests

- [x] Source contract: `SEED_REVEAL_QUIET_MS = 120`; pending scroll suppress;
      `revealSeed` calls `syncSpacer` before snap
- [x] E2E: 80ms gap after seed still `is-seed-pending`; then attach lands bottom

### Task 3: Implement

- [x] Restore quiet 120; comment matches ~7 batches
- [x] Suppress `onTermScroll` / interaction→term sync while seed-pending
- [x] `syncSpacer()` before reveal snap; keep force-follow + always snap
- [x] Rebuild `web/dist/terminal.js`

### Task 4: Validate

- [x] Parent re-runs focused vitest + reviews diff
- [x] Focused e2e skipped (no web:smoke server; connection refused) — e2e wait
      assertion still updated for CI smoke

## Deviations

- Cursor delegate wrote correct scoped delta but `DELEGATE_REPORT` failed
  schema extraction (`INVALID_STRUCTURED_REPORT` / `FILES_CHANGED`). Parent
  accepts on reviewed diff + parent-run verification, not the broken YAML.
- Focused playwright without `web:smoke` harness could not connect to
  localhost:5173; not treated as a product failure.
- CI Web smoke: suppressing wrapper scroll + force-follow on pending writes
  broke "New output" e2e (scroll during quiet left followLive true / re-pinned).
  Fix: keep only `onTermScroll` suppress while pending; let wrapper scroll
  update followLive; remove pending-write force-follow (reveal still always
  snaps).

## Validation ledger

- `npm run web:test -- --run …/TaskTerminal.test.tsx` — EXIT 0 (24 passed)
- Delegate `npm run web:build` — EXIT 0; dist embeds `rg=120,ng=2e3`
- Focused e2e `-g 'seeded open…'` — SKIPPED (no vite server)
- Review gate: **ACCEPT** (scoped delta matches packet)
- Husky pre-commit (`npm run web:build` + `npm run verify` + release build/install) — EXIT 0
- PR: https://github.com/mossipcams/ajax-cli/pull/732
