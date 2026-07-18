# React Slice S6 — Task detail + TestInDevPanel (+ ActionBar dedup)

Worktree: `ajax-cli__worktrees/react-s6`, branch `ajax/react-s6`, off `origin/main` @ 6e864e0.
Blueprint: `docs/react-migration-plan.md` §7 S6. Deps S2 + S5 both merged.

## Scope

Migrate `TaskDetail.svelte` + `TestInDevPanel.svelte` to React islands, delete the
frozen `ActionBar.svelte` duplicate, port their vitest suites to
`@testing-library/react`, repoint `legacyTerminalRemoval.test.ts` from
`TaskDetail.svelte` → `TaskDetail.tsx`, and swap `App.svelte` to mount TaskDetail
via `ReactIsland`.

## Non-goals

- No S7 shell inversion. No behavior change; e2e frozen; frozen TS modules byte-identical (imports aside).

## Delegation decision

`Delegation decision: delegated via model-router`

## Decomposition (two bounded rounds)

- [x] **Round A — TestInDevPanel.tsx (leaf)** — delegated to Cursor/composer-2.5, ACCEPTED. Gate: vitest 24/24, web:check clean, build+build:check pass, serviceWorker=0, cargo nextest -p ajax-web 159/159. NOTE: original react-s6 worktree was deleted externally before commit; Round A reconstructed from context and committed immediately.
- [ ] **Round B — TaskDetail.tsx + ActionBar dedup + shell swap**
  - Port `TaskDetail.svelte` → `TaskDetail.tsx`, importing `TestInDevPanel.tsx`, `ActionBar.tsx`, `TaskTerminal.tsx` natively (drop the Round-A island wrapper).
  - Move TaskDetail scoped styles into `styles.css`.
  - Port `TaskDetail.test.ts` → `.test.tsx`.
  - Delete `ActionBar.svelte` + `ActionBar.test.ts`; `ActionBar.tsx` is sole impl.
  - Swap `App.svelte`: `<TaskDetail .../>` → `<ReactIsland component={TaskDetail} props={...}/>`.
  - Repoint `legacyTerminalRemoval.test.ts` `TaskDetail.svelte` → `TaskDetail.tsx` (assertions unchanged).

## Validation commands (per round, in order)

```bash
npm run web:build
grep -c serviceWorker crates/ajax-web/web/dist/app.js   # expect 0
npm run web:check
npm run web:test -- --run
npm run web:build:check
cargo nextest run -p ajax-web
```

## On-device gate (Matt — required before PR, §9)

Full open-task journey from dashboard; copy buttons; destructive confirm; Test in
Dev redeploy from the phone (self-hosting proof); terminal within detail unchanged.

## Deviations

- 2026-07-18: original `react-s6` worktree + `ajax/react-s6` branch were deleted externally between turns (cause unknown) while Round A was accepted-but-uncommitted; Round A reconstructed from context and committed immediately. Policy for the rest of S6: commit each round as soon as it passes the gate.

## Validation results

- Round A: PASS — vitest 24/24, web:check 0/0, build + build:check pass, serviceWorker=0.
