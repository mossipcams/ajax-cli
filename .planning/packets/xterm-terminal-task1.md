# Xterm terminal rebuild — Task 1 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Mount exactly one new xterm.js terminal on a task route, open exactly one
existing task-terminal WebSocket, render connection status/reconnect behavior,
write PTY output without application errors, and dispose the renderer/socket
when navigating away. This is only lifecycle/output Task 1; input controls,
resize policy, fullscreen, scrolling, and gestures are later packets.

## 3. Allowed files

- `package.json`
- `package-lock.json`
- `crates/ajax-web/web/src/components/TaskDetail.svelte`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte` (new)

## 4. Forbidden changes

- Do not edit any test, fixture, generated `dist/` asset, Rust source, or
  `terminalConnection.ts`.
- Do not recreate any path listed in `legacyTerminalRemoval.test.ts`, especially
  `XtermTerminalView.svelte`, `TerminalSurfaceSelector.svelte`, or a legacy
  helper module.
- Do not restore Ghostty, Surface V2 settings, legacy terminal abstractions, or
  old files wholesale from Git history.
- Do not implement Task 2–5 behavior beyond markup strictly needed by the Task 1
  cases.
- Do not commit, push, merge, rebase, create/switch branches, or touch unrelated
  files.

## 5. Context evidence

- Graphify: `NOT_REQUIRED` because this bounded presentation change does not
  change ownership or cross-crate flow; authoritative `architecture.md` lines
  681–716 already pin `ajax-web::slices::terminal` and the PTY adapter as backend
  owners while the browser consumes `terminalConnection.ts`.
- Serena: `NOT_REQUIRED` because there is no rename or ambiguous semantic call
  graph: one new Svelte component imports the explicit exported connection
  contract and `TaskDetail.svelte` mounts it once.
- ast-grep: `export function $NAME($$$ARGS): $RET { $$$BODY }` identifies
  `connectTaskTerminal` at `terminalConnection.ts:42`; its returned interface
  exposes `reconnectNow()` and `dispose()` at lines 217–223.
- Desired behavior: PR 510's permanent engine-neutral cases in
  `e2e/terminal-behavior.test.ts:142–290`; current baseline is 0/27 with the
  first failure `task-terminal-panel` absent.
- Existing dependency evidence: the immediately preceding implementation used
  compatible pinned `@xterm/xterm` 6.0.0 and `@xterm/addon-fit` 0.11.0. Re-add
  only these packages; do not add Ghostty or another terminal dependency.

## 6. Code anchors

- `package.json`: add one top-level `dependencies` object after
  `devDependencies`, containing only `@xterm/addon-fit: "0.11.0"` and
  `@xterm/xterm: "6.0.0"`; update the lockfile through npm.
- `TaskDetail.svelte:6`: import the new `TaskTerminal.svelte` beside
  `ActionBar.svelte`.
- `TaskDetail.svelte:67`: mount `<TaskTerminal
  handle={detail.qualified_handle} />` once immediately before the task details
  block, inside `.task-detail`.
- New `TaskTerminal.svelte`: import `Terminal`, `FitAddon`, xterm CSS,
  `onMount`, and the existing `connectTaskTerminal` types. Use the permanent
  locator `data-testid="task-terminal-panel"` and status locator
  `data-testid="terminal-status"`.

## 7. Test-first instructions

Run this exact RED command before editing and retain the expected missing-panel
failure:

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'task route mounts|delayed socket open|socket close reconnects|navigation away closes|pty output corpus keeps|reopening the task route'
```

The expected failing assertion is `task-terminal-panel` not found. Do not add or
edit a test because PR 510 already supplies the required failing behavior tests.

## 8. Edit instructions

1. Install the two exact dependencies so `package.json` and `package-lock.json`
   change reproducibly.
2. Implement one concrete Svelte component. On mount:
   - create `Terminal`, load `FitAddon`, and open it into one bound host;
   - create exactly one `connectTaskTerminal(handle, events)` connection;
   - send decoded `onOutput` text to `term.write`;
   - mirror `connecting`, `connected`, `reconnecting`, and `unavailable` into
     the status UI; hide the status with `aria-hidden="true"` when connected;
   - show `Reconnect` only for reconnecting/unavailable and call the existing
     `reconnectNow()`.
3. Cleanup must dispose the connection, addon, terminal, and any Task 1 local
   resource exactly once.
4. Add only minimal component-scoped layout needed for a visible, nonzero xterm
   host on the mobile task route. The component must not own task truth.
5. Mount it once in `TaskDetail.svelte` using the qualified task handle.

## 9. Verification commands

```bash
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'task route mounts|delayed socket open|socket close reconnects|navigation away closes|pty output corpus keeps|reopening the task route'
rtk npm run web:check
rtk npm run web:test -- --run crates/ajax-web/web/src/legacyTerminalRemoval.test.ts
```

## 10. Acceptance criteria

- All six focused lifecycle/output cases pass.
- Exactly one visible terminal panel and one task socket exist per task route.
- Connected output produces no page error.
- Navigation/unmount leaves zero active task sockets.
- Reconnect UI matches the permanent locators and status behavior.
- Legacy removal hygiene remains green.
- Web type/Svelte checks pass.
- Only allowed files changed; no tests changed.

## 11. Stop conditions

- A focused case requires changing a test or `terminalConnection.ts`.
- The new component cannot be implemented without a legacy-forbidden filename,
  helper, selector, setting, or Ghostty dependency.
- An unrelated pre-existing failure prevents proving Task 1 acceptance.
- The patch exceeds roughly 400 changed lines or touches an unlisted path.
