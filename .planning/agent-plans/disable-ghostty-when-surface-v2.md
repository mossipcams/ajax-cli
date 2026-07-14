# Disable Ghostty while Terminal Surface V2 is enabled

## Scope
When `ajax.terminal.surfaceV2` is on, do not mount or preload Ghostty.
Show wterm-only (or a failure banner without Ghostty fallback).

## Non-goals
- Removing Ghostty as the default when the flag is off
- Changing Rust protocol

## Delegation decision
`Delegation decision: not delegated because focused hotfix smaller than a
packet/delegate round.`

## Checklist
- [ ] Selector never mounts TerminalRawView while V2 is enabled
- [ ] warmTerminalAssets skips Ghostty when V2 is enabled
- [ ] Tests updated
- [ ] Validate + push
