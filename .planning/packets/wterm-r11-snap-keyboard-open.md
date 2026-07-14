# TDD Implementation Packet — R11 wterm snap on keyboard open (iOS Safari)

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
On Surface V2 only: when the iOS keyboard opens while the user is scrolled up
in scrollback, snap to newest output (same contract as Ghostty
`pinToBottomOnKeyboardOpen`). Convert
`it.todo("snaps to the newest output when the keyboard opens while scrolled up")`.

## Hard gates
- `WtermTerminalView*` / Surface V2 only; mobile Safari WebKit
- Never `TerminalRawView`

## Allowed files
- `crates/ajax-web/web/src/components/WtermTerminalView.svelte`
- `crates/ajax-web/web/src/components/WtermTerminalView.test.ts`
- `crates/ajax-web/web/dist/terminal.js` (+ app.css/app.js if web:build updates)

## Forbidden changes
- No safe-area CSS (R10) or visualViewport debounce (R9) in this round
- No TerminalRawView / viewport.ts edits
- No commit/push/branch

## Context evidence
- Graphify/Serena/ast-grep: `NOT_REQUIRED`
- `createTerminalLayoutPolicy().setKeyboardOpen(open)` sets
  `pinToBottomOnKeyboardOpen: true` on the false→true edge once
- Existing `snapToNewest()` in WtermTerminalView scrolls host to bottom
- R8 already has a `MutationObserver` on `document.documentElement` class for
  keyboard close — extend the open edge there (or call
  `layoutPolicy.setKeyboardOpen(isKeyboardOpen())` and honor
  `pinToBottomOnKeyboardOpen`)

## Code anchors
- `snapToNewest`, `scrolledUp`, keyboard MutationObserver in WtermTerminalView
- Todo under keyboard lockstep describe

## Test-first instructions
1. Convert snap todo:
   - Mount; set host scrollHeight/clientHeight/scrollTop so scrolled up
   - Fire scroll so `scrolledUp` is true (or rely on isScrolledUp)
   - Add `keyboard-open` to documentElement
   - Expect host.scrollTop at bottom (scrollHeight - clientHeight or scrollHeight per existing snapToNewest which sets scrollTop = scrollHeight)
   - Expect new-output overlay cleared if it was showing (optional)
2. RED → implement → GREEN
3. Command: `cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts`

## Edit instructions
1. On keyboard open transition (MutationObserver or shared keyboard sync), call
   `layoutPolicy.setKeyboardOpen(true)` and if `pinToBottomOnKeyboardOpen`,
   call `snapToNewest()`.
2. Keep R7/R8 behavior intact.
3. Rebuild dist.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts
cd crates/ajax-web/web && npm run web:build
```

## Acceptance criteria
- Snap todo green; safe-area + debounce todos remain
- RED→GREEN; V2-only

## Stop conditions
- Editing TerminalRawView
- Implementing safe-area or debounce in same round
