# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Pin the user-input behaviors mobile Playwright WebKit can observe without
renderer internals: printable/control/navigation ordering, repeated event
cardinality, multiline Unicode paste, focus/blur silence, and post-reconnect
input. Backspace is explicitly excluded from automation after two proven
stable-surface failures and remains a physical-iPhone check.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All production and other files; no Backspace send/assertion, skip/fixme,
  renderer textarea/canvas/private state, sleeps, or git mutation.

# Context evidence

- Graphify/Serena: NOT_REQUIRED; this is a narrowed tests-only task with exact
  existing stable surface, toolbar, frame bag, and clipboard anchors.
- ast-grep anchors: `mockTerminalWebSocket` options and `terminalFrames` in
  fixtures; append after current six permanent tests.
- Prior Task 5 evidence: `keyboard.press("Backspace")` after printable content
  produced no PTY frame through stable-surface WebKit; the failing delta was
  discarded exactly to the Task 4 snapshot.

# Code anchors

- Add `clipboardText?: string` option preserving current default.
- Add `terminalInputFrames(page)` filtering ordered input frames.
- Use stable-surface interior click and public toolbar only.
- Tests: printable `abc`, Enter, Tab, Escape, arrows exact order; three repeated
  printable events exact cardinality; multiline Unicode Paste one exact frame;
  Hide keyboard adds no input; marker after manual reconnect adds one frame.
- Do not press Backspace in automated tests; Task 10 documents it as Physical.

# Test-first instructions

NOT_APPLICABLE per tests-only contract.

# Edit instructions

- Poll input-frame count deltas; ignore resize frames.
- Expected bytes: `a`,`b`,`c`,`\r`,`\t`,`\x1b`, arrows A/B/C/D.
- No production seam and no renderer-specific selector.

# Verification commands

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|textarea|Backspace|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Five new automatable input tests pass; source guard has no matches.
- No assertion implies Backspace passed automation.
- Existing six permanent tests remain green.

# Stop conditions

- Stable surface cannot focus printable/control events.
- Production/more files/git mutation required.
