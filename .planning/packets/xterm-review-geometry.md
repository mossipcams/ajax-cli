# Xterm review fix — logical geometry

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Make the local xterm grid and the PTY use the same agent-sized geometry on a
narrow phone host. The xterm grid must be at least 80 columns, then visually
scale to the available host width instead of merely reporting 80 to the socket.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## Forbidden changes

- Do not touch other tests, fixtures, dependencies, generated assets, docs,
  connection/backend code, or planning files.
- Do not restore deleted geometry/refit helpers or add a new helper module,
  abstraction, dependency, or engine selector.
- Do not commit, push, merge, rebase, or change branches.

## RED test

1. Remove the temporary top-level `test.fail(...)` annotation inherited from
   PR 510 so the implementation branch activates the permanent suite.
2. Add one Playwright case beside the existing resize cases. On the 390px phone
   viewport, prove the logical xterm screen is wider and taller than its host
   before CSS transform while its rendered bounding box fills/fits the host,
   and prove the outbound PTY resize reports at least 80 columns. Use the existing task,
   socket, and resize fixtures; inspect only the xterm root/screen needed to
   distinguish a logical 80-column grid from the current narrow FitAddon grid.
3. Run only the new case and retain the expected failure caused by the local
   screen matching the narrow host instead of being an 80-column logical grid.

## Implementation

- Replace `fitAddon.fit()` as the final geometry authority with the smallest
  concrete fit path based on `fitAddon.proposeDimensions()`.
- Resize xterm itself to `max(proposed.cols, 80)` and compensate logical rows
  for the width scale so the transformed terminal still fills the host height.
  Scale the xterm element from the top left so its rendered width fits the host;
  keep the unscaled logical element large enough for the xterm grid.
- Use public `term.resize`, `term.element`, and native DOM measurements only.
  Do not access xterm private `_core` or renderer internals.
- `sendResize` must use the exact resulting `term.cols` and `term.rows`; retain
  existing dedupe and all unrelated behavior.
- Clear/reset scaling when the proposal is already at least 80 columns.

## Verification

```bash
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'logical xterm grid'
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --grep 'initial open|portrait-to-landscape|repeated same-dimension|keyboard-open resize|fullscreen enter and exit each|reopen with meaningful|logical xterm grid'
npm run web:check
```

Report structured RED/GREEN evidence and stop if xterm cannot expose stable
screen geometry without a production test seam.
