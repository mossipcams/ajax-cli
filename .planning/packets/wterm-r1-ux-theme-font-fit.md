# TDD Implementation Packet — R1 wterm UX (theme, font, forceFit)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
Fix Surface V2 brown wash, oversized font, and tap/layout glitches by (1) setting cooler wterm CSS theme vars including `--term-font-size: 13px` (`DEFAULT_FONT_SIZE`), (2) removing Ajax hardcoded `forceFitTerminal` 8×17 override so `@wterm/dom` `autoResize` + `_measureCharSize` own the grid, (3) converting the measured-cell-metrics `it.todo` into a passing test and dropping/updating the old force-fit assertion.

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (and app.css/app.js only if `npm run web:build` rewrites them — required if this repo vendors dist)

## Forbidden changes
- Do not touch Ghostty `TerminalRawView.svelte`
- Do not implement pinch, 80-col floor, keyboard lockstep, or fullscreen in this round
- Do not change `terminalWtermGhosttyCore.ts` (WASM cache is R2)
- Do not weaken unrelated tests
- Do not commit / push / change branches

## Context evidence
- Graphify: `NOT_REQUIRED` — single-component Surface V2 chrome; TERMINAL.md owns surface contract
- Serena: `NOT_REQUIRED` — anchors below are sufficient
- ast-grep: `NOT_REQUIRED` — exact symbols named below

## Code anchors
- `forceFitTerminal` uses `charWidth = 8`, `charHeight = 17` in `WtermTerminalView.svelte`
- `new WTerm(hostEl, { core, autoResize: true, ... })` already enables native measure
- CSS forces `#1c1714` on `.wterm-host` / `.term-grid` (warm brown); wterm default `--term-bg` is `#1e1e1e`, `--term-font-size: 14px`
- Ghostty Ajax theme font: `DEFAULT_FONT_SIZE = 13` in `terminalGeometry.ts`
- Yellow-smear fix must remain: `.term-grid { background: … !important; }` but use cooler opaque bg matching `--term-bg`
- Existing test `"force-fits the terminal after init…"` expects `termResize(40, 10)` from 320×170 / 8×17 — replace with anti-hardcode assertion
- Todo: `"fits the initial grid with wterm-measured cell metrics instead of the hardcoded 8x17 estimate"`

## Test-first instructions
1. In `WtermTerminalView.test.ts`, replace the force-fit test (or add alongside then delete old) so after mount with host 320×170, `termResize` is **not** called with the hardcoded estimate `40, 10` (and ideally not called from Ajax force-fit at all after init — mock starts at 72×24).
2. Convert the measured-cell-metrics `it.todo` into a real `it(...)` asserting the same: Ajax does not force-fit via 8×17 after init.
3. Add a test that the mounted `.wterm-host` (or `.wterm` on host) has computed/style `--term-font-size` of `13px` (or inline style / attribute you set).
4. Add a test that host/grid theme background is **not** `#1c1714` (warm paper) — expect cooler `#1e1e1e` (or whatever production sets; pick one and match).
5. RED command:
   `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`
   Must fail on the new assertions before production edit.

## Edit instructions
1. Delete `forceFitTerminal` and all call sites (init rAF + reconnect `onOpen`). Keep reconnect RIS `\x1bc` + `reportResize(liveTerm.cols, liveTerm.rows)`.
2. On the host element before/after WTerm construct, set CSS variables:
   - `--term-bg: #1e1e1e` (cooler dark; keeps opaque smear override)
   - `--term-fg: #d4d4d4` (or Ghostty `#f4eee0` — prefer cooler pairing with `#1e1e1e`)
   - `--term-font-size: 13px` using `DEFAULT_FONT_SIZE` from `../terminalGeometry`
   - `--term-color-0` aligned with `--term-bg` if needed for cell defaults
3. Update `<style>` rules: replace `#1c1714` backgrounds with `var(--term-bg, #1e1e1e)` or `#1e1e1e`; keep `.term-grid { background: … !important }` to beat inline smear.
4. Import `DEFAULT_FONT_SIZE` from `terminalGeometry`.
5. Remove the measured-cell-metrics `it.todo` once covered by the real test.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```
If dist is vendored, include rebuilt dist in the change set.

## Acceptance criteria
- No hardcoded 8×17 Ajax force-fit remains
- Font size CSS var is 13px
- Terminal chrome/grid bg is cooler `#1e1e1e`, not `#1c1714`
- Focused vitest file green; `TEST_FIRST` proven with RED then GREEN evidence
- Reconnect still clears with RIS and resends size

## Stop conditions
- Need to disable `autoResize` to make tests pass
- Touching Ghostty view or WASM loader
- Scope expands into pinch / 80-col / keyboard / fullscreen
