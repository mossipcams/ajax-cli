# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: docs-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Create the requested rebuild acceptance matrix, physical-iPhone Safari
checklist, untestable-behavior list, and excluded-bug contract for the iOS-only
terminal surface work.

# Allowed files

- create `crates/ajax-web/web/TERMINAL_REBUILD_ACCEPTANCE.md`

# Forbidden changes

- All existing files, source, tests, dependencies, and git commands.
- Do not claim Playwright WebKit proves physical iOS behavior.

# Required structure

1. Scope/status note: normal iOS Safari is the target; mobile Playwright WebKit
   is a regression proxy; desktop Chromium/WebKit are not acceptance targets;
   Home Screen/standalone PWA is not required. Before Task 12 the permanent
   suite passes current Ghostty; Task 12 intentionally removes both old
   surfaces and makes those rows red until the rebuild.
2. Rebuild matrix with exactly the requested columns:
   `Required behavior | Test location | Automated or manual | Current result |
   Physical iOS required`.
3. Concise physical iPhone checklist.
4. Known/suspected bugs deliberately excluded from compatibility.
5. Behaviors that remain untestable without a real device.

# Matrix coverage

Map every requested category, combining only closely related cases:

- one surface/open, readiness/status, disposal/navigation, reopen dedupe,
  disconnect/reconnect/manual recovery, visibility restoration/history seed;
- output visibility manual plus socket delivery automated; chunk order, split
  UTF-8, emoji/combining/wide/ANSI/CR/LF, initialization/resize output, rapid /
  large output;
- typed exact-once/order, Enter/Tab/Escape/arrows, Ctrl combinations, browser
  repeat cardinality, Unicode multiline paste, focus silence, post-transition
  input; Backspace single/held repeat must be physical;
- initial valid size, meaningful viewport/orientation changes, dedupe, keyboard
  burst/close, fullscreen enter/exit, reopen listener/effect bound;
- reading scrollback/New output, page scroll/zoom ownership, long press /
  selection/copy, Paste, touch momentum, pinch persistence;
- current actual settings: pinch-adjusted density persistence. Mark fixed
  theme/font family/cursor/scrollback values as not user settings and old
  Surface V2 as legacy, not acceptance behavior;
- backend WebSocket auth/origin, PTY session targeting, resize/input frames,
  cleanup, output filtering and ordering where existing Rust/unit coverage is
  the appropriate boundary.

Use these locations accurately:

- permanent browser: `e2e/terminal-behavior.test.ts` (24 tests at Task 8);
- permanent frontend boundary: `src/terminalConnection.test.ts`;
- existing deterministic viewport policy: `src/viewport.test.ts`;
- backend: `crates/ajax-web/src/adapters/terminal_pty.rs` and
  `crates/ajax-web/src/runtime.rs` tests;
- old renderer-specific evidence may be cited only as Legacy/removable, never
  the sole future acceptance gate;
- physical checks in this new document.

# Physical checklist

Include real Safari browser chrome and virtual keyboard; printable and special
keys; single Backspace and held-repeat exactness; focus/blur; native multiline
Unicode paste; native selection handles/loupe/long press/copy and fallback;
vertical momentum/horizontal gesture ownership; no surrounding page scroll /
zoom; two-finger pinch and reload persistence; portrait/landscape settling;
keyboard open/close without clipping/offset/resize loop; fullscreen with
keyboard and fresh dimensions; background/foreground socket restoration;
normal Safari tab. Standalone PWA may be an optional diagnostic comparison but
cannot be an acceptance prerequisite.

# Excluded bugs

Explicitly exclude unwanted focus/page zoom, viewport clipping/Safari chrome
offsets, scroll conflicts/yank, keyboard resize/SIGWINCH loops, Ghostty
Backspace-repeat cancellation, incorrect keyboard offsets, renderer garble /
private-selection quirks, arbitrary exact timers/cadence/listener layout, old
xterm rollout/fallback behavior, and any dimension floor not backed by the
deliberate architecture decision. Note that the documented 80-column layout
is backed by `architecture.md` and therefore is not treated as arbitrary.

# Verification

- Every required bullet in the original request maps to a matrix row or an
  explicit not-applicable/currently-not-supported row.
- Every `Automated` row names a real test location.
- Every physical-only claim says `Yes` in the last column and does not say
  Playwright passed it.
- No old renderer-specific test is the sole future acceptance location.
