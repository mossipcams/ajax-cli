# TDD Implementation Packet — R3 wterm persisted font size

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 mount, apply `persistedFontSize()` from `terminalGeometry` to
`--term-font-size` when valid; ignore out-of-range / missing and fall back to
`DEFAULT_FONT_SIZE`. Convert the matching `it.todo` into a real test.

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates them)

## Forbidden changes
- No pinch gesture yet (next round)
- No 80-col floor, keyboard, fullscreen
- Do not change `terminalGeometry.ts` API
- No commit/push/branch

## Context evidence
- Graphify: `NOT_REQUIRED`
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- `persistedFontSize` / `DEFAULT_FONT_SIZE` / `MIN_FONT_SIZE` / `MAX_FONT_SIZE` in `terminalGeometry.ts`
- Ghostty tests in `TerminalRawView.test.ts` set `localStorage ajax.terminal.fontSize` and assert mount font
- Current `applyWtermTheme` always sets `${DEFAULT_FONT_SIZE}px`

## Code anchors
- `applyWtermTheme` in `WtermTerminalView.svelte`
- Todo: `"applies a persisted font size on mount and ignores out-of-range values"`

## Test-first instructions
1. Convert the todo into tests mirroring Ghostty:
   - `localStorage.setItem("ajax.terminal.fontSize", "16")` → host `--term-font-size` is `16px`
   - `"999"` or invalid → falls back to `13px`
2. RED: `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`

## Edit instructions
1. Import `persistedFontSize` alongside `DEFAULT_FONT_SIZE`.
2. In `applyWtermTheme`: `const size = persistedFontSize() ?? DEFAULT_FONT_SIZE` then set `--term-font-size`.
3. Rebuild dist if required by repo convention.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Persisted valid size applied; invalid ignored
- Todo removed / converted
- RED→GREEN proven

## Stop conditions
- Implementing pinch in this round
- Editing terminalGeometry storage keys
