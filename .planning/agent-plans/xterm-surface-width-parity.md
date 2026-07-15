# Xterm Surface V2 — 80-col width parity + reconnect correctness

Matt runs with `ajax.terminal.surfaceV2` ON, so the terminal is
`XtermTerminalView.svelte` — none of the Ghostty-view client fixes from
PR #504/#506 apply to it. Three defects make history unusable there:

1. **Width mismatch (the garble):** `reportResize` sends
   `max(term.cols, 80)` to the PTY but leaves the local xterm at its fitted
   cols (~45 on a phone). Every 80-wide PTY line soft-wraps locally —
   garbled layout and broken history regardless of server-side seeding.
2. **Seeded reconnect never resets:** `onOpen` ignores its arguments; a
   manual Reconnect (full reseed per #506) appends a duplicate of history.
   (Automatic reconnects are already unseeded via shared
   terminalConnection.)
3. **Reconnect never resizes the new PTY:** `lastSentCols/Rows` dedupe is
   never cleared on open, so the fresh bridge PTY stays at its 80x24
   default unless the viewport happens to change afterwards.

## Design (spike-quality, minimal)

In `XtermTerminalView.svelte`:
- `reportResize`: when the fit proposal is below `MIN_TERMINAL_COLS`, lower
  the font to `fitCapFontSize(current, proposedCols, 80, 1, MAX)` (fit-to-
  width, floor to whole px), refit, then `term.resize(80, rows)` so the
  local grid matches the PTY floor exactly — lines wrap identically on
  both sides. Wide hosts unchanged.
- `onOpen(isReconnect, seeded)`: reset `lastSentCols/Rows = 0` always
  (new PTY must learn the size); `term.reset()` when
  `isReconnect && seeded`.

Depends on #506 (`onOpen` seeded flag; `seed=0` auto-reconnects) — branch
stacks on `ajax/terminal-history-seed`.

Non-goals: no policy-module parity build-out (another session appears to
own that), no pan/pinch for xterm, no Ghostty-view changes.

Delegation decision: delegated via model-router (cursor lane; frontend,
2 files, ~100 lines, opencode GLM lane remains unavailable per log).

## Checklist

- [x] Tests A-D (cursor-delegate composer-2.5, first round, red PROVEN:
      exit 1 with intended failures). Delegate deviation (accepted):
      proposeDimensions?.() optional call so TerminalSurfaceSelector's
      partial FitAddon mock cannot throw.
- [x] Impl as designed.
- [x] Validate (parent re-run): vitest 565/565, web:check clean, dist
      rebuilt, mobile-webkit 46/46, ajax-web 136/136, ajax-cli 335/335.
      GATE: ACCEPT.

## Deviations / validation

(recorded during execution)
