# Agent-sized scale-to-fit

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Phone Web PWA: Ghostty/PTY logical cols = `max(80, hostFitCols)`; CSS-scale
the terminal element to the host width so live and scrollback share an
agent-sized layout. Do not fit the PTY down to ~43 cols.

## 3. Allowed files

- `crates/ajax-web/web/src/terminalGeometry.ts`
- `crates/ajax-web/web/src/terminalGeometry.test.ts`
- `crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/src/terminalGestures.ts`
- `crates/ajax-web/web/src/terminalSelection.test.ts`
- `crates/ajax-web/web/src/terminalZeroLag.ts`
- `crates/ajax-web/web/src/terminalZeroLag.test.ts`
- `crates/ajax-web/web/e2e/terminal-scroll-garble.test.ts`
- `crates/ajax-web/web/TERMINAL.md`
- `architecture.md` (Web Cockpit fit-geometry paragraph only)
- `.planning/agent-plans/agent-sized-scale-to-fit.md`

## 4. Forbidden changes

- No wterm migration; no ghostty-web bump.
- No Wide key-bar toggle; no default-zoom horizontal pan.
- No `terminal_pty.rs` / capture-pane / attach-plan changes.
- No task-list / polling / state.ts changes.
- No commit/push/branch/rebase; avoid `web/dist` unless install snapshots force it.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — no `.planning/graphs` corpus; Web Cockpit terminal
  ownership is already documented in `architecture.md` § ajax-web terminal and
  `crates/ajax-web/web/TERMINAL.md` (fit/font/pan → terminalGeometry;
  Ghostty mount → TerminalRawView).
- Serena: `NOT_REQUIRED` — exact symbols located by rg/Read:
  `fitNow`, `colsFloor`, `hostFitCols`, `sendResize`, `cellHeightPx`,
  `selectionCellAt`, `measureZeroLagFromTerminalHost`.
- ast-grep: `NOT_REQUIRED` — single call site
  `term.resize(flooredCols(hostFitCols() ?? proposed.cols, colsFloor()), proposed.rows)`
  in TerminalRawView; no sibling fit implementations.
- Repro: e2e softwrap at cols=43 yields lines ending ` cra` / starting `tes/`.

## 6. Code anchors

Quoted from current tree:

- `terminalGeometry.ts`: `export const MIN_TERMINAL_COLS = 80;` /
  `export const FIT_TERMINAL_COLS = 40;` / `flooredCols` / `fitCapFontSize`
- `TerminalRawView.svelte` ~335: `const colsFloor = () => FIT_TERMINAL_COLS;`
- `TerminalRawView.svelte` `fitNow` ~605–659: font cap + 
  `term.resize(flooredCols(hostFitCols() ?? proposed.cols, colsFloor()), proposed.rows)`
- `TerminalRawView.svelte` `sendResize` ~537–543 → `resizeDedupe` →
  `connection.sendResize`
- `TerminalRawView.svelte` `selectionCellAt` ~415–426 uses
  `canvas.getBoundingClientRect()` + `cellAtPoint`
- `TerminalRawView.svelte` `cellHeightPx` ~323–328 (host canvas height / rows)
- `terminalGestures.ts` `wheelNotchesFromDrag` / `attachTerminalGestures`
  uses `host.cellHeightPx()`
- `terminalZeroLag.ts` `measureZeroLagFromTerminalHost` uses
  `canvas.clientWidth/Height` and renderer metrics
- Markup: `<div class="terminal-host ..." bind:this={container}>` then
  `term.open(container)` — **apply scale to `term.element`** after open/fit
  (do not transform `.terminal-host` itself; that is the overflow clip +
  gesture target)
- Tests to change: `floors fit mode at 40 columns`,
  `uses a fit proposal below 80 columns`,
  e2e `long soft-wrapped Claude-like paths show wrap column (hypothesis 3)`

## 7. Test-first instructions

Exact names / expected RED:

1. `terminalGeometry.test.ts`:
   - `logicalCols floors phone hostFit up to MIN_TERMINAL_COLS`
     expect `logicalCols(43) === 80`
   - `fitScale is below 1 when logical canvas is wider than host`
     expect `fitScale(390, 80, 9) < 1` and `fitScale(1200, 80, 9) === 1` (or ≤1)
2. `TerminalRawView.test.ts`:
   - Replace/repurpose `floors fit mode at 40 columns` to
     `uses agent-sized floor of 80 columns on a narrow host` — expect
     `resize` called with cols >= 80 and a scale style/transform applied to
     `term.element` (or documented scale attribute).
3. `terminal-scroll-garble.test.ts` softwrap case:
   - expect probe `cols >= 80`
   - expect softwrap sample does **not** match `/ cra$/` + `/^tes\//`
   - expect a recorded resize frame `cols >= 80`

RED:

```bash
rtk npm run web:test -- --run src/terminalGeometry.test.ts src/components/TerminalRawView.test.ts
rtk npx playwright test e2e/terminal-scroll-garble.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
```

## 8. Edit instructions

1. Add `logicalCols`, `fitScale`, `logicalRows` to `terminalGeometry.ts`; cover
   fuzz oracles. Stop using `FIT_TERMINAL_COLS` as `colsFloor` for live fit
   (`colsFloor` → `MIN_TERMINAL_COLS` or delete FIT usage).
2. In `fitNow`: compute `cols = logicalCols(hostFitCols() ?? proposed?.cols)`;
   keep readable font (may keep shrink-to-fit **only** when it does not pull
   logical cols below 80 — prefer fixed DEFAULT/chosen font + scale).
3. After resize, set `term.element.style.transformOrigin = "0 0"` and
   `transform = scale(${scale})` where
   `scale = fitScale(container.clientWidth, cols, cellWidth)`.
4. `sendResize` must send logical cols/rows (post-resize `term.cols/rows`).
5. `selectionCellAt`: map pointer through scale (divide offsets by scale, or
   use unscaled canvas metrics carefully so col/row match logical grid).
6. `cellHeightPx` for gestures: return **visual** line height
   `cellHeight * scale` when scale < 1.
7. Zero-lag overlay: multiply left/top by scale (or paint in host space).
8. When scale < 1, pinch must not reduce logical cols below 80.
9. Update architecture.md fit paragraph + TERMINAL.md; check ledger boxes.

## 9. Verification commands

```bash
rtk npm run web:test -- --run src/terminalGeometry.test.ts src/terminalGeometry.fuzz.test.ts src/components/TerminalRawView.test.ts src/terminalZeroLag.test.ts src/terminalSelection.test.ts
rtk npx playwright test e2e/terminal-scroll-garble.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
rtk npm run web:check
rtk git diff --check
```

## 10. Acceptance criteria

- Narrow host: logical cols ≥ 80, scale ∈ (0, 1), no cra/tes mid-wrap in e2e.
- Wide host: logical cols = hostFit (≥ 80), scale === 1.
- Resize WS frames use logical cols.
- Existing scroll-marker garble cases still pass.

## 11. Stop conditions

- Named anchors differ materially from this packet.
- Cannot produce the expected RED failures before edits.
- Diff > ~400 lines or outside Allowed files.
- Requires backend tmux width query.
- Keyboard+scale leaves cursor unusable with no in-scope fix.
