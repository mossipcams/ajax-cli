# wterm ghostty-parity behavioral tests

## Scope
Port the behavioral contract of the Ghostty terminal (`TerminalRawView.test.ts`)
onto the wterm Surface V2 experiment (`WtermTerminalView.svelte`) as tests:

- Real behavioral tests for everything the wterm view already implements
  (key bar, Ctrl arm/fold/timeout, paste, status labels, reconnect button,
  focus-on-open, closed-connection guard).
- `it.todo` entries — named after the source Ghostty test titles — for the
  parity gaps, grouped by owning area, so the suite is the executable
  parity checklist for reproducing Ghostty functionality on wterm.

## Non-goals
- No implementation changes to WtermTerminalView.svelte or wterm modules.
- No tests for zero-lag overlay or Ghostty selection-manager casts —
  TERMINAL.md marks those intentionally out of scope for wterm.
- No tests for WS backoff/visibility reconnect — shared `terminalConnection.ts`
  already owns and tests those for both surfaces.

## Delegation decision
Round 1 (test authoring): not delegated — the deliverable is the behavioral
specification itself; a work order would have to enumerate every test.

Round 2 (implement findings 1+2): delegated via model-router →
tdd-implementation-packet (READY) → cursor-delegate (composer-2.5; the GLM
security lane was rerouted once as unavailable per its 4-hang/0-success
record). Delegate proved red (2 intended failures, 22 pass) → green (24
pass), touched only the two allowed files (+19/-3 production). Review Gate:
ACCEPT. Parent rebuilt the committed dist (vendored-dist trap) and
re-validated independently: web vitest 595 passed; cargo nextest
-p ajax-web -p ajax-cli 464 passed.

## Tasks
- [x] Read TerminalRawView.test.ts behaviors + WtermTerminalView implementation
- [x] Extend WtermTerminalView.test.ts harness (capture all connection events,
      controllable isOpen, shared reconnectNow spy)
- [x] Add "ghostty parity — implemented" component tests (13)
- [x] Inspect @wterm/dom + @wterm/ghostty source to map native capabilities
      (round 2: "improve, don't port")
- [x] Add real-WASM behavioral suite in
      terminalWtermGhosttyCore.integration.test.ts driving the TERMINAL.md
      bake-off scenarios against the real core + DOM renderer (12 tests):
      echo/backspace, alt-screen, resize reflow, UTF-8 wide glyphs,
      scrollback, native scroll-follow (no-yank + re-pin), snap-on-type,
      DECCKM app-cursor arrows, bracketed-paste wrap + ESC-injection strip,
      iOS-safe hidden input, DSR/DA canary
- [x] Rewrite the it.todo checklist around wterm-native capabilities the
      component bypasses + remaining Ajax-chrome gaps (25 todos)
- [x] Validate focused files and full web suite

## Deviations / findings
- wterm natively implements scroll-follow, snap-on-type, VT response pump,
  app-cursor arrows, bracketed paste, and iOS input attrs — those are pinned
  with the real WASM instead of mock-ported from Ghostty tests.
- FINDING: the component's Paste key bypasses wterm's safe paste path — no
  bracketed-paste wrap, no ESC-injection strip (wterm strips \x1b so clipboard
  text cannot close the \x1b[200~ guard). Captured as a priority todo.
- FINDING: key-bar arrows hardcode CSI and are wrong under DECCKM (vim/less);
  wterm's own keyboard path handles this. Ghostty's key bar had the same
  defect — improvement todo, not a port.
- FINDING (upstream): @wterm/ghostty 0.3.0 never answers DSR/DA device
  queries (probed directly; read_response yields null). Pinned as a named
  canary test that flips red when an upgrade implements responses.
- Wide CJK glyphs render with a spacer cell ("世 界") — asserted per-glyph.

## Validation results
- `npx vitest run src/terminalWtermGhosttyCore.integration.test.ts
   src/components/WtermTerminalView.test.ts` — 37 passed, 0 failed
- `npx vitest run` (full ajax-web web suite) — 593 passed, 0 failed
- Fresh worktree needed `npm install` first (known worktree gap)
