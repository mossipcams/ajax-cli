# Fix wterm blank terminal + yellow page

## Scope

Repair experimental Terminal Surface V2 so PTY text paints and init
failures are obvious (not a mysterious yellow screen).

## Non-goals

- Production migration off Ghostty
- Porting Ghostty scale-to-fit / zero-lag / private API work
- Changing Rust PTY / WS protocol

## Root cause (diagnosed)

1. **Col mismatch**: `reportResize` floored cols to `MIN_TERMINAL_COLS` (80)
   while wterm `autoResize` shrinks the local grid to the host (~35–40 on
   phone). PTY lays out for 80; local viewport is ~40 → truncated / looks
   empty. Ghostty avoids this by keeping an 80-col grid and CSS-scaling.
2. **Layout**: `WtermTerminalView` lacked Ghostty panel/host height rules.
3. **Yellow**: mustard init-failure banner replaced the whole terminal.

## Delegation decision

`Delegation decision: delegated via model-router` — frontend UI behavior →
`cursor-delegate` / `composer-2.5`.

## Task checklist

- [x] Failing tests first (resize no longer expects 80-col floor)
- [x] Fix `reportResize` to send real cols/rows (floor ≥1 only)
- [x] Align panel/host CSS for flex fill; override wterm demo chrome
- [x] Force-fit after `init()` via rAF
- [x] Selector root fills `.terminal-primary`
- [x] Error UI: left mustard border, not full wash
- [x] Parent validation: focused vitest + `web:check`

## Validation

```bash
npm run web:check   # exit 0
npm run web:test -- --run src/components/WtermTerminalView.test.ts src/components/TerminalSurfaceSelector.test.ts  # 14/14
```

## Deviations

- Force-fit uses fixed 8×17 char metrics (RO/autoResize corrects after);
  good enough for the spike without porting Ghostty measure helpers.
