PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port `ConnectionStatus` from Svelte to a React component hosted by the existing one-way `ReactIsland`, preserving its DOM, labels, links, callbacks, accessibility, styling hooks, and App behavior exactly.

## Allowed files

Create:

- `crates/ajax-web/web/src/components/ConnectionStatus.tsx`
- `crates/ajax-web/web/src/components/ConnectionStatus.test.tsx`

Edit:

- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/react/mountIsland.tsx`

Delete after the React test is green:

- `crates/ajax-web/web/src/components/ConnectionStatus.svelte`
- `crates/ajax-web/web/src/components/ConnectionStatus.test.ts`

## Forbidden changes

- Do not edit `ReactIsland.svelte`, `ErrorBoundary.tsx`, the island test, CSS, package files, Vite/TypeScript config, fixtures, e2e tests, Rust, generated `dist/`, or any other component. `mountIsland.tsx` is allowed only for the synchronous shared-render correction described below.
- Do not alter connection state derivation, polling, retry/reload/navigation callbacks, health URL, visible strings, class names, `data-state`, `aria-label`, target/rel attributes, or button/link order.
- Do not add shadcn, Tailwind classes, wrappers, hooks, context, state, memoization, dependencies, or abstractions.
- Do not weaken, omit, merge, or delete an assertion from the five existing tests; syntax may change only where React uses `className` and RTL uses JSX props directly.
- Do not commit, push, merge, rebase, create/switch branches, run a production build, or use `git checkout`/`git restore`.

## Context evidence

- `architecture.md` makes Web Cockpit a presentation adapter over backend/core projections. This port renders the same `ConnectionState`; it must not own or reinterpret connection truth.
- `docs/react-migration-plan.md` S1 requires an assertion-for-assertion RTL port, one-way React-inside-Svelte hosting, same-slice Svelte deletion, and unmodified connection-action e2e behavior.
- Baseline on 2026-07-17: `npm run web:test -- --run crates/ajax-web/web/src/components/ConnectionStatus.test.ts` passes 5/5 before the port.
- Direct source/caller inspection: `App.svelte` is the sole production importer and consumer. CSS selectors and two Playwright tests address stable DOM/class/text behavior rather than framework internals.
- Parent review exposed the first real-consumer RED after the partial port: `npm run web:test -- --run crates/ajax-web/web/src/components/App.test.ts` fails `renders the shared chrome` because `.connection-status` is null immediately after Svelte mount. The React component's own 5/5 suite passes, proving the defect is the island commit boundary rather than component markup.
- Graphify: no repository graph exists; the required browser presentation boundary was reconstructed from `architecture.md` and verified source reads.
- Serena: unavailable; `rg` found every `ConnectionStatus` import, render, style selector, unit test, and e2e locator.
- ast-grep is not useful for Svelte markup; direct line-stable anchors below cover the only import and render site. No repeated structural rewrite is involved.

## Code anchors

- `ConnectionStatus.svelte` defines props `state`, optional `detail`, default `healthHref = "/api/health"`, and optional `onRetry`, `onReload`, `onCopyDiagnostics`; its derived label is `detail ? `${state}: ${detail}` : state`.
- Preserve the root markup exactly in rendered DOM: `.connection-status[data-state]`, `.connection-label`, `.connection-actions[aria-label="Connection actions"]`, Retry as the sole `.is-primary`, then Reload, Copy Diagnostics, and Open Health URL.
- `ConnectionStatus.test.ts` contains five tests: state/data attribute, detail label, sole primary action plus source strings, callback cardinality, and default health link. Port every assertion; use `@testing-library/react` and JSX.
- `App.svelte` import anchors are `import ConnectionStatus from "./ConnectionStatus.svelte";` and `import Skeleton from "./Skeleton.svelte";`. Add `ReactIsland` from `../react/ReactIsland.svelte`, import the React component from `./ConnectionStatus`, and leave Skeleton untouched.
- `App.svelte` render anchor is the single `<ConnectionStatus ... />` in the header. Replace only that node with `<ReactIsland component={ConnectionStatus} props={{ ... }} />`, passing the same five values/callbacks without renaming or moving their logic.
- Existing CSS in `styles.css` owns all visuals. `ConnectionStatus.tsx` must use the same DOM classes and add no style.
- `mountIsland.tsx` currently calls `root.render(...)` directly inside its private `render()` helper. React schedules that commit, while the established Svelte App test contract observes children synchronously after mount. Wrap only that existing root render in React DOM's `flushSync`; do not add timers, promises, consumer waits, or per-component workarounds.

## Test-first instructions

1. Create only `ConnectionStatus.test.tsx` first, while the old Svelte test/component remain untouched.
2. Port the existing five tests and every assertion to `@testing-library/react`. Import `ConnectionStatus` from `./ConnectionStatus`; import its raw TSX source only for the existing source-contract assertions and translate Svelte `class` spelling to React `className` without weakening intent.
3. Run exactly this focused command before any production edit:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/ConnectionStatus.test.tsx
```

4. It must exit nonzero because `ConnectionStatus.tsx` does not exist. Preserve an output excerpt showing that intended missing-module failure.

## Edit instructions

1. Create `ConnectionStatus.tsx` as one plain exported-default function component with a small props interface matching the Svelte props. Default `detail` to null and `healthHref` to `/api/health` in parameter destructuring.
2. Render the exact DOM described in Code anchors. Call optional callbacks directly from button `onClick` handlers. No local state/effect/ref/hook is needed.
3. Run the exact RED command unchanged and record GREEN with all five tests passing.
4. In `App.svelte`, replace only the import and the one header render site as described. Pass a plain props object containing current state/detail and the identical retry/reload/diagnostics callbacks.
5. Delete `ConnectionStatus.svelte` and `ConnectionStatus.test.ts` only after the React suite is green.
6. Before editing the seam, run the App command from Verification and confirm the existing `.connection-status` assertion fails because the island has not committed.
7. In `mountIsland.tsx`, import `flushSync` from `react-dom` and use it around the existing `root.render(...)` call so initial mount and prop updates retain the island's synchronous contract. Change no other seam behavior.
8. Rerun the App command and require 34/34 GREEN.
9. Do not format or rewrite unrelated App code.

## Verification commands

Run in order from the repository root:

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/ConnectionStatus.test.tsx
npm run web:test -- --run crates/ajax-web/web/src/components/App.test.ts
npm run web:check
```

## Acceptance criteria

- The exact focused command records a nonzero missing-module RED before production edits and 5/5 GREEN afterward.
- Every original test assertion is represented with the same behavioral strength.
- App renders the React component through `ReactIsland` and retains identical callbacks/props.
- Existing App tests pass 34/34 without assertion changes; their observed DOM remains synchronous.
- Old Svelte component and test are deleted; no dual implementation remains.
- No CSS, e2e, dependency, frozen module, generated artifact, or unrelated App line changes.
- Non-deletion patch stays below roughly 180 changed lines.

## Stop conditions

- The new test unexpectedly passes before `ConnectionStatus.tsx` exists or fails for an unrelated baseline reason.
- Any original assertion cannot be preserved, App callbacks would need semantic changes, or any seam file other than the single `mountIsland.tsx` render call must change.
- A CSS/e2e/Rust/dependency/config/generated-file edit appears necessary.
- Existing App tests fail outside the migrated connection-status surface.
- The patch exceeds Allowed files or roughly 180 non-deletion changed lines.
