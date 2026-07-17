PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Mechanically port `TaskTerminal.svelte` (~1600 lines) to `TaskTerminal.tsx` with behavior parity. Island-swap the sole consumer in `TaskDetail.svelte`. Move component CSS into `styles.css`. Keep frozen terminal modules byte-identical in behavior. **Definition of done for this packet:** focused unit/source contracts green **and** `e2e/terminal-behavior.test.ts` on `mobile-webkit` green **unmodified**.

## Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.tsx` (new)
- `crates/ajax-web/web/src/components/TaskTerminal.test.tsx` (new; port from `.test.ts`)
- `crates/ajax-web/web/src/components/TaskTerminal.svelte` (delete)
- `crates/ajax-web/web/src/components/TaskTerminal.test.ts` (delete)
- `crates/ajax-web/web/src/components/TaskDetail.svelte` (import + ReactIsland swap only)
- `crates/ajax-web/web/src/styles.css` (append TaskTerminal CSS; convert `:global(X)` → `X`)
- `crates/ajax-web/web/src/components/keyboardBandPin.test.ts` (repoint TaskTerminal raw import)
- `crates/ajax-web/web/src/components/App.test.ts` (repoint TaskTerminal raw import only if present)
- `.planning/agent-plans/react-slice-s5.md` (checklist only)

## Forbidden changes

- `viewport.ts`, `terminalConnection.ts`, `terminalGeometry.ts`, `terminalRefit.ts`, `api.ts`
- Enabling React StrictMode anywhere
- Weakening or editing `e2e/terminal-behavior.test.ts` assertions
- Redesign, shadcn, smoothScroll, scroll-behavior changes
- Commit / push / branch changes

## Context evidence

- Consumer: `TaskDetail.svelte` line ~72 `<TaskTerminal handle={detail.qualified_handle} />`.
- Props: `{ handle: string }` only.
- Lifecycle: `onMount` ~410 builds xterm + `connectTaskTerminal` + refit controller; cleanup on destroy. React: one `useEffect` on `[handle]` that tears down and rebuilds on handle change (navigation).
- Expand path must stay sync for `beginExpandFlush` / band settle — mirror Svelte imperative calls; use `flushSync` only if island commit timing breaks expand e2e.
- CSS: `<style>` starts ~1234; many `:global(textarea.xterm-helper-textarea)` rules — flatten into `styles.css`.
- Source contracts: `TaskTerminal.test.ts` greps settle/expand/CSS from `TaskTerminal.svelte?raw` — after port, grep `TaskTerminal.tsx?raw` and/or `styles.css`.
- E2E: `npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts` must pass unchanged.
- Prior island pattern: `ReactIsland` + `flushSync` in `mountIsland.tsx` already used for sync DOM.

## Code anchors

- Keep `@xterm/xterm` + FitAddon + xterm CSS import.
- Preserve all `data-testid`s: `task-terminal-panel`, `terminal-interaction-surface`, `terminal-status`, `terminal-copy-overlay`, keys toolbar, paste fallback, etc.
- `html.terminal-expanded` class toggling unchanged.
- Inert chrome while expanded unchanged.
- Do not change `connectTaskTerminal` call shape.

## Test-first instructions

1. Add/update `TaskTerminal.test.tsx` pointing at missing `./TaskTerminal` / new CSS locations → RED.
2. ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/TaskTerminal.test.tsx
   ```
3. Implement port; update keyboardBandPin/App.test raw paths; delete svelte.
4. Green unit, then:
   ```bash
   npm run web:check
   npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
   ```

## Edit instructions

1. Mechanical 1:1 port: `$state`→`useState`, `$derived`→inline/useMemo sparingly, `$effect`/`onMount`→`useEffect`, `bind:this`→`useRef`.
2. Prefer refs for term/connection/controllers that must not retrigger effects.
3. Move CSS to `styles.css` carefully (drop `:global()`).
4. TaskDetail: `import ReactIsland` + `import TaskTerminal from "./TaskTerminal"` + island with `{ handle: detail.qualified_handle }`.
5. Grep `TaskTerminal.svelte` empty under `src/` after delete.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/components/TaskTerminal.test.tsx crates/ajax-web/web/src/components/keyboardBandPin.test.ts crates/ajax-web/web/src/components/App.test.ts
npm run web:check
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

## Acceptance criteria

- Source-contract unit tests green against React/CSS.
- `terminal-behavior` mobile-webkit suite green with zero assertion edits.
- Frozen modules untouched.
- No StrictMode.
- Diff limited to allowed files.

## Stop conditions

- terminal-behavior fails after two honest fix attempts → STOP and report.
- Need to edit viewport/terminalRefit/geometry/connection.
- Diff escapes allowed files.
