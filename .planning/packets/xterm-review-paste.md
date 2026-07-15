# Xterm review fix — paste semantics and fallback

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Successful clipboard paste must pass through xterm so bracketed-paste mode is
honored. Missing or rejected Clipboard API must expose a visible native textarea
fallback instead of silently doing nothing.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED tests

1. Enable bracketed-paste mode by emitting `\x1b[?2004h`, click Paste with known
   multiline Unicode clipboard text, and require one input frame containing
   `\x1b[200~<text>\x1b[201~`. Current direct socket send is raw.
2. Start without `navigator.clipboard.readText`, click Paste, and require a
   visible accessible fallback textarea plus Send/Cancel controls. Current code
   swallows the failure.

## Implementation

- On successful non-empty read call public `term.paste(text)`; do not call the
  socket directly. Preserve one-frame cardinality through existing `onData`.
- On unavailable/rejected clipboard open the smallest inline fallback with a
  native textarea. Send routes non-empty text through `term.paste`, then closes;
  Cancel closes without input. Show a short status notice while open.
- Empty supported clipboard may simply refocus without sending.
- Preserve accepted geometry, reconnect, keyboard, cleanup, and toolbar input.

## Verification

```bash
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'bracketed paste|clipboard fallback'
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'multiline Unicode paste|Paste stays available|bracketed paste|clipboard fallback'
npm run web:check
```

No other files, helpers, fixtures, dependencies, private APIs, debug logging,
assertion changes, commits, or branch operations.
