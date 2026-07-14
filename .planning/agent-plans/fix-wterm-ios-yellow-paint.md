# Fix wterm iOS solid yellow/olive paint

## Scope

Device still shows solid olive terminal fill after PR #476. That PR fixed
init-failure mustard banners. `Yellow page.jpg` (2026-07-14 12:47) shows:

- key toolbar present → `WtermTerminalView` mounted (init OK)
- host region y≈448–1966 is perfectly flat `#b6bc72` with zero text pixels
- live `ajax-cli/ci` tmux pane has normal agent text (not yellow ANSI)
- Playwright mobile-webkit paints dark `#1c1714` and can show injected text

## Non-goals

- Reintroduce Ghostty fallback while V2 is on
- Port Ghostty scale-to-fit / zero-lag
- Upgrade `@wterm/*` unless CSS override fails

## Root cause (working)

iOS Safari compositor bug path with `@wterm/dom` `.term-grid` rules
`contain: layout paint style` + `will-change: contents`, possibly interacting
with Ajax `backdrop-filter` chrome. Device-only; e2e WebKit does not reproduce.

## Delegation decision

`Delegation decision: not delegated because parent owns device diagnosis and
the change is a small CSS override + e2e assertion in known files.`

## Checklist

- [x] Override `.term-grid` / `.wterm` GPU hints; force opaque host bg
- [x] Fix undefined `--surface-raised` → `--paper-raised` on wterm keys
- [x] E2e: assert host computed background is dark (not yellow/mustard)
- [x] Focused vitest + mobile-webkit smoke
- [x] Rebuild dist if required by repo convention

## Validation

```bash
npm run web:test -- --run src/components/WtermTerminalView.test.ts
npm run web:smoke -- --project=mobile-webkit e2e/terminal-surface-v2.test.ts
npm run web:check
```

## Deviations

(none yet)
