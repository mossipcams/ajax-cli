# Inline hotbar above keyboard

## Scope

Mobile task terminal: host fills the wrap (no gap above hotbar), hotkeys
share row width evenly, keyboard-open stays flush above the keyboard.

Root-cause notes: `.planning/agent-plans/terminal-host-fill-root-cause.md`

## Non-goals

- Desktop terminal height rules.
- PTY protocol / FitAddon min-col math.
- Selection-fit freezes or other CI band-aids unrelated to host fill.

## Delegation

`Delegation decision: not delegated — focused CSS flex-chain + hotbar distribution.`

## Checklist

- [x] Flex chain: route → outlet → detail → panel → wrap → host (`1 1 0%`)
- [x] Host fills via flex (not `height: 100%`)
- [x] Hotbar keys: `flex: 1 1 0` + `width: 0`
- [x] Unit contracts + web:build
- [x] Removed useless CI-chase plan / selection-skip band-aid

## Validation

```bash
npx vitest run --config crates/ajax-web/web/vite.config.mts \
  crates/ajax-web/web/src/components/TaskTerminal.test.ts \
  crates/ajax-web/web/src/components/App.test.ts
npm run web:build
```
