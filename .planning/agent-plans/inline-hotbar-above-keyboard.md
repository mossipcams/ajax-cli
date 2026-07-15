# Inline hotbar above keyboard

## Scope

When tapping the terminal (not fullscreen), the hotkey bar must sit flush
above the iOS keyboard — same geometry as fullscreen. Header/status stay
visible above the terminal.

## Root cause

Fullscreen pins `.terminal-panel.is-expanded` to `--app-band-top` /
`--app-band-height`, so keys at the panel bottom sit on the visual-viewport
bottom (above the keyboard).

Inline + `keyboard-open` only tried flex-fill inside `route-scroll`. The
`section[data-outlet=task]` / height chain does not reliably stretch the
panel to the band bottom, so keys float mid-page or sit under the keyboard.

## Fix

When `html.keyboard-open` on mobile, pin `.task-detail` itself to the app
band (same top/height as fullscreen). Header + one-row status stay
`flex: none` at the top; terminal panel `flex: 1`; keys remain at the
bottom of that column = above the keyboard.

## Delegation

`Delegation decision: not delegated because smaller than the work order —
targeted CSS geometry already diagnosed; iterating on open PR #518.`

## Checklist

- [ ] CSS: keyboard-open `.task-detail` fixed to app band
- [ ] Terminal panel fills remaining; keys flex-none at bottom
- [ ] Test contract + focused verify
- [ ] Push to PR #518
