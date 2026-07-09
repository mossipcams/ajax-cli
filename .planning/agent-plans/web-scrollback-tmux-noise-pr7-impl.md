# PR7 impl: mobile scrollback + ephemeral tmux noise

## Scope

- Mobile 2000 / desktop 10000 Ghostty scrollback via `terminalScrollbackLines()`
- Ephemeral-only tmux `set-option` for quieter status redraw

## Non-goals

- Shared session options, binary protocol, polling, disabling scrollback

## Delegation decision

`Delegation decision: not delegated because` this session is the Cursor CLI
implementation worker for a parent agent (already delegated).

## Checklist

- [x] Failing TS tests for constants + `terminalScrollbackLines()`
- [x] Failing Rust test for ephemeral `set-option` setup
- [x] Implement helpers + Terminal option + IsolatedAttachPlan setup
- [x] Verification commands pass

## Validation

```bash
npm run web:test -- crates/ajax-web/web/src/terminalGeometry.test.ts crates/ajax-web/web/src/components/TerminalRawView.test.ts --run
# PASS 166 tests

cargo test -p ajax-web isolated_attach -- --nocapture
cargo test -p ajax-web reaper_targets -- --nocapture
cargo test -p ajax-web filter_scrollback -- --nocapture
# (cargo accepts one filter; run separately)
```

## Deviations

- Packet verification listed multiple cargo filters in one invocation; cargo only
  accepts one TESTNAME, so filters were run separately.
- Used `ITerminalOptions.scrollback` (maps to WASM `scrollbackLimit` internally).
