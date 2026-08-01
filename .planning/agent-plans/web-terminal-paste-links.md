# Fix: pasting links into Web Cockpit terminal

## Scope

Make native paste of HTTP(S) links into the Ajax web terminal send the URL to
the PTY. Today xterm only reads `clipboardData.getData('text/plain')`. Rich
link clips (Safari/Chrome “Copy Link”, Messages, etc.) often put the URL in
`text/uri-list` and/or `text/html` with an empty or non-URL `text/plain`, so
paste appears to do nothing.

## Non-goals

- No change to the floating link Open/Copy menu for URLs already on screen
- No ghostty / xterm library bump
- No CLI/TUI clipboard changes
- No architecture.md changes

## Suspected root cause

`@xterm/xterm` `handlePasteEvent` only uses `text/plain`. Toolbar Paste uses
`navigator.clipboard.readText()` (plain only) and is covered by e2e; native
ClipboardEvent paste of rich links is not.

## Approach

1. Add a small `readPasteText(data: DataTransfer | null): string` helper that
   prefers an http(s) URL from `text/plain`, `text/uri-list`, or an HTML `href`.
2. Own the terminal helper-textarea `paste` event in capture phase:
   `preventDefault` + `stopImmediatePropagation`, then send via existing
   `pasteThroughTerm` / `pasteToPty` (bracketed paste + disconnect retain).
3. Reseed the ZWS backspace sentinel after handling.
4. Keep toolbar Paste behavior; optionally reuse the same URL-preferring
   extraction only if clipboard API later exposes typed reads (out of scope
   unless trivial).

## Approval

- User reported: “Pasting links in the terminal on ajax web does not work at all”
- Mode: Behavior Change. Delegation via model-router.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] **T1 (test):** unit tests for `readPasteText` — plain URL; uri-list-only;
  html-href-only; prefer http(s) when plain is link title; empty → `""`
- [x] **T2 (test):** TaskTerminal source contract + e2e uri-list-only paste
- [x] **T3 (impl):** helper + capture paste listener wired to `pasteThroughTerm`
- [x] **T4 (verify):** focused vitest + web:check; parent reviewed diff

## Validation

```bash
npm run web:test -- --run src/shared/lib/clipboard.test.ts src/features/task/TaskTerminal.test.tsx
# 35 passed
npm run web:check
# passed
npm run web:smoke -- --grep "native uri-list-only paste"
# 1 passed
npm run verify
# passed (exit 0)
cargo build --release -p ajax-cli
# passed
cargo install --path crates/ajax-cli --locked --force
# passed
```

## Deviations

- Parent tightened `readPasteText` after delegate: never return raw HTML;
  keep full plain when it starts with a URL plus trailing text.
- Packet verification used `cd crates/ajax-web/web && npm run check` which has
  no package.json; correct commands are root `npm run web:test` / `web:check`.
- Delegate report envelope failed parsing (`MISSING_STRUCTURED_REPORT`) but
  delta was in scope and behavior is correct after parent review.
- Ponytail follow-up: inlined clipboard helpers; dropped TaskTerminal
  source-regex paste pin (e2e covers wiring).
- Follow-up bugfix: capture paste always `preventDefault`d before reading,
  so empty Safari `clipboardData` swallowed pastes. Only cancel when we have
  text; handle `insertFromPaste` beforeinput; async `readText` fallback.
