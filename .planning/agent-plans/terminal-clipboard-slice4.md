# Plan: Terminal clipboard UI (Slice 4)

## Scope

Extract paste/copy UI *state transitions* from `TerminalRawView.svelte` into
`terminalClipboard.ts`. Keep Ghostty `term.paste` / selection-manager writes /
gesture geometry in existing owners. Document in TERMINAL.md.

## Non-goals

- Changing clipboard UX behavior
- Moving selection cell math (stays in terminalGestures)
- Layout / scroll-follow / zero-lag edits

## Delegation decision

`Delegation decision: delegated via model-router` → cursor-delegate /
composer-2.5

## Task checklist

- [x] Packet + failing unit tests for clipboard UI state
- [x] Implement + wire TerminalRawView
- [x] Update paste source-contract test to new owner
- [x] TERMINAL.md row
- [x] Parent validate

## Validation

```bash
npm run web:test -- --run src/terminalClipboard.test.ts src/components/TerminalRawView.test.ts
npm run web:check
npm run web:build
```

### Results (delegate + parent)

- PASS clipboard + TerminalRawView (150)
- PASS web:check
- Accepted: state in `terminalClipboard.ts`; svelte keeps paste/copy effects only.
- Note: `takePasteFallbackText` trims (packet-specified); behavior tests still green.

### Results (2026-07-11)

- `terminalClipboard.test.ts`: 9 passed
- `TerminalRawView.test.ts`: 141 passed
- `web:check`: 0 errors
- `web:build`: success
- `rg flashCopyNotice|copyNoticeTimer` in TerminalRawView.svelte: gone (only thin `dismissCopyUi` wrapper remains)
