# Xterm test reliability — deterministic clipboard unavailable state

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: test-reliability
TEST_FIRST: NOT_MEANINGFUL
PRODUCTION_EDIT: FORBIDDEN
```

## Goal

Clipboard-unavailable cases must not depend on ordering between multiple
Playwright init scripts.

## Allowed files

- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

## Implementation

Add a `clipboardUnavailable` option to the existing terminal WebSocket mock so
the same init script deterministically installs either the clipboard stub or an
unavailable clipboard. Replace the four separate `page.addInitScript` removals
with that option. Do not change assertions or production code.

## Verification

- Run all clipboard/fallback/paste cases three times with one worker; all green.
- Full terminal file: green.
- `npm run web:check`; `git diff --check`.

No new behavioral test is meaningful because this removes nondeterministic test
setup without changing product behavior.
