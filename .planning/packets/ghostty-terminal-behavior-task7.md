# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior-change
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: ALLOWED_ONE_TEST_ID
BLOCKERS: []
```

# Goal

Protect observable iPhone-WebKit reading, touch/selection, clipboard, and
fullscreen continuity behavior without renderer DOM, canvas pixels, or private
terminal state. Physical touch fidelity remains manual.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TerminalRawView.svelte` only to add
  `data-testid="terminal-interaction-surface"` to the existing user gesture /
  scroll target after the red locator failure.

# Forbidden changes

- Any production logic/style/refactor or other file.
- Ghostty/xterm names, current class names, canvas/textarea/private state,
  screenshots, pixel assertions, arbitrary sleeps, skip/fixme, or git commands.
- Do not assert native iOS selection handles, Safari menus, real momentum,
  browser zoom, or pinch/font persistence in Playwright.

# Context evidence

- The outer `task-terminal-panel` includes controls; its inner current gesture
  and scroll target has no stable public locator. A single test ID is the
  smallest black-box seam and can be reproduced by the rebuilt surface.
- `New output ↓`, `Copy`, `Paste`, `Expand terminal`, and terminal status are
  already accessible user-visible controls.
- The permanent suite already observes PTY input frames and emits PTY output.

# Required RED evidence

1. Add `terminalInteractionSurface(page)` in the fixture and a focused test
   that requires it to be visible. Run that test and capture failure because
   the production test ID is absent.
2. Add only the stable test ID attribute to the existing user interaction /
   scroll element; rerun and capture pass before adding the remaining tests.

# Required behavior tests

1. With enough PTY lines to create scrollback, move the stable interaction
   surface away from the latest output using its public scroll position and
   dispatch `scroll`; subsequent output shows `New output ↓`. Clicking that
   control hides it and sends no input.
2. Emit terminal text, dispatch a single-finger long press on the stable
   interaction surface, and prove the visible `Copy` affordance becomes
   available while zero PTY input is sent. End/cancel the gesture cleanly.
   Use Playwright polling for the UI transition, not a sleep. If the proxy
   cannot resolve a non-empty selection without renderer coordinates, retain
   only the deterministic no-input assertion and report Copy/selection as
   physical iOS; do not inspect renderer internals.
3. A representative synthetic single-finger touch/scroll gesture on the stable
   interaction surface sends no PTY input and leaves the surrounding route /
   document scroll position unchanged. Do not claim this proves native
   momentum; label that physical-only in Task 10.
4. Enter fullscreen, send one printable marker, exit fullscreen, send another;
   both reach the same active PTY exactly once and in order, with one active
   socket and one terminal surface.
5. The public Paste control remains visible/usable after the gesture and
   fullscreen sequence. Reuse the existing exact Unicode paste behavior; do
   not duplicate it unless transition-specific state is under test.

# Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|textarea|terminal-host|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Stable user interaction seam has one attribute-only production change.
- Deterministic reading/input-continuity tests pass; unsupported native touch
  fidelity is reported for the physical checklist rather than faked.
- Existing 17 permanent tests stay green.

# Stop conditions

- More production changes than the single test ID are required.
- Visible selection requires engine-specific coordinates/private state.
- Synthetic events would be mislabeled as physical iOS proof.
