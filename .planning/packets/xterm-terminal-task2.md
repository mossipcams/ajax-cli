# Xterm terminal rebuild — Task 2 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Add exact terminal input and bottom-toolbar behavior to the accepted Task 1
surface: xterm keyboard data, Esc/Tab/Ctrl-C/arrows, sticky Ctrl conversion,
one-frame Unicode paste, Hide keyboard, and no duplicate input after reconnect.

## 3. Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## 4. Forbidden changes

- Do not edit tests, fixtures, dependencies, `TaskDetail.svelte`,
  `terminalConnection.ts`, Rust/backend code, docs, or generated assets.
- Do not add helper modules, abstractions, legacy files, or another dependency.
- Do not alter Task 1 lifecycle/status/disposal behavior.
- Do not commit, push, merge, rebase, create/switch branches, or exceed the
  single allowed file.

## 5. Context evidence

- Graphify: `NOT_REQUIRED`; browser input remains a presentation concern and
  uses the accepted `TerminalConnection.sendInput` boundary without changing
  architecture.
- Serena: `NOT_REQUIRED`; the single component and explicit connection method
  are exact, with no semantic rename or ambiguous call graph.
- ast-grep evidence: `connectTaskTerminal` returns `sendInput(data: string)` at
  `terminalConnection.ts:209–212`, encoding each call into one binary socket
  frame. Exact frame cardinality therefore depends on calling it once per
  xterm/toolbar action.
- Desired behavior: permanent cases at
  `terminal-behavior.test.ts:292–428` and `:746–776`.

## 6. Code anchors

- `TaskTerminal.svelte` accepted Task 1 state: add input/control functions after
  `requestReconnect`; register one `liveTerm.onData` subscription after xterm
  initialization; render controls after status inside the same panel.
- Permanent locator: wrap a `role="toolbar" aria-label="Terminal keys"` inside
  `data-testid="terminal-bottom-controls"`.
- Exact button accessible names: `Tab`, `Esc`, `←`, `↑`, `↓`, `→`, `⌃C`,
  `Ctrl`, `Paste`, `Hide keyboard`.

## 7. Test-first instructions

Before editing, run:

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'printable, control|repeated printable|multiline Unicode|Hide keyboard|typing after manual|supported Ctrl'
```

Expected RED: the toolbar/interaction control locators are absent and input
frames do not match. Do not modify tests; PR 510 supplies the failing cases.

## 8. Edit instructions

1. Subscribe exactly once to xterm `onData`; send each emitted string once
   through `connection.sendInput` only when the connection is open. Dispose the
   subscription during component cleanup.
2. Add the exact toolbar buttons. Prevent pointer/mouse focus theft where
   appropriate, refocus the xterm only when needed, and never send input from
   the Hide keyboard action.
3. Paste must call `navigator.clipboard.readText()` and send the full returned
   multiline Unicode string in one `sendInput` call. Ignore denied/empty reads.
4. Implement sticky Ctrl locally: toggle with a 4-second timeout, consume and
   disarm on the next keyboard/toolbar datum, map ASCII letters to control bytes,
   and map arrows `CSI A/B/C/D` to `CSI 1;5A/B/C/D`. The direct `⌃C` button sends
   `\x03` and leaves sticky Ctrl disarmed.
5. Clear the Ctrl timer and input subscription on cleanup. Do not add a generic
   keyboard/controller abstraction.

## 9. Verification commands

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'printable, control|repeated printable|multiline Unicode|Hide keyboard|typing after manual|supported Ctrl'
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'task route mounts|delayed socket open|socket close reconnects|navigation away closes|pty output corpus keeps|reopening the task route'
rtk npm run web:check
```

## 10. Acceptance criteria

- All six focused input cases pass with exact frame order/cardinality.
- Paste is one frame with exact Unicode/newline content.
- Hide keyboard adds no frame.
- Sticky Ctrl sends `\x03`, `\x1b[1;5D`, `\x03` in the tested sequence and
  disarms after each consumption.
- Manual reconnect does not duplicate xterm input listeners.
- Task 1 group and web checks remain green.
- Only `TaskTerminal.svelte` changes; tests remain untouched.

## 11. Stop conditions

- Passing requires a connection/backend/test edit or a new dependency.
- xterm emits duplicate data that cannot be fixed within this component.
- Task 1 regresses, an unrelated failure blocks proof, or the patch exceeds
  roughly 400 changed lines.
