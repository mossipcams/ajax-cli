PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Port the new-task sheet journey to React as **bespoke** UI (no shadcn/Radix): `FullscreenLayer.tsx`, `useSheetDrag`, `NewTaskSheet.tsx`. Island-swap App. Move CSS into `styles.css`. Delete Svelte `NewTaskSheet`, `FullscreenLayer`, and `sheetDragAction` (+tests). Keep pure `sheetDrag.ts`. Update source-contract tests that imported `FullscreenLayer.svelte?raw` / `NewTaskSheet.svelte?raw` to use `styles.css` / React sources as appropriate.

## Allowed files

- `crates/ajax-web/web/src/react/useSheetDrag.ts` (new)
- `crates/ajax-web/web/src/react/useSheetDrag.test.ts` or `.test.tsx` (new)
- `crates/ajax-web/web/src/components/FullscreenLayer.tsx` (new)
- `crates/ajax-web/web/src/components/NewTaskSheet.tsx` (new)
- `crates/ajax-web/web/src/components/NewTaskSheet.test.tsx` (new)
- `crates/ajax-web/web/src/components/NewTaskSheet.svelte` (delete)
- `crates/ajax-web/web/src/components/NewTaskSheet.test.ts` (delete)
- `crates/ajax-web/web/src/components/FullscreenLayer.svelte` (delete)
- `crates/ajax-web/web/src/gestures/sheetDragAction.ts` (delete)
- `crates/ajax-web/web/src/gestures/sheetDragAction.test.ts` (delete)
- `crates/ajax-web/web/src/components/App.svelte` (NewTaskSheet → ReactIsland only)
- `crates/ajax-web/web/src/styles.css` (append FullscreenLayer + NewTaskSheet CSS verbatim)
- `crates/ajax-web/web/src/components/keyboardBandPin.test.ts` (repoint FullscreenLayer CSS source to `styles.css`)
- `.planning/agent-plans/react-slice-s4.md` (checklist only)

## Forbidden changes

- No `viewport.ts` edits
- No new npm dependencies / shadcn components
- No terminal / TaskDetail / settings edits beyond App island swap
- No commit/push/branch changes

## Context evidence

- `FullscreenLayer.svelte`: fixed band pin via `--app-top` / `--app-height`; `data-testid="fullscreen-layer"`.
- `sheetDragAction.ts`: passive `touchstart`/`touchmove`; `touchend`/`cancel`; uses `sheetDrag.ts`.
- `NewTaskSheet.svelte`: form, agent radiogroup, prefs localStorage, `startTask`, Escape/backdrop close, grabber drag-dismiss.
- App ~375: `<NewTaskSheet repos=… selectedProject=… onClose=… onCockpit=… onResult=… onOpenTask=… />`.
- Tests: `NewTaskSheet.test.ts` (focus, submit, drag dismiss, prefs, CSS contracts); `sheetDragAction.test.ts`; `keyboardBandPin` fullscreen rule; e2e `actions` cancel/start + `layout-scroll` sheet band.

## Code anchors

- `FullscreenLayer({ children, zIndex=50 })` — ReactNode children.
- `useSheetDrag(ref, { onDismiss, onOffset })` — mirror swipeReveal hook pattern / sheetDragAction flags.
- `NewTaskSheet` props identical to Svelte; compose FullscreenLayer; focus dialog on mount (`useEffect` + ref).
- CSS: move both `<style>` blocks into `styles.css` unchanged selectors.
- Source contracts: assert `.fullscreen-layer` / `.sheet-card` rules from `styles.css`; drop `FullscreenLayer` string check against svelte or assert React import of FullscreenLayer.

## Test-first instructions

1. Add failing `useSheetDrag` tests + `NewTaskSheet.test.tsx` port while implementations missing → RED.
2. ```bash
   npm run web:test -- --run crates/ajax-web/web/src/react/useSheetDrag.test.ts crates/ajax-web/web/src/components/NewTaskSheet.test.tsx
   ```
   (adjust extension if `.test.tsx`)
3. Implement to green; update keyboardBandPin; App swap; delete svelte/action; then:
   ```bash
   npm run web:check
   npm run web:smoke -- crates/ajax-web/web/e2e/actions.test.ts crates/ajax-web/web/e2e/layout-scroll.test.ts
   ```

## Edit instructions

1. Implement hook from `sheetDragAction.ts` using `sheetDrag.ts`.
2. Port FullscreenLayer + NewTaskSheet mechanically; no redesign; font-size 16px on inputs stays.
3. Append CSS to `styles.css`; update keyboardBandPin + NewTaskSheet source assertions.
4. App ReactIsland swap; delete Svelte + sheetDragAction; grep `NewTaskSheet.svelte` / `FullscreenLayer.svelte` / `sheetDragAction` empty under `src/`.

## Verification commands

```bash
npm run web:test -- --run crates/ajax-web/web/src/react/useSheetDrag.test.ts crates/ajax-web/web/src/react/useSheetDrag.test.tsx crates/ajax-web/web/src/components/NewTaskSheet.test.tsx crates/ajax-web/web/src/components/keyboardBandPin.test.ts crates/ajax-web/web/src/components/App.test.ts
npm run web:check
npm run web:smoke -- crates/ajax-web/web/e2e/actions.test.ts crates/ajax-web/web/e2e/layout-scroll.test.ts
```

## Acceptance criteria

- Unit + keyboardBandPin + App green.
- New-task cancel/start e2e + layout-scroll sheet band e2e green.
- No shadcn; no svelte sheet/layer/action left in src.
- Diff limited to allowed files.

## Stop conditions

- Temptation to add Radix Sheet / change viewport.ts.
- e2e needs weakening.
- Scope grows into terminal.
