# Xterm review fix — keyboard resize and disposal

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Freeze ordinary local fit and PTY resize while `html.keyboard-open` is present,
but allow one explicit fit+resize for expand-enter and pinch-end. Ensure no
animation frame or resize timer can run terminal work after component disposal.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## Forbidden changes

- No other files, helpers, dependencies, fixtures, private xterm APIs, debug
  logging, assertion changes, or unrelated formatting.
- Do not commit, push, merge, rebase, or change branches.

## RED tests

1. Add one black-box case: open the task, set `html.keyboard-open`, record PTY
   resize frames, enter fullscreen, and require one fresh positive-integer
   non-duplicate resize while the class remains present and the socket remains
   singular. Current code blocks it.
2. Add the smallest disposal case: schedule the post-layout fullscreen path and
   immediately navigate away; after two animation frames there must be no page
   error, active socket, or terminal surface. Current nested frame is untracked.
3. Run both and retain intended RED evidence before production edits.

## Implementation

- Give the fit/resize scheduler one boolean discrete-intent override. Ordinary
  debounced viewport events return before local fit or PTY resize while the
  keyboard is open and cancel pending ordinary work.
- Expand-enter and pinch-end pass the override and perform a single coherent
  fit+resize. Expand-exit does not gain the exception.
- Track the nested post-layout frame, mark disposal before cleanup, guard frame
  callbacks, and cancel every timer/frame on cleanup.
- Preserve geometry, resize dedupe, socket lifecycle, and existing tests.

## Verification

```bash
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'keyboard-open expand|scheduled terminal work'
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'keyboard-open resize|fullscreen enter and exit each|outward pinch|keyboard-open expand|scheduled terminal work'
npm run web:check
```
