# Plan: Terminal layout policy (Slice 1)

## Scope

Replace `pinchFlushPending` / `expandFlushPending` one-shot flags in
`TerminalRawView.svelte` with a pure `terminalLayoutPolicy.ts` that owns
fit/resize permission. Keep Ghostty/tmux; no product rewrite.

## Non-goals

- Scroll-follow, zero-lag, paste/copy extraction (later slices)
- CSS/chrome redesign
- Weakening TerminalRawView behavior tests

## Delegation decision

`Delegation decision: delegated via model-router` → BUILD_PACKET then
`cursor-delegate` / `composer-2.5` (frontend UI, bounded files).

## Approval

User approved attached plan execution 2026-07-11.

## Task checklist

- [x] Inventory via ast-grep (+ Serena where available)
- [x] Packet READY + unit tests for policy (TDD)
- [x] Wire TerminalRawView; delete FlushPending flags
- [x] Update TERMINAL.md + architecture.md
- [x] Validate web tests/check/build; ast-grep zero FlushPending

## Deviations

- Serena CLI has no symbol find/reference subcommands here; inventory used
  ast-grep + rg.
- Parent review: `sendResize` uses one decision read; expand exit goes through
  `endExpandFlush` slot (template cannot see mount-scoped `layoutPolicy`).
- Component keeps `expandRewrapTimer` for refit-only re-schedule at
  `EXPAND_REWRAP_MS`; permission lifetime is owned by the policy.

## Validation

- PASS `npm run web:test -- --run src/terminalLayoutPolicy.test.ts src/components/TerminalRawView.test.ts` (149)
- PASS `npm run web:check`
- PASS `npm run web:build`
- PASS ast-grep `$XFlushPending` — no production matches; only
  `terminalOwnership.test.ts` asserts the anti-pattern docs still mention it
