# Fullscreen hotbar–keyboard gap

**Date:** 2026-07-16
**Mode:** Small Fix
**Reported:** gap still present on device after #557 / 0.47.9

## Root cause (updated)

#557 correctly added the source CSS override in `TaskTerminal.svelte`, but
never rebuilt/committed `crates/ajax-web/web/dist/app.css`. Ajax embeds the
dist assets via `include_bytes!` in `adapters/assets.rs`, so the shipped binary
kept the old rule and the ~34px safe-area pad still won on iPhone.

Source contract test passed against `.svelte`; Playwright cannot see
`env(safe-area-inset-bottom)` either — so CI stayed green while devices did
not get the fix.

## Fix

Rebuild and commit `web/dist/app.css` so the embedded shell includes:
`html.keyboard-open .terminal-panel.is-expanded … .terminal-keys { padding-bottom: 6px }`.

Add a dist-level source contract so this class of miss fails unit tests next
time.

## Non-goals

- No CSS logic change beyond shipping the already-merged source rule
- No Playwright geometry assertions for safe-area (still 0 there)

## Checklist

- [x] Confirm main `dist/app.css` lacks the expanded keyboard-open override
- [x] `npm run web:build:check` regenerates dist with the override present
- [x] Assert override string in rebuilt `dist/app.css` (unit test; RED on
  main dist, GREEN locally)
- [x] Focused web unit tests for the source + dist contract green (7/7)

Delegation decision: not delegated because one-file generated-asset rebuild
plus a tiny contract test (model-router LOCAL exception).

## Validation

- `npm run web:build:check` — passed
- `npm run web:test -- --run src/components/keyboardBandPin.test.ts` — 7/7
- main dist regex check — `false`; local — `true`
