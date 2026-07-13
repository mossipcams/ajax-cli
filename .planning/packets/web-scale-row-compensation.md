# Packet: scale-compensated terminal rows

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

When agent-sized fit applies CSS `scale < 1` on a phone host, raise the logical
PTY row count so the *scaled* canvas still fills the host height. Eliminate the
blank band under a newly mounted Web Cockpit terminal (especially after new-task
creation). Keep column floor `max(80, hostFitCols)` and scale-to-fit width
behavior from #440/#442.

## 3. Allowed files

- `crates/ajax-web/web/src/terminalGeometry.ts`
- `crates/ajax-web/web/src/terminalGeometry.test.ts`
- `crates/ajax-web/web/src/terminalGeometry.fuzz.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/TERMINAL.md` (one-line ownership note only if needed)
- `.planning/agent-plans/web-drop-defect.md` (checklist only)

## 4. Forbidden changes

- No ghostty-web bump, wterm, Wide toggle, or PTY/tmux backend changes.
- No `web/dist` rebuild unless install/asset snapshots force it.
- No ajax-cli / registry / Drop changes (that is packet T2).
- No commit, push, branch, rebase, or merge.
- No drive-by CSS chrome / TaskDetail layout refactors beyond
  `.terminal-scale-layer` size if required for FitAddon measurement.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — Web Cockpit terminal ownership already in
  `architecture.md` § ajax-web terminal and `TERMINAL.md`; this is a local
  geometry fix inside that contract.
- Serena: `NOT_REQUIRED` — anchors located by rg/Read on current worktree.
- ast-grep: `NOT_REQUIRED` — single `fitNow` resize call site; no sibling
  fit implementations to rewrite.

## 6. Code anchors

- `terminalGeometry.ts`: `logicalCols`, `logicalRows`, `fitScale` (~99–135)
- `TerminalRawView.svelte` `fitNow` (~657–707):
  ```ts
  const cols = logicalCols(fitProposal);
  const rows = logicalRows(proposed.rows);
  term.resize(cols, rows);
  applyTerminalScale();
  ```
- `TerminalRawView.svelte` `applyTerminalScale` (~404–434) uses `fitScale`
- `.terminal-scale-layer` CSS (~1371–1378): absolute, no width/height today
- Test to update: `uses agent-sized floor of 80 columns on a narrow host`
  currently expects `{ cols: 80, rows: 30 }` with `terminalHostClientWidth = 390`
  and `terminalCellMetrics.width = 8` → `fitScale(390, 80, 8) = 0.609375`

## 7. Test-first instructions

1. `terminalGeometry.test.ts` — add:
   - `scaledLogicalRows raises host-fit rows when scale is below 1`
     - expect `scaledLogicalRows(30, 0.609375) === 50` (ceil(30/0.609375))
     - expect `scaledLogicalRows(30, 1) === 30`
     - expect `scaledLogicalRows(30, 0) === 30` (invalid scale → unscaled rows)
     - expect `scaledLogicalRows(undefined, 0.5) === 24` (logicalRows fallback)
2. `TerminalRawView.test.ts` — change
   `uses agent-sized floor of 80 columns on a narrow host` so after open:
   - `cols >= 80`
   - transform matches `/scale\(/`
   - resize frame rows are `scaledLogicalRows(30, fitScale(390, 80, 8))` (= 50),
     not 30

RED command:

```bash
rtk npm run web:test -- --run src/terminalGeometry.test.ts src/components/TerminalRawView.test.ts
```

Expected: new geometry assertion fails (helper missing) and/or narrow-host
rows still 30.

## 8. Edit instructions

1. Add `scaledLogicalRows(proposedRows, scale)` in `terminalGeometry.ts`:
   - Start from `logicalRows(proposedRows)`.
   - If `scale` is finite and `0 < scale < 1`, return
     `Math.max(1, Math.ceil(rows / scale))`.
   - Otherwise return `rows` unchanged.
2. In `fitNow`, after computing `cols = logicalCols(fitProposal)`:
   - Compute `scale = fitScale(container.clientWidth, cols, cellWidth)` when
     metrics are available (same inputs as `applyTerminalScale`), else `1`.
   - `rows = scaledLogicalRows(proposed.rows, scale)`.
   - `term.resize(cols, rows)` then `applyTerminalScale()`.
3. Give `.terminal-scale-layer` `width: 100%; height: 100%;` so FitAddon can
   measure the host-sized parent in production (open target is this layer).
4. Update any TerminalRawView tests that assert unscaled rows under an active
   scale (`terminalHostClientWidth` set + cols floored to 80) to expect
   compensated rows. Leave wide-host / scale=1 expectations unchanged.

## 9. Verification commands

```bash
rtk npm run web:test -- --run src/terminalGeometry.test.ts src/components/TerminalRawView.test.ts
rtk npm run web:check
```

Optional if time:

```bash
rtk npx playwright test e2e/terminal-scroll-garble.test.ts e2e/fullscreen-refit.test.ts \
  --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
```

## 10. Acceptance criteria

- Narrow-host agent-sized fit still sends cols ≥ 80 and applies CSS scale.
- Logical rows increase enough that `rows * scale ≈ host-fit rows` (ceil).
- Wide hosts (scale = 1) keep proposed rows unchanged.
- Focused vitest + web:check pass.
- Diff stays inside Allowed files.

## 11. Stop conditions

- FitAddon / Ghostty requires a different measurement model than
  `proposed.rows / scale`.
- Compensated rows break scroll-follow or expand e2e in a way that needs a
  second packet.
- Any edit outside Allowed files, or scope expands into Drop/registry.
