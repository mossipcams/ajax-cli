# React migration cleanup — master plan

Branch: `ajax/react-migration-cleanup`
Started: 2026-07-18

Follow-on to react slices S1–S7 (`docs/react-migration-plan.md`), which landed
behavioral parity. This program covers the cleanup, structure, lifecycle,
shadcn, a11y, and performance work that S1–S7 deliberately deferred.

## Scope

Twelve independently validated slices, each its own PR, each validated on a real
iPhone before the PR is opened.

## Non-goals

- No redesign. Visual identity, iOS Safari behavior, backend contracts, terminal
  behavior, and task-lifecycle ownership are preserved.
- No new state library (Redux/Zustand/TanStack Query). `architecture.md` forbids
  a browser-side source of truth.
- No renderer replacement. xterm 6.0 + fit addon stay.
- No default shadcn appearance. Semantic tokens map onto existing Ajax tokens.

## Ground truth established before planning

| Fact | Evidence |
| --- | --- |
| No ESLint exists in the repo — no dep, no config | `package.json` devDependencies; no `eslint.config.*` anywhere |
| The two `eslint-disable-next-line react-hooks/exhaustive-deps` comments suppress a linter that never runs | `crates/ajax-web/web/src/components/App.tsx:193,209` |
| A second JS chunk is a hard build failure | `scripts/web-build-check.mjs:38` (`expected exactly dist/app.js`), plus explicit `dist/terminal.js` and preload bans at `:44,:64` |
| `legacyTerminalRemoval.test.ts` `.svelte` paths are intentional absence guards, not stale references | `crates/ajax-web/web/src/legacyTerminalRemoval.test.ts:9-40` — asserts the files do **not** exist |
| Zero Svelte source files, deps, or plugins remain | `find`/`grep` sweep; `package.json` has no svelte entries |
| Live Svelte references are prose/config only | `architecture.md:712-729`, `TERMINAL*.md`, `install.rs` comments, 4 e2e comments, 4 src comments, `tsconfig.json:23` |

## Slices

Each slice: plan file → tests first → focused validation → full web gate →
dev deploy → iPhone checklist → **wait for Matt** → PR.

| # | Slice | Plan file | Status |
| --- | --- | --- | --- |
| 1 | Svelte + documentation cleanup | `react-cleanup-s1-svelte-docs.md` | in progress |
| 2 | ESLint toolchain + dependency boundaries | `react-cleanup-s2-eslint.md` | not started |
| 3 | `useHashRoute` → `useSyncExternalStore` | — | not started |
| 4 | Cockpit/version/task resource hooks (`RemoteResource<T>`) | — | not started |
| 5 | Strict Mode lifecycle safety | — | not started |
| 6 | shadcn foundation + Button | — | not started |
| 7 | Dialog/Sheet + NewTaskSheet | — | not started |
| 8 | RadioGroup + remaining low-risk primitives | — | not started |
| 9 | App composition cleanup (feature folders) | — | not started |
| 10 | Terminal-controller extraction | `react-cleanup-s10-terminal-controller.md` | done (r1+r2a) |
| 11 | Bundle / code-splitting investigation | `react-cleanup-s11-bundle.md` | done — deferred `terminal.js` |
| 12 | Remaining audit findings | — | not started |

## Slice ordering rationale

Ordering differs from a naive read of the request in one place, deliberately:

- **Slice 2 (ESLint) precedes every behavioral slice** so the hooks rules are
  failing loudly *before* slices 3–5 change effect code. Adding the linter after
  the effect rewrites would let the rewrites define the baseline.
- **Slice 9 (feature-folder moves) lands after slices 3–8**, not first. Moving
  files and changing their contents in the same slice makes every diff unreviewable.
  Boundaries are enforced by ESLint from slice 2; the physical moves come once
  the contents have stopped changing.
- **Slice 11 implemented** the deferred `terminal.js` embed (not deferred).

## Known risks

1. **Code splitting vs. the embed contract (slice 11) — resolved.** Embed is now
   four assets (`index.html`, `app.js`, `app.css`, `terminal.js`). Do not use
   `manualChunks` for TaskTerminal/@xterm: that pulled `api.ts` into the
   deferred chunk. Natural dynamic import + `chunkFileNames` → `terminal.js`.
2. **Strict Mode vs. terminal socket cardinality (slice 5).** `docs/react-migration-plan.md:300`
   records an explicit S5 decision *not* to enable Strict Mode because of
   double-effect socket opening. Slice 5 reverses that decision, so it must fix
   the effect first and prove one-socket cardinality, never weaken the test.
3. **Terminal controller extraction (slice 10)** touches the 1,272-line
   `TaskTerminal.tsx`, the highest-risk surface in the app. If clipboard or
   fullscreen ownership cannot move cleanly, extract construction/connection/
   disposal only and continue in separate tested slices.

## Validation gate (every slice)

```bash
npm run web:check
npm run web:test -- --run
npm run web:lint            # from slice 2 onward
npm run web:build:check
npm run web:smoke           # mobile-webkit
cargo nextest run -p ajax-web
npm run verify              # full gate before PR
```

Dev deploy → validate at https://ajaxdev.mossyhome.net:8788 → iPhone checklist.
