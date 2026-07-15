# Xterm paste fix — retain unsent text on disconnect

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Clipboard and native-fallback paste text must not disappear if the task terminal
disconnects before it can be sent.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED tests

1. Open the native fallback, type multiline/Unicode text, close the socket, and
   press Send. Prove the fallback stays open, the exact text remains, no input
   frame is added, and a visible reconnect/unavailable notice is shown.
2. With a successful clipboard read but closed socket, press Paste. Prove the
   exact clipboard text is retained in the native fallback with a visible
   reconnect/unavailable notice and no input frame.

Run both against current code and show silent loss as RED.

## Implementation

- Make the PTY paste helper return whether it sent.
- Dismiss/reset fallback only after a successful send.
- On failure, retain or prefill exact text and show a concise visible notice.
- Preserve original LF/Unicode bytes, public bracketed-paste wrapping, and all
  accepted focus-ownership behavior. Do not auto-reconnect or queue hidden data.

## Verification

1. New disconnect cases plus all paste/fallback/focus cases: green.
2. Full `terminal-behavior.test.ts`, Mobile WebKit, one worker: green.
3. `npm run web:check`.
4. `git diff --check`.

No other files, assertion weakening, plans/packets/generated assets, commits,
branches, dependencies, private APIs, debug code, or unrelated changes.
