# TDD Implementation Packet — R5 wterm pinch font (Surface V2 only)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 (`WtermTerminalView` only), grow/shrink `--term-font-size` on a
two-finger pinch using existing `pinchActivated` / `pinchFontSize` /
`persistFontSize` / `MAX_FONT_SIZE` / `MIN_FONT_SIZE` from `terminalGeometry`.
Do **not** attach full `attachTerminalGestures` (that steals scroll from wterm
native overflow). Convert the pinch `it.todo` into a real test.

## Hard gate
Experimental flag only: edit `WtermTerminalView*` (+ vendored dist). Never
`TerminalRawView` or shared gesture behavior that changes Ghostty.

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if `npm run web:build` updates them)

## Forbidden changes
- Do not call `attachTerminalGestures` (would hijack wterm native scroll)
- Do not edit `terminalGestures.ts`, `TerminalRawView.svelte`, or Ghostty path
- No 80-col floor / keyboard / fullscreen in this round
- No commit/push/branch

## Context evidence
- Graphify: `NOT_REQUIRED` — V2-only chrome
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- Geometry helpers: `pinchActivated`, `pinchFontSize`, `persistFontSize`, `MIN_FONT_SIZE`, `MAX_FONT_SIZE` in `terminalGeometry.ts`
- Ghostty reference tests: `"grows the font on a pinch spread, clamps it, and persists the choice"` in `TerminalRawView.test.ts` (~2037) — synthesize TouchEvents with 2 fingers
- Current todo: `"grows/shrinks the font on pinch with clamps, persisting the choice"`
- `applyWtermTheme` already sets initial `--term-font-size` from persisted/default

## Code anchors
- `WtermTerminalView.svelte` `onMount` init after `term = liveTerm` — attach pinch listeners on `hostEl`, detach on cleanup
- Keep current font in a module-local `let currentFontSize = …` updated by theme + pinch
- `PINCH_ACTIVATION_PX = 12` matches `terminalGestures.ts`

## Test-first instructions
1. Convert the pinch `it.todo` into a real test:
   - Mount WtermTerminalView
   - Dispatch `touchstart` with 2 touches distance D on `.wterm-host`
   - Dispatch `touchmove` with larger distance so activated + new size > start
   - Expect `--term-font-size` increased and clamped ≤ 20
   - Expect `localStorage ajax.terminal.fontSize` updated
   - Optionally a pinch-in shrink case or clamp-at-MAX
2. RED: `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`

## Edit instructions
1. Import `persistFontSize`, `pinchActivated`, `pinchFontSize`, `MIN_FONT_SIZE`, `MAX_FONT_SIZE` (and keep `persistedFontSize` / `DEFAULT_FONT_SIZE`).
2. Implement a small pinch-only handler on `hostEl` (touchstart/move/end/cancel):
   - track start distance + base font
   - on move: if activated, `next = pinchFontSize(...)`; set CSS var; `persistFontSize(next)`
   - do not `preventDefault` on single-finger moves (preserve wterm scroll); for 2-finger, `preventDefault` when engaged to block page zoom (match Ghostty intent)
3. Cleanup listeners on unmount.
4. Rebuild dist via `npm run web:build` in `crates/ajax-web/web`.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Pinch todo → passing test(s)
- Font CSS var updates + persists within 7–20
- Single-finger scroll path not replaced by Ajax scrollLines
- RED→GREEN proven; V2-only files touched

## Stop conditions
- Wiring `attachTerminalGestures`
- Editing Ghostty view or shared gesture module behavior
- Scope into 80-col / keyboard / fullscreen
