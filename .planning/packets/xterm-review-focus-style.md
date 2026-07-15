# Xterm review fix — focus and control styling

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Toolbar controls must preserve terminal focus only when the terminal already
owned it; fullscreen exit must blur. Use pointer focus prevention on touch,
correct the undefined raised-surface token, and keep primary mobile controls at
least 44px in both dimensions.

## Review revision

The first delegated pass is not accepted. Fix these findings with a new RED →
GREEN round:

1. The touch-target test measures height only and misses narrow keys. Assert
   both dimensions for every visible terminal button, then give short controls
   a 44px minimum width without redesigning the toolbar.
2. `toolbarPointerOwnedFocus` can leak from a pointer activation into a later
   keyboard or assistive click. Add an Enter/Space regression after a
   terminal-owned pointer click, consume/reset the captured value on click, and
   treat a click with `MouseEvent.detail === 0` as not pointer-owned.
3. The fullscreen test checks focus only after both clicks. Prove focus remains
   owned immediately after entry, then prove exit blurs it.
4. Paste captures focus on pointerdown but successful paste always refocuses.
   Add coverage for Paste invoked while a non-terminal element owns focus and
   preserve/refocus only when terminal focus was actually owned before the
   pointer activation. Keep the async clipboard path race-safe.

## Second review revision

The successful clipboard path, stale keyboard ownership, and fullscreen proof
are accepted. Two findings remain:

1. The native fallback does not retain captured focus ownership. Add coverage
   that opens fallback while a non-terminal control owns focus and proves both
   Cancel and Send do not refocus xterm; also prove the terminal-owned path can
   restore focus. Store the captured ownership with fallback state and use only
   conditional refocus on close/send.
2. The size test covers only expand and toolbar keys. Exercise the conditional
   New output, Reconnect, and fallback Send/Cancel states and assert every
   visible terminal button in those states is at least 44×44. Do not merely add
   selectors for elements that never render in the test.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED tests

1. Focus a non-terminal task-detail element, press a control-key toolbar button,
   and prove the hidden xterm textarea does not become active; then focus the
   terminal, press a control key, and prove focus remains there. Current clicks
   always refocus xterm.
2. Focus the terminal, enter and exit fullscreen, and prove the hidden textarea
   is blurred on exit without sending PTY input.
3. On the phone viewport, require every visible terminal button to have at least
   a 44px rendered width and height.
4. After a terminal-owned pointer activation, activate a toolbar control by
   keyboard and prove stale ownership does not refocus xterm.
5. Invoke successful Paste while another control owns focus and prove xterm is
   not unconditionally refocused.

## Implementation

- Replace mouse-only prevention with pointerdown prevention. Capture whether
  the xterm textarea owned focus before the pointer action; refocus with
  `preventScroll` only in that case after control-key/Ctrl clicks.
- Fullscreen exit blurs the xterm textarea. Enter behavior and socket remain.
- Replace all `--surface-raised` references with existing `--paper-raised`.
- Set terminal button minimum width and height to 44px where needed;
  do not redesign the toolbar.
- Preserve paste fallback, input cardinality, geometry, and resize behavior.

## Verification

```bash
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'toolbar preserves prior terminal focus|keyboard activation does not reuse pointer focus|Paste preserves prior terminal focus|fullscreen exit blurs|terminal controls meet mobile touch target'
npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit --workers=1 --grep 'printable, control|Hide keyboard|supported Ctrl|fullscreen enter and exit keep|toolbar preserves prior terminal focus|fullscreen exit blurs|terminal controls meet mobile touch target'
npm run web:check
```

No other files, helpers, fixtures, dependencies, private APIs, debug logging,
assertion changes, commits, or branch operations.
