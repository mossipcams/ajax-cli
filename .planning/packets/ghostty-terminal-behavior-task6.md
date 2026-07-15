# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Pin the iPhone-WebKit application contract for terminal dimensions and layout
transitions without freezing Ghostty's fit formula, debounce durations,
80-column floor, listeners, or DOM.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All production and other files.
- No Ghostty/xterm names, canvas/textarea/private state, renderer classes,
  sleeps, exact debounce timing, exact row/column formulas, 80-column floor,
  skip/fixme, or git mutations.

# Context evidence

- The permanent suite already mounts through `terminalSurface`, controls the
  mocked task WebSocket, and records outbound JSON frames.
- Existing renderer-specific `fullscreen-refit.test.ts` proves the current
  behavior but is not a rebuild contract.
- `TerminalRawView.svelte` listens to window/orientation/visualViewport and
  reports `{type:"resize",cols,rows}` through the public socket boundary.
- Existing unit suites cover viewport inputs and scheduling mechanics; this
  task asserts only their application-visible result.

# Code anchors

- Add a typed `terminalResizeFrames(page)` fixture helper that filters the
  shared outbound frame bag for finite positive integer `cols`/`rows` fields.
  It must not know renderer internals.
- Add a fixture helper that dispatches a caller-specified burst of public
  `window.resize`, `orientationchange`, and `visualViewport.resize` events.
- Use `page.setViewportSize`, `html.keyboard-open` as the existing public
  application viewport signal, and accessible `Expand terminal` controls.

# Required tests

1. Initial open eventually sends at least one valid positive-integer PTY size.
2. Portrait-to-landscape viewport/orientation change eventually produces a
   fresh valid final resize; adjacent duplicate dimension pairs are absent.
3. A repeated same-dimension viewport event burst followed by a meaningful
   viewport change produces a bounded result with no adjacent duplicate size
   pairs (assert outcome, not debounce duration).
4. While `keyboard-open` is present, a resize-event burst does not create a
   resize storm; closing it eventually produces one valid settled resize and
   the resulting slice contains no adjacent duplicates.
5. Fullscreen entry and exit each eventually produce a fresh valid resize,
   leave the accessible toggle state correct, and retain one active socket.
6. Navigate away and reopen, then make one meaningful viewport change; the
   active session produces a bounded deduplicated resize result and only one
   surface/socket remains.

If the current browser behavior cannot deterministically distinguish a count
bound without a timing assertion, assert valid final dimensions plus absence
of adjacent duplicates and document the narrower evidence. Do not add a fake
production seam.

# Test-first instructions

NOT_APPLICABLE per tests-only contract. Add fixture helpers only as required by
the tests in the same change.

# Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
npm run web:test -- --run src/viewport.test.ts src/terminalRefit.test.ts src/terminalLayoutPolicy.test.ts src/terminalOutputPolicy.test.ts
rg -n "ghostty|xterm|canvas|textarea|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout|MIN_TERMINAL_COLS" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Six new result-oriented dimension/layout tests pass in mobile WebKit.
- Existing eleven permanent tests remain green.
- No renderer or current scheduling implementation becomes a requirement.

# Stop conditions

- Production edits or renderer-specific selectors are required.
- The requested event outcome is nondeterministic without arbitrary sleeps.
  Keep deterministic coverage, report the physical-iOS remainder, and stop.
