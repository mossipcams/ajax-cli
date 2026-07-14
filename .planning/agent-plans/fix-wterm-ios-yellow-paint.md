# Fix wterm iOS solid yellow/olive paint

## Scope

Device still shows solid olive terminal fill after PR #476 and the CSS
override commit 351f8e1. `Yellow page.jpg` (2026-07-14 12:47) shows:

- key toolbar present → `WtermTerminalView` mounted (init OK)
- host region y≈448–1966 is perfectly flat `#b6bc72` with zero text pixels
- live `ajax-cli/ci` tmux pane has normal agent text (not yellow ANSI)
- Playwright mobile-webkit paints dark `#1c1714` and can show injected text

## Non-goals

- Reintroduce Ghostty fallback while V2 is on
- Port Ghostty scale-to-fit / zero-lag
- Upgrade `@wterm/*` unless CSS override fails

## Root cause (CONFIRMED in @wterm/dom source)

`@wterm/dom` `Renderer.render()` (node_modules/@wterm/dom/dist/renderer.js:374-378)
copies the **bottom-right cell's background color onto the whole `.term-grid`
container as an INLINE style** (`container.style.background = ...`), rewritten
on every dirty-last-row render. Ajax panes run tmux: the bottom row is the
status line (`status-style bg=green`; tmux message/copy-mode line defaults to
`bg=yellow`), so the entire grid gets smeared with the status-line color.
Matches user sightings of green/red/yellow full washes.

Every previous fix (including 351f8e1's `.term-grid { background: #1c1714 }`)
was a plain stylesheet rule — inline style always wins over non-`!important`
author rules, so the fixes could never take effect. The earlier
contain/will-change GPU theory was wrong.

The blank text is a separate suspect: `WTerm.resize()` calls
`renderer.setup()` (wipes all row DOM) *before* `render()`, so `render()` sees
matching dims (`resized=false`) and only repaints rows ghostty reports dirty.
If resize doesn't dirty all rows, the grid stays empty until new PTY output —
iOS resizes constantly (URL bar, keyboard, autoResize jitter). Covered by a
new e2e persistence assertion; fixed only if the test proves it red.

## Delegation decision

`Delegation decision: not delegated because the production edit is a one-line
CSS !important override in a known file — smaller than the work order needed
to describe it.`

## Checklist

- [x] E2e (red first): paint bottom row with yellow bg via socket
      (`ESC[43m` + EL) and assert `.term-grid` computed background stays
      `#1c1714` on mobile-webkit — RED reproduced: `rgb(240, 198, 116)`
- [x] Fix: `background: #1c1714 !important` on
      `:global(.wterm-host.wterm .term-grid)`; dropped the wrong
      contain/will-change overrides and stale comment
- [x] E2e: text survives a viewport resize — PASSED without a fix (ghostty
      dirties all rows on resize); kept as regression guard
- [x] Focused vitest (9/9) + mobile-webkit e2e (3/3) green
- [x] Rebuilt dist (`npm run web:build`); `cargo nextest -p ajax-web -p
      ajax-cli` 464/464 green

## Validation

```bash
npm run web:smoke -- --project=mobile-webkit e2e/terminal-surface-v2.test.ts
npm run web:test -- --run src/components/WtermTerminalView.test.ts
npm run web:check
```

## Deviations

- 351f8e1 shipped a wrong root cause (GPU compositing); this plan replaces it.
