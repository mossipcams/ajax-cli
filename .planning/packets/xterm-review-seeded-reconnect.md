# Xterm review fix — seeded reconnect

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Manual seeded reconnect must replace stale local history and restore live follow;
automatic unseeded reconnect must preserve the local buffer.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED test

Add one black-box case: write enough output, scroll the interaction surface away
from live, force the socket into unavailable, click manual Reconnect, open the
new seeded socket, and emit seed/live output. Require `New output` to remain
absent and the interaction surface to be at its bottom. Current `onOpen` ignores
`(isReconnect, seeded)` and fails this behavior.

## Implementation

- Consume both `onOpen` arguments.
- Only when `isReconnect && seeded`: reset xterm before seeded output arrives,
  set follow live, clear unseen UI, synchronize spacer, and snap both xterm and
  wrapper to bottom.
- Preserve buffer/follow state for unseeded automatic/visibility reconnects.
- Preserve status, resize dedupe, geometry, and input behavior.

## Verification

```bash
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'seeded reconnect restores live follow'
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'socket close reconnects|typing after manual reconnect|seeded reconnect restores live follow|reading scrollback'
npm run web:check
```

No other files, helpers, fixtures, dependencies, private APIs, debug logging,
assertion changes, commits, or branch operations.
