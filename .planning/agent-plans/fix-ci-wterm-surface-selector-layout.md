# CI fix: Ghostty e2e scroll drag blocked

## Scope

Unblock PR #465 Web CI. Playwright `canvas.dragTo` fails because Ghostty's
bottom `textarea` intercepts pointer events; layout regression from
`.surface-selector { display:flex }` made this consistently fatal.

## Non-goals

- Changing Ghostty textarea / iOS paste behavior
- Rewriting e2e helpers

## Cause

`.surface-selector` flex wrapper (added for wterm fill) sits between
`.terminal-primary` and `TerminalRawView`, altering Ghostty host geometry so
swipe tests start on the 44px bottom input and hang on `dragTo`.

## Fix

Use `display: contents` on `.surface-selector` so Ghostty (and wterm root)
participate in `.terminal-primary` flex exactly as on main. Keep the mustard
error-banner tweak.

## Delegation decision

`Delegation decision: not delegated because smaller than the work order
needed to describe it` — one CSS rule change + dist rebuild.

## Checklist

- [ ] `.surface-selector { display: contents; }`
- [ ] Update selector test if needed
- [ ] Rebuild dist; focused vitest; push
