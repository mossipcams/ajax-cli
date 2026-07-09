# Fix: keyboard-open blank space (keep touch→keyboard)

## Constraint (user)

On terminal touch we **still need to open the keyboard**. Do **not** remove
`touchBegan` → `textarea.focus({ preventScroll: true })`.

## Scope

1. Fix massive blank space above the keyboard when it opens (layout band).
2. Improve native iOS copy/paste **without** dropping touch→keyboard.

## Non-goals

- Do not remove focus-on-touchstart / touchBegan.
- No ghostty bump; no desktop 58vh change; no Rust.

## Root cause (blank space)

Keyboard-open shrinks `--app-height` but:

- `.app-viewport` is not `position:fixed` / not offset by `--app-top`
- `.bottom-nav` stays visible (only hidden under `terminal-expanded`)
- task `route-scroll` keeps `padding-bottom: calc(72px + safe-area)` for nav

→ shrunken band + leftover nav clearance = large empty strip above keyboard.

## Tasks

- [ ] T1 (test): CSS/source contracts — keyboard-open hides bottom-nav (+ chrome),
  zeros task route-scroll bottom pad, pins app-viewport to band.
- [ ] T2 (impl): `styles.css` + `AppViewport.svelte` as below.
- [ ] T3 (paste): keep touchBegan; on bare long-press, position textarea under
  finger (rcarmo) before selection; drag still Ajax Copy overlay. If paste
  needs more than that, stop and report — layout is the must-ship.
- [ ] T4: vitest + playwright layout/fullscreen; push to PR 393.

## Impl sketch

**AppViewport.svelte**

```css
:global(html.keyboard-open) .app-viewport {
  position: fixed;
  top: var(--app-band-top, 0px);
  left: 0;
  right: 0;
  height: var(--app-band-height, 100dvh);
  max-height: var(--app-band-height, 100dvh);
  z-index: 30;
}
```

**styles.css** (mobile block)

```css
html.keyboard-open .bottom-nav,
html.keyboard-open .cockpit-chrome {
  display: none;
}
html.keyboard-open [data-testid="route-scroll"]:has([data-outlet="task"]) {
  padding-bottom: 0;
}
```

## Delegation

`Delegation decision: delegated via model-router` — Grok 4.5 High (viewport/Svelte).
