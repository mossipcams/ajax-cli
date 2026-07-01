# Plan: Mobile Web Terminal Visibility

## Problem

On iPhone Safari the task terminal PTY is sized to the visible viewport:
~55 columns at the hardcoded 10px font. The hosted tmux/Claude Code TUI
assumes ~80 columns, so nearly every line wraps and the output is hard to
read. There is no way to trade columns for legibility (font size is fixed),
no way to reclaim the remaining screen chrome, and a rotated (landscape)
iPhone falls out of the `max-width: 767px` mobile breakpoint entirely —
losing the full-screen takeover and getting the 13px desktop font.

## Approach (user-selected)

1. **80-column PTY floor + pan/zoom** — stop sizing the PTY below 80 cols.
   The terminal canvas becomes wider than the phone screen; horizontal touch
   drags pan it, pinch adjusts the font size (persisted), so the operator
   chooses the visible window instead of suffering wrapped output.
2. **Expand toggle** — a key-bar button that collapses the remaining task
   chrome (header, status pill, action bar, details) on mobile and makes the
   panel fill the viewport on desktop.
3. **Other visibility fixes** — landscape iPhone gets the same full-screen
   takeover and mobile font treatment as portrait.
4. **Input-line corruption fix** — while the iOS keyboard is open the local
   xterm grid is shrunk (`fitAddon.fit()`) but the server resize is withheld,
   so the PTY/tmux keep addressing rows that no longer exist locally; xterm
   clamps those writes to its bottom row, making the TUI input box appear to
   move up and overwrite the line below it. Keep the local grid in lockstep
   with the PTY instead, and crop the view bottom-anchored while the keyboard
   is open.
5. **Major UX gaps** — momentum (fling) scrolling for scrollback, a Paste
   key (iOS long-press paste does not reliably reach xterm's hidden
   textarea), and a keyboard-dismiss key (iPhone keyboards have no dismiss
   button, and the keyboard-open chrome collapse hides the Back button, so
   there is currently no way to stop typing and read full-height).

No backend change: the `/terminal` WebSocket resize frame already carries
arbitrary `cols`/`rows`, and `build_isolated_attach_plan` gives each browser
client its own grouped tmux session, so an 80-col PTY cannot shrink any
other client.

Constraints honored: raw xterm/tmux-first stays the only terminal mode; all
existing hardening (reconnect, resize debounce, keyboard suppression, sticky
Ctrl, scroll interception) is preserved and extended, not replaced.

## Files touched

Implementation:
- `crates/ajax-web/web/src/terminalGeometry.ts` (new — pure math)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`
- `crates/ajax-web/web/src/components/TerminalPanel.svelte` (CSS only, if needed)
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/dist/*` (rebuilt bundle)
- `crates/ajax-web/src/slices/install.rs`, `crates/ajax-cli/src/web_backend.rs`
  (only if the web-asset snapshot expectations must be re-synced after rebuild)

Test files this plan explicitly names (approval of this plan approves editing
exactly these):
- `crates/ajax-web/web/src/terminalGeometry.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`
- `crates/ajax-web/web/src/terminalTouchScroll.test.ts` (fling decay math)

## Tasks

### Task 0 — Worktree setup (no test)

- `npm install` (fresh worktree lacks `node_modules`; pre-commit verify needs it).
- Verify: `npm run web:test -- --run` passes on the untouched tree.

### Task 1 — Pure geometry helpers

- Failing tests (`terminalGeometry.test.ts`):
  - `flooredCols(proposedCols, minCols=80)` → returns `max` of the two;
    handles undefined/NaN proposals by returning the floor.
  - `clampPan(panPx, contentPx, viewportPx)` → clamps horizontal pan to
    `[0, max(0, contentPx - viewportPx)]`.
  - `pinchFontSize(baseFontSize, startDistancePx, currentDistancePx, min=7, max=20)`
    → scales base by the distance ratio, rounds, clamps; guards zero/NaN
    distances by returning the base.
- Implement `terminalGeometry.ts`.
- Verify: `npm run web:test -- --run terminalGeometry`.

### Task 2 — 80-column PTY floor

- Failing tests (`TerminalRawView.test.ts`): extend the `FitAddon` mock with
  `proposeDimensions()` returning e.g. `{ cols: 55, rows: 30 }`; assert the
  post-open resize frame sends `cols: 80, rows: 30` and `term.resize(80, 30)`
  was called. A wide proposal (e.g. 120 cols) still fits to 120 — the floor
  only raises, never lowers.
- Implement: replace `fitAddon.fit()` in `fitNow` with
  `proposeDimensions()` + `flooredCols` + `term.resize(cols, rows)`
  (fall back to `fit()` when no dimensions are proposed, e.g. jsdom).
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 3 — Horizontal pan on touch drag

- Failing tests (`TerminalRawView.test.ts`): a touch drag with a horizontal
  component adjusts the terminal host's `scrollLeft` (clamped via `clampPan`)
  while the vertical component still drives `scrollLines`; a vertical-only
  drag leaves `scrollLeft` untouched.
- Implement: track `clientX` in the existing touch handler; apply the
  horizontal delta to the host scroll position. Relax the
  `max-width: 100%` clamps on `.xterm`/`.xterm-screen` so the 80-col canvas
  can exceed the host width (host keeps `overflow: hidden`; only our handler
  moves it). Keep wheel behavior unchanged.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 4 — Pinch-to-zoom font size, persisted

- Failing tests (`TerminalRawView.test.ts`):
  - Two-finger spread updates `term.options.fontSize` (clamped 7–20),
    triggers a refit, and persists `ajax.terminal.fontSize` to localStorage.
  - On mount, a persisted `ajax.terminal.fontSize` overrides the 10px/13px
    default; absent/garbage values fall back to the defaults.
  - Single-finger scroll behavior is untouched while two fingers are down.
- Implement: two-touch branch in the touch handlers using `pinchFontSize`
  (applied via rAF, server resize reuses the existing debounce), plus the
  localStorage read at terminal construction.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 5 — Expand toggle

- Failing tests (`TerminalRawView.test.ts`): the key bar has an
  "Expand terminal" toggle button; activating it adds
  `terminal-expanded` to `<html>` (aria-pressed reflects state), toggling
  again — and unmount — removes it.
- Implement: button in the key bar; CSS in `styles.css`:
  - Mobile: `html.terminal-expanded` hides `.detail-header`,
    `.interact-panel`, `.meta-details` (same selectors as the existing
    `keyboard-open` collapse) so the terminal owns the full band.
  - Desktop: `html.terminal-expanded` makes the terminal panel a fixed
    `inset: 0` overlay above the page chrome.
  - The refit path already reacts via ResizeObserver; no extra wiring.
- Verify: `npm run web:test -- --run TerminalRawView` + manual CSS
  read-through of the new `styles.css` rules.

### Task 6 — Landscape iPhone gets the mobile treatment

- Failing tests (`TerminalRawView.test.ts`): mobile detection returns true
  for a coarse-pointer, low-height viewport even when width exceeds 767px
  (landscape phone) — asserted via the mobile font-size selection with a
  stubbed `matchMedia`.
- Implement: extend `isMobileViewport()` to
  `(max-width: 767px), ((pointer: coarse) and (max-height: 500px))`, and
  extend the `styles.css` / component `@media (max-width: 767px)` takeover
  blocks with the same coarse-pointer landscape clause so the fixed
  full-screen task view, chrome tightening, and keyboard rules apply when
  the phone rotates. iPads (coarse pointer but tall) stay on desktop rules.
- Verify: `npm run web:test -- --run TerminalRawView` + read-through of the
  media-query changes.

### Task 7 — Fix keyboard-open input-line corruption (grid/PTY lockstep)

Root cause (confirmed in `TerminalRawView.svelte`): with the keyboard open,
`fitNow()` still runs `fitAddon.fit()` — shrinking the local grid to the
visible band — while `sendResize()` early-returns, leaving the PTY at the
full row count. tmux cursor-addresses rows beyond the local grid; xterm
clamps them to its last row, so the app's bottom-anchored input box is drawn
one-or-more rows up and overwrites the line(s) below it.

- Failing tests (`TerminalRawView.test.ts`):
  - While the keyboard is open (visualViewport shrunk past the threshold),
    a refit must NOT change the local grid size (no `fit()`/`term.resize()`
    row shrink) — the grid stays at the last size the server was told.
  - While the keyboard is open, the terminal host's visible crop is anchored
    to the bottom of the canvas (host `scrollTop` pinned to max) so the
    cursor/input row remains visible above the keyboard.
  - When the keyboard closes, exactly one settled fit + server resize
    restores the true dimensions (existing flush behavior, re-asserted).
- Note: this task revises the expectations of the two existing keyboard
  tests in `TerminalRawView.test.ts` ("keeps fitting locally but withholds
  the server resize…" and the bottom-follow-on-viewport-resize assertions)
  because the contract they pin — local-only shrink while the server resize
  is withheld — is itself the defect being fixed. The no-server-resize-
  while-keyboard-open hardening is preserved unchanged.
- Implement: gate the grid-changing part of `fitNow` on `!keyboardOpen()`;
  while open, apply a bottom-anchored crop of the host instead (programmatic
  `scrollTop`, host stays `overflow: hidden`); on keyboard close the existing
  debounced flush performs the real fit + resize.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 8 — Momentum (fling) scrolling for scrollback

Native momentum scrolling was deliberately disabled (it desynced from
`scrollLines`); the synthetic replacement only moves while the finger moves,
so paging through long scrollback takes many deliberate drags.

- Failing tests (`terminalTouchScroll.test.ts`): a pure
  `flingFrames(velocityPxPerMs, cellPx, decay)` (or equivalent) helper —
  given a release velocity, yields a finite, decaying sequence of
  line-notch steps; zero/sub-threshold velocity yields nothing; the
  sequence is capped so one fling cannot flood the terminal.
- Failing tests (`TerminalRawView.test.ts`): after a fast drag ends
  (touchend), `scrollLines` continues to be called across subsequent
  animation frames; a new touchstart cancels the fling immediately.
- Implement: track recent move velocity in the touch handler; on touchend
  run an rAF decay loop through the pure helper; cancel on touchstart,
  wheel, or dispose. Reuse the existing notch math for px→lines.
- Verify: `npm run web:test -- --run terminalTouchScroll` and
  `npm run web:test -- --run TerminalRawView`.

### Task 9 — Paste key

- Failing tests (`TerminalRawView.test.ts`): the key bar has a "Paste"
  button; activating it reads `navigator.clipboard.readText()` and feeds
  the text through `term.paste(...)` (so bracketed-paste mode is honored
  and the data flows through the existing `onData` → socket path); a
  clipboard read failure surfaces in the status detail instead of silently
  doing nothing.
- Implement: Paste button in the control-key bar; multi-character `onData`
  strings already pass through to the socket unchanged.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 10 — Keyboard-dismiss key

- Failing tests (`TerminalRawView.test.ts`): the key bar has a
  "Hide keyboard" button that blurs the terminal (`term.blur()`), letting
  the visual viewport expand back to full height (the existing settled
  resize flush then restores the full grid).
- Implement: dismiss button in the control-key bar (compact ⌄ glyph with an
  accessible label). No focus side effects on the other keys change: they
  keep refocusing the terminal.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task 11 — Rebuild bundle, sync snapshots, full validation

- `npm run web:build` (web asset changes trip the string-snapshot tests in
  `crates/ajax-web/src/slices/install.rs` and
  `crates/ajax-cli/src/web_backend.rs`; update those expectations only as
  required by the rebuilt `dist`).
- Run required validation:
  - `cargo fmt --check`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features`
  - `npm run web:check`
  - `npm run web:test -- --run`
- Report results, including anything that failed.

## Round 2 — post-device feedback (2026-07-01)

On-device findings after PR #278: fullscreen toggle is janky, the terminal
is too short, the default text is too small, and a "massive" scrollbar
overlays the text.

Diagnosis:
- The scrollbar is xterm 6's built-in VS Code DOM scrollbar
  (`SmoothScrollableElement`, `verticalScrollbarSize` defaults to 14px) —
  ~3 columns wide on a phone and visible almost constantly because tmux
  redraws keep triggering scroll activity. Ajax owns all touch scroll
  gestures, so on mobile it is purely decorative.
- The 10px mobile default font predates the 80-column floor; its original
  purpose (maximize columns to reduce wrapping) is obsolete.
- The ⛶ expand handler calls `focusTerm()`, which pops the iOS keyboard —
  and with the keyboard open the grid refit is deliberately frozen
  (lockstep fix), so the newly expanded area doesn't refit until the
  keyboard closes. The normal expand refit also waits behind the 300ms
  debounce.
- The mobile task view still spends ~100px on the status/action chrome and
  the "Task details" disclosure below the terminal.

Additional named test file for Round 2 (approval covers editing it):
- `crates/ajax-web/web/src/components/TaskDetail.test.ts` (only if its
  rendering assertions are affected by the chrome collapse)

### Task R1 — Raise the mobile default font to 13px

- Revise the existing "uses a readable font size on a mobile viewport"
  test in `TerminalRawView.test.ts` to expect 13 (the 10px value being
  pinned was a column-count lever the 80-col floor made obsolete; the
  user reports it is too small).
- Implement: `MOBILE_FONT_SIZE = 13`. Pinch persistence still overrides.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task R2 — Hide the xterm DOM scrollbar on touch devices

- Failing test (`TerminalRawView.test.ts`): source-shape assertion that a
  coarse-pointer media rule hides `.xterm-scrollable-element > .scrollbar`
  within the terminal panel.
- Implement: scoped `:global` CSS in `TerminalRawView.svelte` under
  `@media (pointer: coarse)`. Desktop keeps the scrollbar (usable there).
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task R3 — Smooth expand toggle

- Failing tests (`TerminalRawView.test.ts`): activating ⛶ must NOT focus
  the terminal (no keyboard pop), and with the socket open it must send
  the post-layout resize through the immediate path (no 300ms debounce
  wait).
- Implement: drop `focusTerm()` from the expand handler; call the
  post-layout refit (assigned in onMount) after toggling.
- Verify: `npm run web:test -- --run TerminalRawView`.

### Task R4 — Give the terminal more height on mobile

- Collapse remaining chrome in the mobile task view: hide the "Task
  details" disclosure (its facts stay available on desktop), tighten the
  status/action rows, and slim the control-key bar (32px keys).
- Tests: revise the mobile CSS source-shape assertions in
  `TerminalRawView.test.ts` (key bar sizing); adjust
  `TaskDetail.test.ts` only if existing rendering assertions break.
- Verify: `npm run web:test -- --run` (full web suite).

### Task R5 — Rebuild bundle and full validation

- `npm run web:build`, then the full Required Validation set, then commit
  and push to the open PR #278.

## Out of scope

- No snapshot/composer terminal mode (explicitly barred by AGENTS.md).
- No backend/PTY protocol changes.
- Touch text selection / copy-from-terminal: a real gap, but it directly
  conflicts with the scroll-gesture interception (every drag is currently a
  scroll) and needs its own gesture design (e.g. long-press to enter a
  selection mode). Deferred to a follow-up rather than half-fixed here.
- No change to the default 10px mobile font (pinch + persistence now covers
  legibility preference; keeping the default preserves the widest visible
  window).
- Desktop-specific layout work beyond the expand toggle.
