# Fix xterm Surface V2 visual spam + dual background

## Scope

Fix experimental `XtermTerminalView` so the screen uses one Ghostty-matched
theme background and ResizeObserver does not spam PTY resizes (which redraws
and looks like duplicated text).

## Non-goals

- Porting Ghostty zero-lag / gestures / CSS scale
- Changing Ghostty `TerminalRawView`
- Changing Surface V2 flag / Dev settings
- Commit / push

## Delegation decision

`Delegation decision: delegated via model-router` — frontend visual/layout fix
→ `cursor-delegate` / `composer-2.5`.

## Task checklist

- [x] Failing tests: theme options; resize only sent when cols/rows change; debounced RO
- [x] Apply theme + host CSS background `#1c1714` / foreground `#f4eee0`
- [x] Debounce ResizeObserver; skip `sendResize` when dimensions unchanged
- [x] Parent validate focused vitest + web:check

## Validation

```bash
cd crates/ajax-web/web && npx vitest run src/components/XtermTerminalView.test.ts
npm run web:check
```

## Results

### RED (before implementation)

```text
FAIL constructs Terminal with Ghostty-matching theme — theme undefined
FAIL does not spam sendResize — sendResize called 2 times (expected 1)
9 tests | 2 failed | 7 passed
```

### GREEN (after implementation)

```text
9 tests passed (9)
```

### web:check

```text
svelte-check found 0 errors and 0 warnings
exit 0
```

### Deviations

- Resize spam test simulates a cols change (`80 → 100`) before double RO so
  debounced `reportResize` has new dims to send; unchanged-dims path is covered
  by `lastSentCols`/`lastSentRows` guard in implementation.
