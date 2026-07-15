# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Start the permanent iOS-WebKit terminal behavior suite with an engine-neutral
application-surface locator while leaving existing Ghostty characterization
helpers and tests intact.

# Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts` (new)

# Forbidden changes

- All production code, component/unit tests, Playwright config, dependencies,
  lockfiles, generated assets, docs, existing e2e tests, and planning files.
- Do not alter or delete the existing Ghostty-specific `terminalPanel` helper.
- No xterm/Ghostty adapter, renderer DOM, canvas, or private probe dependency.

# Context evidence

- Graphify: NOT_REQUIRED; tests-only change within the existing browser adapter
  and no architecture boundary changes.
- Serena: NOT_REQUIRED; exact fixture/test anchors are already known and no
  production symbol or reusable implementation is being designed.
- ast-grep: existing export anchor found by direct syntax inspection in
  `e2e/fixtures.ts` at `export const terminalPanel`; add a sibling export, no
  structural rewrite.
- Stable application boundary: `TerminalRawView.svelte` and
  `XtermTerminalView.svelte` both expose
  `data-testid="task-terminal-panel"`; the engine attribute is not permanent.
- Existing harness: `mockFetch`, `mockTerminalWebSocket`, and
  `waitForTerminalSocket` in `e2e/fixtures.ts`.
- Browser project: `mobile-webkit` already uses `devices["iPhone 15 Pro"]` in
  `playwright.config.mts`; do not add desktop coverage.

# Code anchors

- Add `terminalSurface(page)` next to `terminalPanel(page)` and select only
  `[data-testid='task-terminal-panel']`.
- New test imports the engine-neutral helper and existing mocks, visits the
  encoded task route, and asserts exactly one visible task terminal surface and
  one open task-terminal WebSocket.

# Test-first instructions

NOT_APPLICABLE per the tests-only packet contract. Still preserve repository
red/green evidence: create the new test importing the not-yet-existing
`terminalSurface`, run it to capture the expected import/type failure, then add
the fixture export and rerun green.

# Edit instructions

- Keep the new test black-box: no `ghostty`, `xterm`, `canvas`, `.terminal-*`
  class, renderer probe, private state, or screenshot assertion.
- Use `expect` polling/locator readiness, never arbitrary sleeps.
- Do not duplicate the existing fetch/socket mock implementation.

# Verification commands

```bash
npx playwright test --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|__ajaxTerminalProbe|data-terminal-engine" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Focused mobile-WebKit test passes.
- New permanent suite locates only the stable application test ID.
- Exactly one surface/socket is observed.
- Existing legacy helper is unchanged.
- Forbidden-token `rg` returns no matches in the new permanent suite.

# Stop conditions

- Stable test ID is absent from either current surface.
- Test requires production code or renderer-generated DOM.
- Existing e2e test must be weakened or changed.
- More than the two allowed files are needed.
