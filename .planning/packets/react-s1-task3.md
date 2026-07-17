PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port `Skeleton` from Svelte to a React component hosted by the existing one-way `ReactIsland`, preserving its DOM (`.skeleton` root, `data-testid`, `aria-hidden="true"`, N `.skeleton-row` children) and moving its component CSS unchanged into the authoritative stylesheet.

## Allowed files

Create:

- `crates/ajax-web/web/src/components/Skeleton.tsx`
- `crates/ajax-web/web/src/components/Skeleton.test.tsx`

Edit:

- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/styles.css`

Delete after the React test is green:

- `crates/ajax-web/web/src/components/Skeleton.svelte`

## Forbidden changes

- Do not edit `ReactIsland.svelte`, `mountIsland.tsx`, `ErrorBoundary.tsx`, `ConnectionStatus.tsx`, any test other than the new `Skeleton.test.tsx`, package files, Vite/TypeScript config, fixtures, e2e tests, Rust, or generated `dist/`.
- Do not alter the rendered DOM shape, class names, `data-testid` values (`task-skeleton`, `dashboard-skeleton`), `aria-hidden`, row counts (6 for task, 4 for dashboard), or the default `rows = 4`.
- In `styles.css`, only add the copied skeleton rules; do not reorder, reformat, or edit any existing rule, and do not introduce new color literals — the copied rules use only existing CSS variables.
- Do not add shadcn, Tailwind classes, hooks, state, memoization, dependencies, or abstractions.
- Do not commit, push, merge, rebase, create/switch branches, run a production build, or use `git checkout`/`git restore`.

## Context evidence

- Behavior: `Skeleton.svelte` is purely decorative — props `rows = 4` and optional `testid`; renders `<div class="skeleton" data-testid={testid} aria-hidden="true">` containing `rows` empty `<div class="skeleton-row">` children. No callbacks, no state.
- Consumers: `App.svelte:18` is the sole import; render sites are `App.svelte:302` (`<Skeleton testid="task-skeleton" rows={6} />`) and `App.svelte:331` (`<Skeleton testid="dashboard-skeleton" rows={4} />`).
- Pattern to reuse: `App.svelte:251` already hosts `ConnectionStatus` via `<ReactIsland component={...} props={{...}} />`; `mountIsland.tsx` already uses `flushSync`, so island children commit synchronously and the App tests' synchronous DOM queries work.
- Test contract: `App.test.ts:347-357` asserts only `[data-testid='dashboard-skeleton']` and `[data-testid='task-skeleton']` exist while projections load; assertions must keep passing unchanged (34/34).
- CSS: the component `<style>` block in `Skeleton.svelte` (`.skeleton`, `.skeleton-row`, `@keyframes skeleton-sweep`, and its `@media (prefers-reduced-motion: reduce)` override) uses only existing variables (`--space-3/4`, `--radius-lg`, `--paper-tint`, `--paper-raised`, `--ease`). `styles.css` currently has zero `skeleton` rules; React strips Svelte scoping, so the rules must move verbatim into `styles.css` in the same slice.
- RED baseline: no Skeleton test exists today; a new `Skeleton.test.tsx` importing `./Skeleton` fails with a missing-module error until `Skeleton.tsx` is created.

## Code anchors

- `Skeleton.svelte` markup to preserve exactly in rendered DOM: root `div.skeleton[aria-hidden="true"]` with `data-testid` only when `testid` is provided, containing exactly `rows` `div.skeleton-row` children with no text.
- `Skeleton.tsx`: one plain exported-default function component, props interface `{ rows?: number; testid?: string }`, default `rows = 4` in parameter destructuring — mirror `ConnectionStatus.tsx` in the same directory for style.
- `App.svelte` import anchor: `import Skeleton from "./Skeleton.svelte";` at line 18 → `import Skeleton from "./Skeleton";`. `ReactIsland` is already imported at line 11.
- `App.svelte` render anchors: replace `<Skeleton testid="task-skeleton" rows={6} />` (line 302) with `<ReactIsland component={Skeleton} props={{ testid: "task-skeleton", rows: 6 }} />` and `<Skeleton testid="dashboard-skeleton" rows={4} />` (line 331) with `<ReactIsland component={Skeleton} props={{ testid: "dashboard-skeleton", rows: 4 }} />`. Change nothing else in either `{:else}` branch.
- `styles.css` anchor: insert one new section immediately before the line `/* NARROW PHONES (shared chrome) ------------------------------------------- */`, containing the four rule blocks copied character-for-character from the `Skeleton.svelte` `<style>` element (a `/* SKELETON ... */` header comment matching the file's existing section style is allowed).

## Test-first instructions

1. Create only `Skeleton.test.tsx` first, while `Skeleton.svelte` and `App.svelte` remain untouched. Use `@testing-library/react` like `ConnectionStatus.test.tsx`.
2. Cover at minimum: default render produces 4 `.skeleton-row` children and no `data-testid` attribute; `rows={6}` produces exactly 6 rows; `testid="task-skeleton"` appears as `data-testid` on the `.skeleton` root; the root has `aria-hidden="true"`.
3. Run exactly this focused command before any production edit:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/Skeleton.test.tsx
```

4. It must exit nonzero because `Skeleton.tsx` does not exist. Preserve an output excerpt showing that missing-module failure.

## Edit instructions

1. Create `Skeleton.tsx` rendering the exact DOM in Code anchors: `Array.from({ length: rows })` mapped to `<div className="skeleton-row" key={index} />` inside `<div className="skeleton" data-testid={testid} aria-hidden="true">`. Omitting `data-testid` when `testid` is undefined is React's default behavior; do not add conditional logic for it.
2. Rerun the exact RED command and record GREEN with all tests passing.
3. In `App.svelte`, change only the import (line 18) and the two render sites (lines 302 and 331) as specified in Code anchors.
4. Copy the `<style>` rules from `Skeleton.svelte` into `styles.css` at the specified anchor, unchanged except for removing the Svelte `<style>` wrapper.
5. Delete `Skeleton.svelte` only after the React suite is green.
6. Do not format or rewrite unrelated App or CSS lines.

## Verification commands

Run in order from the repository root:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/Skeleton.test.tsx
npm run web:test -- --run crates/ajax-web/web/src/components/App.test.ts
npm run web:test -- --run crates/ajax-web/web/src/design-colors.test.ts
npm run web:check
```

## Acceptance criteria

- The focused command records a nonzero missing-module RED before production edits and full GREEN afterward.
- App renders both skeletons through `ReactIsland` with identical `testid`/`rows` values; both `data-testid` values are unchanged.
- Existing App tests pass 34/34 without assertion changes; design-colors test passes (no new color literals).
- All skeleton CSS rules exist once in `styles.css`, byte-identical to the Svelte `<style>` content apart from the removed wrapper and indentation normalization.
- `Skeleton.svelte` is deleted; `rg -l "Skeleton.svelte" crates/ajax-web/web/src` finds nothing.
- No dependency, config, seam-file, e2e, or generated-artifact changes.
- Non-deletion patch stays below roughly 120 changed lines.

## Stop conditions

- The new test unexpectedly passes before `Skeleton.tsx` exists or fails for an unrelated baseline reason.
- Any file outside Allowed files appears to need editing (including any seam file or another component).
- Existing App tests fail outside the two skeleton render sites.
- The design-colors test fails because of the copied CSS.
- The patch exceeds Allowed files or roughly 120 non-deletion changed lines.
