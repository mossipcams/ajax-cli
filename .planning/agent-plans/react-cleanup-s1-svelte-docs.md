# Slice 1 — Svelte and documentation cleanup

Master plan: `react-migration-cleanup.md`

## Scope

Remove obsolete Svelte references from **active** claims: config, source
comments, Rust test comments/names, e2e comments, `architecture.md`, `TERMINAL.md`.

## Non-goals

- No behavior change. Comments, docs, one test name, one tsconfig `include` entry.
- No touching `legacyTerminalRemoval.test.ts` — its `.svelte` paths are
  intentional **absence** guards (asserts the files do not exist).
- No rewriting `TERMINAL_BEHAVIOR_CONTRACT.md` / `TERMINAL_LEGACY_SURFACE_TESTS.md`
  citation rows — see decision below.
- Not removing the `eslint-disable` suppressions in `App.tsx` — slice 2 owns those
  lines. Slice 1 corrects only the stale Svelte wording on the same comment.

## Delegation decision

`Delegation decision: not delegated because the change is comment-, doc-, and
config-only` (AGENTS.md delegation exception: "one-line typo, formatting-only
edit, or comment-only correction"). Every edit is smaller than the work order
that would describe it, and none alters behavior.

## Decision: historical documents are preserved, not rewritten

`TERMINAL_BEHAVIOR_CONTRACT.md` and `TERMINAL_LEGACY_SURFACE_TESTS.md` carry
~60 `.svelte` citations with line numbers. Both are already self-labelled as
historical records of the removed Ghostty/Surface-V2 surface
(`TERMINAL_LEGACY_SURFACE_TESTS.md:1-6`, `TERMINAL_BEHAVIOR_CONTRACT.md:3-4`).
Those citations are accurate references into deliberately deleted files;
repointing them at `TaskTerminal.tsx` line numbers would fabricate provenance.

They are therefore **not** "active architectural claims naming `.svelte` owners".
One exception: `TERMINAL_BEHAVIOR_CONTRACT.md:6` calls itself an inventory of
"the current Ajax browser terminal", contradicting its own status line at :3-4.
That single sentence is corrected to say pre-removal.

## Tests

Mechanical change; no new tests, per AGENTS.md ("comments or docs", "pure renames
with compiler coverage"). Coverage that already protects this slice:

- `cargo nextest run -p ajax-web` — the renamed `install.rs` test still asserts
  the same shell contract; a rename that broke it would fail to compile.
- `npm run web:check` — proves the `tsconfig.json` include change does not drop
  any file from type checking.
- `npm run web:test -- --run` — `legacyTerminalRemoval.test.ts` proves no Svelte
  file reappeared.

## Task checklist

- [x] T1 `crates/ajax-web/web/tsconfig.json:23` — drop `"src/**/*.svelte"` from `include`
- [x] T2 `crates/ajax-web/src/slices/install.rs` — module doc, `Svelte mounts` comment, `rendered client-side by Svelte components` comment, rename `shell_is_the_bundled_svelte_mount_point` → `shell_is_the_bundled_react_mount_point`
- [x] T3 `web/src/routes.ts:2` — "move into Svelte" → framework-neutral
- [x] T4 `web/src/taskActions.ts:3` — `App.svelte` → `App.tsx`
- [x] T5 `web/src/components/App.tsx:208` — drop the stale "exact Svelte $effect dep set" claim (suppression itself left for slice 2)
- [x] T6 `web/src/components/keyboardBandPin.test.ts:97` — ".svelte source" → source of truth wording
- [x] T7 e2e comments: `smoke.test.ts:3`, `swipe-reveal.test.ts:1`, `shell-characterization.test.ts:2` — drop present-tense Svelte claims
- [x] T8 `e2e/visual.test.ts:1` — kept; it is accurate past-tense history explaining why the guard exists (see `svelte_migration_dropped_css` memory)
- [x] T9 `architecture.md:712-729` — `.svelte` owners → `.tsx`
- [x] T10 `web/TERMINAL.md:5-15` — `.svelte` owners → `.tsx`
- [x] T11 `TERMINAL_BEHAVIOR_CONTRACT.md:6` — "current" → "pre-removal"
- [x] T12 Confirm zero Svelte source files / deps / plugins / active claims

## Validation

| Command | Result |
| --- | --- |
| `npm install` | required first — fresh worktree had no `node_modules` |
| `npm run web:check` | pass — tsc clean |
| `npm run web:test -- --run` | pass — 36 files, 321 tests, 0 failed |
| `cargo nextest run -p ajax-web` | pass — 159/159 |
| `npm run web:build:check` | pass — deterministic shell, version placeholder intact |
| `npm run web:smoke` | pass — 118 passed, 0 failed (70 skipped = desktop-chromium inventory-only) |
| `npm run verify` | pass — exit 0, 1628 Rust tests, 321 vitest |

### Zero-runtime-delta proof

Built `dist/app.js` + `dist/app.css` before and after the slice are byte-identical:

```
app.js   45aa35e0672c0364a3ccfe95706728e6e8c5436ca6cdfa8833c08b01233dc131
app.css  abfe25a1896beca51d3a2fdf5768b930e330b398fe3916a7b564f3624be3aa67
```

(`git stash` → build → hash → `stash pop` → build → hash.) Comments are stripped
at build time and the only non-comment edits were `tsconfig.json` `include`, a
Rust test name, and Markdown. **No iPhone validation is required for this slice** —
there is no shipped byte for a device to exercise.

### Bundle baseline recorded for slice 11

| Asset | Raw | Gzip |
| --- | --- | --- |
| `dist/app.js` | 592.74 kB | 165.69 kB |
| `dist/app.css` | 43.52 kB | 9.25 kB |

Vite emits a >500 kB chunk-size warning on `app.js`, which is the concrete
motivation for the slice 11 investigation.

## Deviations

- T8: planned as an edit, kept as-is. The `visual.test.ts` comment is accurate
  history (the migration *did* once ship a stub stylesheet) and it is the
  rationale for the guard existing. Removing it would delete the reason.
- `TERMINAL_BEHAVIOR_CONTRACT.md` / `TERMINAL_LEGACY_SURFACE_TESTS.md` citation
  rows preserved rather than rewritten — see decision above.
