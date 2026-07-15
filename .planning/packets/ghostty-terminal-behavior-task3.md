# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Extend the permanent iOS-WebKit black-box suite to pin terminal session mount,
navigation disposal, reopen deduplication, and visible connection/recovery
states through the rendered task route and mocked WebSocket boundary.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All production/component/unit/config/dependency/lock/generated/docs/planning
  files and all existing e2e tests.
- No renderer DOM, Ghostty/xterm name, canvas, private terminal state, arbitrary
  sleep, production test hook, or future adapter/controller shape.
- Do not weaken Task 2's one-surface/one-socket assertion.

# Context evidence

- Graphify: architecture boundary is already explicit in `architecture.md`:
  browser surface owns presentation, `terminalConnection.ts` owns socket
  lifecycle, backend owns PTY cleanup. No graph regeneration is needed.
- Serena: NOT_REQUIRED; exact public route, status test ID, reconnect button,
  and fixture anchors are known; no production semantic edit is allowed.
- ast-grep anchor: `class MockTerminalWebSocket` in `e2e/fixtures.ts`; extend
  only its test-facing controls and the exported fixture helpers.
- Lifecycle source: `TerminalRawView.svelte` onMount cleanup calls
  `connection.dispose`; `terminalConnection.dispose` removes visibility
  listener, clears timer, and closes socket.
- Status source: stable `data-testid="terminal-status"`, visible labels
  Connecting/Reconnecting/No live session, and public Reconnect button.
- Existing unit contract: `terminalConnection.test.ts` covers internal backoff;
  this task asserts rendered application results and socket cardinality.

# Code anchors

- Change the fixture signature to
  `mockTerminalWebSocket(page, options: { autoOpen?: boolean } = {})`; pass the
  serializable option into `addInitScript`. Default `autoOpen` to `true` so all
  current e2e tests are unchanged. Add public mock methods `emitOpen()`,
  `emitClose()`, and existing `emitMessage(data)` on each mock socket.
- Export these exact engine-neutral fixture helpers, each filtering URL by the
  existing `/terminal` bridge path and targeting the latest matching socket:
  `terminalSocketSummaries(page): Promise<Array<{ url: string; readyState: number }>>`,
  `openLatestTerminalSocket(page): Promise<void>`,
  `closeLatestTerminalSocket(page): Promise<void>`, and
  `failLatestTerminalSocket(page, message: string): Promise<void>`.
  `failLatestTerminalSocket` must emit `JSON.stringify({type:"error", error: message})`
  and then close that same live socket.
- Append focused tests in `terminal-behavior.test.ts`:
  1. call `mockTerminalWebSocket(page, {autoOpen:false})`; delayed initial open
     shows Connecting; `openLatestTerminalSocket` transitions to connected
     (status banner becomes hidden and no Reconnect button is visible);
  2. `closeLatestTerminalSocket` on the first open socket shows Reconnecting;
     poll summaries until the scheduled second socket exists, manually open it,
     then call `failLatestTerminalSocket` on that same live socket. Assert the
     unavailable outcome via the visible public Reconnect button and nonempty
     status banner, not renderer-specific label copy. Click Reconnect, poll for
     exactly one new third socket, manually open it, and assert exactly one
     active task socket;
  3. navigation away closes the active socket and removes the surface;
  4. reopening yields exactly one active task socket and no duplicate surface.

# Test-first instructions

NOT_APPLICABLE per tests-only contract. The delegate may record an initial
missing-control failure as optional `OTHER` command evidence, but router-level
red evidence is not required for this tests-only task.

# Edit instructions

- Prefer `expect.poll` and locator state over timeouts.
- Count active task sockets (`readyState === OPEN`) separately from historical
  closed instances retained by the mock bag.
- Do not assert reconnect delay/backoff constants or current listener layout.
- A server error must be emitted through the live mocked WebSocket message
  frame and followed by close on that same instance, matching
  `terminalConnection.ts` ordering; never reach into component state.
- If current Ghostty behavior violates the intended contract, leave the focused
  test failing and report; do not edit production.

# Verification commands

```bash
npx playwright test --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- All permanent lifecycle tests pass in mobile WebKit.
- Task route owns one surface and one active task socket.
- Navigation closes the socket; reopen does not duplicate it.
- Connecting, reconnecting, unavailable, and recovery are observed through UI
  and traffic, not private state or renderer-specific unavailable copy.
- Existing e2e tests retain default mock behavior.
- Forbidden-token `rg` has no matches.

# Stop conditions

- Production code is required.
- Existing default fixture behavior would change.
- More than two allowed files are needed.
- Test requires renderer internals or arbitrary sleeps.
