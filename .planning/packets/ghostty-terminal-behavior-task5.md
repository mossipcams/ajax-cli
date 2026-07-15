# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Pin iOS-WebKit user-input behavior through stable surface interaction and
decoded PTY frames: printable text, control/navigation keys, repeated browser
inputs, multiline Unicode paste, focus/blur silence, and input after reconnect.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All production and other test/config/dependency/lock/generated/docs/planning
  files.
- No renderer textarea/canvas/generated DOM/private state/probe, no screenshots,
  no arbitrary sleeps, no exact physical-iOS repeat claim, and no git mutation.

# Context evidence

- Graphify: NOT_REQUIRED; input travels from rendered browser surface through
  existing `terminalConnection.sendInput` to the PTY boundary; no ownership
  change.
- Serena: NOT_REQUIRED; stable surface, key toolbar, clipboard mock, and frame
  bag anchors are exact; production edits are forbidden.
- ast-grep anchors: `mockTerminalWebSocket` options, `terminalFrames`,
  `terminalSurface`, and `terminalToolbar` in `e2e/fixtures.ts`.
- Existing smoke covers toolbar Esc/Tab/Ctrl-C/Ctrl-arrow and one ASCII paste;
  permanent suite must extend exact-once browser input and Unicode paste without
  depending on `data-terminal-engine` or renderer nodes.

# Code anchors

- Add `clipboardText?: string` to `mockTerminalWebSocket` options, defaulting to
  current `"echo pasted"`; pass it with `autoOpen` into `addInitScript` and keep
  all existing call sites unchanged.
- Reuse `terminalFrames(page)` or add a small `terminalInputFrames(page)` helper
  returning ordered `{type:"input", data:string}` frames.
- Append permanent mobile-WebKit tests:
  1. click the stable terminal surface at an interior point, send printable
     `abc`, Enter, Backspace, Tab, Escape, and four arrows through Playwright
     keyboard events; assert expected PTY data exactly once and in order;
  2. send the same printable/backspace browser event three times; assert three
     frames, not fewer or more (automation proxy only, not physical hold repeat);
  3. boot with multiline Unicode clipboard text, click public Paste button, and
     assert content is preserved exactly in one input frame;
  4. focus then blur via public Hide keyboard control without typing and assert
     no new input frame;
  5. after public reconnect recovery, refocus the stable surface, type one
     marker, and assert exactly one new marker frame.
- Supported expected bytes: Enter `\r`, Backspace `\x7f`, Tab `\t`, Escape
  `\x1b`, arrows `\x1b[A/B/C/D`. If stable surface click cannot focus in
  Playwright without renderer DOM, leave that test failing and report rather
  than adding a production seam in this task.

# Test-first instructions

NOT_APPLICABLE per tests-only contract. Optional OTHER evidence may show the
missing clipboard option/helper before addition.

# Edit instructions

- Use public application controls, stable test ID, Playwright keyboard, and
  mocked traffic only.
- Derive deltas from frame counts before/after actions so resize traffic cannot
  affect input assertions.
- Use polling/locators, not timing constants or `waitForTimeout`.
- Keep physical hold-to-repeat, native paste menu, and modifier/shortcut OS
  fidelity for Task 10's physical checklist.

# Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|textarea|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Exact ordered PTY data for supported browser keys.
- Repeated events have exact cardinality.
- Multiline Unicode paste is one exact frame.
- Focus/blur sends nothing.
- Input works exactly once after reconnect.
- Permanent suite stays renderer-neutral and passes mobile WebKit.

# Stop conditions

- Stable surface interaction cannot focus without renderer internals.
- Production code or more files are required.
- Current behavior violates expected bytes/cardinality (leave failing and report).
- Any git mutation is attempted.
