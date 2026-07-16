# Fullscreen hotbar–keyboard gap

**Date:** 2026-07-16
**Mode:** Small Fix (behavior test first)
**Reported:** in terminal fullscreen mode there is a space between the hotbar
and the keyboard; there should not be.

## Root cause

`.terminal-panel.is-expanded .terminal-keys` keeps
`padding-bottom: max(2px, env(safe-area-inset-bottom))` (home-indicator pad,
~34 px on iPhone). Its specificity (0,3,0) beats the generic keyboard-open
override `html.keyboard-open .terminal-keys { padding-bottom: 6px }` (0,2,1),
so with the keyboard up in fullscreen the band ends flush at the keyboard top
(`--app-height` pin is correct) but the keys carry 34 px of internal bottom
padding → visible dead gap. Not caught by e2e because
`env(safe-area-inset-bottom)` is 0 in Playwright and the flush assertions
measure the `.terminal-keys` element box, which includes the padding.

## Fix

Add one higher-specificity override in `TaskTerminal.svelte` styles:
`:global(html.keyboard-open) .terminal-panel.is-expanded .terminal-keys
{ padding-bottom: 6px; }` (matches the inline keyboard-open value; keyboard
covers the home indicator, so the safe-area pad is dead space while open).
Keep the existing safe-area rule for keyboard-closed fullscreen — it is
pinned by `TaskTerminal.test.ts:158`.

## Checklist

- [ ] Test: additive source-contract case in
  `src/components/keyboardBandPin.test.ts` asserting the keyboard-open
  expanded keys override exists with a fixed (non-env) padding-bottom; RED
  first.
- [ ] Implementation: the one CSS rule above.
- [ ] Verification: new test GREEN; `TaskTerminal.test.ts` unchanged and
  green; `npm run web:test -- --run`; regenerate `dist/app.js` via
  `npm run web:build:check`; focused mobile-webkit keyboard/fullscreen e2e
  cases green.

Delegation decision: delegated via model-router (MiniMax lane, bounded
two-file change with exact anchors).
