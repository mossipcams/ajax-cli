# React migration S5 — TaskTerminal

Source of truth: `docs/react-migration-plan.md` §S5; `TERMINAL.md`; `TERMINAL_BEHAVIOR_CONTRACT.md`.

Status: **complete** — Task 1–3 done; `npm run verify` GREEN (327 web tests); on-device §9 matrix passed; PR open.

Delegation decision: delegated via model-router (mechanical port); parent owned e2e diagnosis/validation.

## Scope

- Mechanical port `TaskTerminal.svelte` → `TaskTerminal.tsx` (refs/`useEffect`/`useState`, no redesign).
- Island-swap in `TaskDetail.svelte` only (`handle` prop).
- Move scoped CSS into `styles.css` (convert `:global(...)` to plain selectors).
- Port `TaskTerminal.test.ts` source contracts to `.tsx` / `styles.css`.
- Repoint `keyboardBandPin.test.ts` / `App.test.ts` raw imports from `.svelte` → `.tsx` or `styles.css`.
- Delete `TaskTerminal.svelte` + old test.
- **Do not** enable React StrictMode. Keep `beginExpandFlush` / expand settle path synchronous (use `flushSync` only if required for parity).
- Frozen modules untouched: `terminalConnection.ts`, `terminalGeometry.ts`, `terminalRefit.ts`, `viewport.ts`, `api.ts`.

## Non-goals

- No behavior redesign, no scroll/smoothScroll changes, no shadcn.
- No TaskDetail full migration (S6).
- No Svelte shell changes beyond TaskDetail island line.

## Task checklist

- [x] **Task 1 — mechanical TaskTerminal React port + TaskDetail island + CSS move.**
  - Test first: port source-contract tests to target `.tsx`/`styles.css` paths (RED → GREEN).
  - Implement: `TaskTerminal.tsx`; CSS → `styles.css`; TaskDetail `<ReactIsland component={TaskTerminal} props={{ handle }} />`; delete svelte; update raw imports.
  - Verify unit: TaskTerminal + keyboardBandPin + App source tests GREEN; `web:check` GREEN.
  - **e2e:** `npm run web:smoke -- --project=mobile-webkit terminal-behavior.test.ts` → **66 passed, 1 skipped** (exit 0).

- [x] **Task 2 — full automated validation** (`npm run verify`) → 0 (327 web tests, Rust suite + doctests, 0 type errors).
- [x] **Task 3 — on-device matrix + PR.** On-device §9 matrix passed; PR open.

## Escalate instead of guessing

- Temptation to edit `viewport.ts` / `terminalRefit.ts` / scroll behavior.
- Socket double-connect (StrictMode or effect deps).
- Weakening e2e assertions.

## Deviations

- ReactIsland wrapper broke flex chain `.task-detail` → `.terminal-panel`; fixed with `:has([data-testid="task-terminal-panel"])` flex passthrough in `styles.css`.
- `applyExpandedInert` used `panel.parentElement` (island host) instead of `.task-detail`; fixed with `closest('.task-detail')` + `!child.contains(panel)`.
- Stale `ctrlArmed` closure in xterm `onData` handler; fixed with `ctrlArmedRef` mirror for imperative reads.
- Committed `dist/` bundle was stale (built before deviations 1–3 landed in source): shipped `app.css` was missing the `:has([data-testid="task-terminal-panel"])` flex-passthrough rules. `npm run verify` validates source only, not build output, so it did not flag the drift. Fixed by `npm run web:build`; corrected `dist/app.js`+`app.css` committed with this slice.

## Validation results

| Command | Exit |
| --- | --- |
| `npm run web:test -- --run TaskTerminal.test.tsx keyboardBandPin.test.ts App.test.ts` | 0 (54 passed) |
| `npm run web:check` | 0 |
| `CI=1 npm run web:smoke -- --project=mobile-webkit terminal-behavior.test.ts` | 0 (66 passed, 1 skipped) |
| `npm run verify` | 0 (327 web tests + full Rust suite; 0 type errors) |
| `npm run web:build` (dist rebuild) | 0 (flex-passthrough restored to shipped `app.css`) |
