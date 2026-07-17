PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Lock the S1 Tailwind contract: extend the existing source-level design test to require a preflight-free Tailwind-v4 utilities import and an `@theme` mapping onto the existing Ajax tokens, then add exactly those declarations to `styles.css`.

## Allowed files

Edit:

- `crates/ajax-web/web/src/design-colors.test.ts`
- `crates/ajax-web/web/src/styles.css`

## Forbidden changes

- Do not edit any other file: no components, seam files, package files, Vite/TS config, `components.json`, e2e tests, Rust, or generated `dist/` (do not run `npm run web:build`; the parent runs build, bundle greps, and Rust guards).
- Do not weaken, delete, or rewrite the two existing tests in `design-colors.test.ts`; only add.
- In `styles.css`, do not modify or move any existing rule. Add only (a) the utilities `@import` at the very top and (b) one `@theme inline` block at the end. No new color hex literals anywhere; `@theme` values must be `var(--…)` references to existing custom properties.
- Do not import `tailwindcss` bare, `tailwindcss/preflight`, or `tailwindcss/theme` — utilities only, preflight off.
- Do not add Tailwind classes to any component, generate shadcn components, or add dependencies.
- Do not commit, push, merge, rebase, create/switch branches, or use `git checkout`/`git restore`.

## Context evidence

- Behavior source of truth: `docs/react-migration-plan.md` D7 and S1 — "Add Tailwind v4 via `@tailwindcss/vite`, utilities + theme only — no preflight"; "Theme maps to the existing custom properties (`--paper`, `--ink`, `--accent`, …), never duplicates hex values"; S1 target: "Tailwind v4 entry (`@import "tailwindcss/utilities"` + `@theme` token mapping appended to the CSS pipeline, preflight off)".
- Pipeline: `@tailwindcss/vite` 4.3.3 is already in `vite.config.mts` plugins (Task 1); `components.json` `tailwind.css` points at `src/styles.css`, so `styles.css` is the Tailwind entry. It currently contains zero Tailwind directives (`rg "@import|@theme" styles.css` finds nothing).
- Existing contract: `design-colors.test.ts` has two tests parsing `DESIGN.md` frontmatter and the first `:root { … }` block of `styles.css` (helper `rootCustomProps` matches the first `:root` only, so a trailing `@theme` block cannot disturb it). Role aliases asserted: `paper`, `accent`, `warn`, `danger`, `ok`.
- Token anchors in `styles.css` `:root`: DESIGN.md names (`--soft-charcoal`, `--soft-steel-blue`, `--attention-amber`, `--fault-rose`, `--done-sage`, `--ink`, …) plus role aliases (`--paper`, `--paper-tint`, `--paper-raised`, `--paper-high`, `--accent`, `--warn`, `--danger`, `--ok`).
- CSS validity: `@import` must precede all style rules; comments may precede it, so it goes after the file-header comment block, before `:root`.
- Preflight risk (S1 risk table): preflight would restyle live Svelte components; the new contract test plus the parent-run install guards catch it.

## Code anchors

- `design-colors.test.ts`: append one new `describe("Tailwind contract", …)` after the existing `describe` block, reusing the already-loaded `stylesCss` string. Assert at minimum:
  1. `stylesCss` contains an `@import` of `tailwindcss/utilities` (allow an optional `layer(...)` suffix).
  2. `stylesCss` contains no bare `@import "tailwindcss"`, no `tailwindcss/preflight`, and no `tailwindcss/theme` import.
  3. `stylesCss` contains exactly one `@theme inline` block; every declaration inside it has the form `--color-<name>: var(--<existing-token>);` — no `#` hex literal appears inside the block.
  4. The `@theme` block maps at least the five locked role tokens: `--color-paper: var(--paper)`, `--color-ink: var(--ink)`, `--color-accent: var(--accent)`, `--color-warn: var(--warn)`, `--color-danger: var(--danger)`, `--color-ok: var(--ok)` (paper/ink plus the four status roles; six mappings minimum).
- `styles.css` top anchor: insert `@import "tailwindcss/utilities" layer(utilities);` on its own line immediately after the closing `*/` of the file-header comment and before `:root {`.
- `styles.css` end anchor: append after the final `@media (max-width: 380px) { … }` block one `@theme inline { … }` section (with a `/* TAILWIND THEME ... */` header comment in the file's existing section style) containing exactly the token mappings the test requires — `var(--…)` references only.

## Test-first instructions

1. Edit only `design-colors.test.ts` first; `styles.css` stays untouched.
2. Add the `Tailwind contract` describe block with the assertions in Code anchors.
3. Run exactly this focused command before the CSS edit:

```bash
npm run web:test -- --run crates/ajax-web/web/src/design-colors.test.ts
```

4. It must exit nonzero with the two original tests passing and the new Tailwind assertions failing because `styles.css` has no Tailwind declarations. Preserve an output excerpt.

## Edit instructions

1. Add the `@import` line at the top anchor and the `@theme inline` block at the end anchor of `styles.css`, exactly as specified — nothing else.
2. Rerun the exact RED command and record GREEN with all tests (old and new) passing.
3. Do not reformat unrelated lines in either file.

## Verification commands

Run in order from the repository root:

```bash
npm run web:test -- --run crates/ajax-web/web/src/design-colors.test.ts
npm run web:test -- --run crates/ajax-web/web/src/components/App.test.ts
npm run web:check
```

(The parent separately runs `npm run web:build`, `grep -c serviceWorker crates/ajax-web/web/dist/app.js`, `npm run web:build:check`, and `cargo nextest run -p ajax-web` at the review gate.)

## Acceptance criteria

- The focused command records the intended RED (new assertions only) before the CSS edit and full GREEN afterward.
- Both pre-existing design tests pass unchanged; App suite stays 34/34.
- `styles.css` gains exactly two additions: the utilities import line and the `@theme inline` block; `git diff` shows no other styles.css hunks.
- No hex color literal appears in the diff.
- Patch stays below roughly 80 changed lines across the two files.

## Stop conditions

- The new assertions unexpectedly pass before the CSS edit, or the RED run fails in the two pre-existing tests.
- Any file outside the two Allowed files appears to need editing.
- Making the contract pass seems to require importing preflight/theme, editing existing CSS rules, or adding a hex literal.
- The patch exceeds Allowed files or roughly 80 changed lines.
