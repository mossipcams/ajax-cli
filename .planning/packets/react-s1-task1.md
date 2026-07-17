PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Establish the S1 React-in-Svelte coexistence seam: React components can be mounted into one Svelte-owned host, re-rendered with new props, safely unmounted, and contained by one visible error boundary. Add only the compatible React/Tailwind test/build configuration needed for later S1 ports; do not migrate a user-facing component yet.

## Allowed files

New test:

- `crates/ajax-web/web/src/react/mountIsland.test.tsx`

New production/configuration:

- `crates/ajax-web/web/src/react/mountIsland.tsx`
- `crates/ajax-web/web/src/react/ReactIsland.svelte`
- `crates/ajax-web/web/src/react/ErrorBoundary.tsx`
- `crates/ajax-web/web/components.json`

Existing configuration/dependency files:

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/vite.config.mts`
- `crates/ajax-web/web/tsconfig.json`
- `crates/ajax-web/web/tsconfig.check.json`

## Forbidden changes

- Do not edit `App.svelte`, any existing component, `styles.css`, any e2e test, Rust source, generated `dist/`, or a framework-neutral module.
- Do not add a service worker, PWA manifest, router/state/form library, shadcn component, CSS reset, Tailwind preflight, code splitting, React StrictMode, or Vite major upgrade.
- Do not add `clsx`, `tailwind-merge`, `class-variance-authority`, Radix, icon, animation, or other later-slice dependencies.
- Do not weaken, delete, or rewrite any existing test assertion.
- Do not commit, push, merge, rebase, create/switch branches, or touch files outside Allowed files.

## Context evidence

- `architecture.md` defines `ajax-web` as a presentation adapter over core projections and forbids browser-owned task truth. This task adds presentation mounting only and must not touch data or lifecycle controllers.
- `docs/react-migration-plan.md` D1/D5/D7 and S1 require React 19 SPA coexistence, one-way React-inside-Svelte islands, the deterministic single-bundle contract, Tailwind v4 without preflight, and no generated shadcn components in S1.
- Graphify: unavailable for this checkout (`graphify-out/graph.json` does not exist); architecture boundaries were reconstructed from the required architecture document and verified source reads.
- Serena: unavailable in this environment; exact definitions, imports, call sites, tests, and configuration were inspected directly with `rg`/`sed`.
- ast-grep: `export default defineConfig($CONFIG)` matches the single Vite config root; `mount($COMPONENT, { target: $TARGET })` matches the current Svelte root mount in `src/main.ts`. Task 1 must not edit `main.ts`.
- Registry compatibility check on 2026-07-17: React/ReactDOM 19.2.7, RTL 16.3.2, Tailwind and `@tailwindcss/vite` 4.3.3 are current stable; `@vitejs/plugin-react` 5.2.0 is the newest release compatible with existing Vite 6. The latest plugin-react 6.0.3 requires Vite 8 and is forbidden here.

## Code anchors

- `crates/ajax-web/web/vite.config.mts`: imports at lines 1–5; `plugins: [svelte(), svelteTesting(), renameAppHtml()]`; deterministic `entryFileNames: "app.js"` and CSS asset naming; test include `src/**/*.test.ts`.
- `crates/ajax-web/web/tsconfig.json`: add `jsx: "react-jsx"`, `baseUrl: "."`, `paths: { "@/*": ["./src/*"] }`, and `.tsx` to `include`; preserve every strictness flag.
- `crates/ajax-web/web/tsconfig.check.json`: exclude both `src/**/*.test.ts` and `src/**/*.test.tsx`; preserve `e2e` exclusion.
- `package.json`: preserve all scripts. Add runtime `react`/`react-dom`; add only the dev packages named under Edit instructions.
- `crates/ajax-web/web/src/contracts.ts`: the established visible phrase is `Incompatible server response`; do not import or modify this frozen module. The error boundary fallback must contain that phrase.
- The new seam API must stay concrete and tiny: one `mountIsland(target, Component, props)` function returning only `update(nextProps)` and `unmount()`; `ReactIsland.svelte` owns one host `<div>`, mounts once, forwards prop changes, and unmounts on destroy.

## Test-first instructions

1. Create only `crates/ajax-web/web/src/react/mountIsland.test.tsx` first.
2. Use React `createElement` plus `act`; no JSX, RTL matcher, fixture, snapshot, or test helper is needed.
3. One focused test may cover the lifecycle: mount text from initial props, call `update` and observe new text, call `unmount` and observe an empty host.
4. A second focused test renders a component that throws and asserts the host contains `Incompatible server response` rather than escaping the island.
5. Before any production/config/dependency edit, run exactly:

```bash
npm exec -- vitest run crates/ajax-web/web/src/react/mountIsland.test.tsx --environment jsdom
```

The command must exit nonzero because the new production seam/dependencies do not exist. Preserve the output excerpt proving that intended missing seam/dependency failure.

## Edit instructions

1. Install exact compatible versions and let npm update the lockfile:
   - runtime: `react@19.2.7`, `react-dom@19.2.7`
   - dev: `@types/react@19.2.17`, `@types/react-dom@19.2.3`, `@testing-library/react@16.3.2`, `@testing-library/dom@10.4.1`, `@vitejs/plugin-react@5.2.0`, `tailwindcss@4.3.3`, `@tailwindcss/vite@4.3.3`
   Use exact versions; do not change existing dependency versions or scripts.
2. `ErrorBoundary.tsx`: a conventional minimal React class error boundary. On error, render a small `role="alert"` fallback containing `Incompatible server response`; otherwise render `children`. No logging service, reset state machine, hook wrapper, or dependency.
3. `mountIsland.tsx`: use `createRoot` and `createElement`. Render the supplied component inside `ErrorBoundary`; return concrete `update(nextProps)` and `unmount()` methods. No StrictMode, context, portal, registry, or generic factory layer.
4. `ReactIsland.svelte`: accept a React `component` and plain `props`; bind one host `<div>`. Use `onMount` to create the island exactly once and clean it up; use one `$effect` only to forward new props via `update`. Do not copy props into Svelte state or remount the React root on every prop change.
5. `vite.config.mts`: add the React and Tailwind Vite plugins while retaining Svelte coexistence and all deterministic output. Add the `@` alias to `src`. Widen only the Vitest include to `src/**/*.test.{ts,tsx}`.
6. `tsconfig.json`/`tsconfig.check.json`: make the exact JSX/include/alias/exclusion changes named in Code anchors; preserve strict options.
7. `components.json`: create the current shadcn configuration in the web project root with `style: "new-york"`, `rsc: false`, `tsx: true`, Tailwind v4 blank config, CSS `src/styles.css`, neutral base, CSS variables, no prefix, and `@/components`, `@/components/ui`, `@/lib`, `@/hooks`, `@/lib/utils` aliases. Configure only; generate nothing.
8. Re-run the exact RED command unchanged for GREEN.

## Verification commands

Run in order from the repository root:

```bash
npm exec -- vitest run crates/ajax-web/web/src/react/mountIsland.test.tsx --environment jsdom
npm run web:test -- --run crates/ajax-web/web/src/react/mountIsland.test.tsx
npm run web:check
npm run web:build
npm run web:build:check
```

## Acceptance criteria

- The exact focused command has one recorded nonzero RED before any production/config/dependency edit and exits zero after implementation.
- Mount, prop update, unmount cleanup, and error containment are executable tests.
- Svelte and React plugins coexist; `.tsx` is typechecked and collected by Vitest.
- Existing Vite 6 remains unchanged and the build still emits exactly the Rust embed contract files.
- `components.json` is configuration only; no UI component or helper is generated.
- Only Allowed files change and the patch remains below roughly 400 changed lines excluding `package-lock.json`.

## Stop conditions

- The RED command unexpectedly passes or does not execute the new test.
- Vite 6 cannot accept the specified plugin versions without an unrelated dependency upgrade.
- The seam needs `App.svelte`, `styles.css`, a frozen module, Rust, e2e, or any file outside Allowed files.
- The build emits another JavaScript chunk, hashed names, Tailwind preflight, or the string `serviceWorker`.
- Svelte check cannot type the concrete island props without broad weakening (`any` is acceptable only at the Svelte/React component-type boundary, not for props data).
- Any unrelated baseline failure appears, or the non-lockfile patch would exceed roughly 400 changed lines.
