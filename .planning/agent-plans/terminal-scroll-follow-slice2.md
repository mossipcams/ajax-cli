# Plan: Terminal scroll-follow policy (Slice 2)

## Scope

Extract pinned/unseen scroll-follow *state* out of `TerminalRawView.svelte`
into a stateful owner in `terminalOutputPolicy.ts` (already named in
TERMINAL.md). Preserve behavior; no layout-policy or zero-lag changes.

## Non-goals

- Zero-lag / paste / copy (Slices 3–4)
- Layout policy changes
- Ghostty/tmux product changes

## Delegation decision

`Delegation decision: delegated via model-router` → cursor-delegate /
composer-2.5

## Task checklist

- [x] Packet + failing tests for scroll-follow state owner
- [x] Implement + wire TerminalRawView
- [x] Update TERMINAL.md if wording needs sharpening
- [x] Parent validate focused tests + web:check (+ build if svelte changed)

## Validation

```bash
npm run web:test -- --run src/terminalOutputPolicy.test.ts src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

### Results

- `npm run web:test -- --run src/terminalOutputPolicy.test.ts` — PASS (18 tests)
- `npm run web:test -- --run src/components/TerminalRawView.test.ts` — PASS (141 tests)
- `npm run web:check` — PASS (0 errors)
- `npm run web:build` — PASS
- `rg -n 'let pinnedToBottom' crates/ajax-web/web/src/components/TerminalRawView.svelte` — no matches

### Parent review

- Accepted: policy API matches packet; UI sync after pin/unpin/note/viewport/reconnect.
- Parent re-ran: outputPolicy + TerminalRawView 159 PASS; web:check PASS.
