# Restore dashboard design to PR 672

## Scope
Restore the post-#672 / post-#676 calm **Active / Idle TaskList** design (no MusterBar, no attention-band row controls, no control-panel rebuilds).

## Keep (post-672 Ajax improvements)
- Terminal links: #686, #689, #690
- Terminal reconnect: #692 web + #694 Rust
- Terminal scroll/load behavior from #672 itself and later terminal fixes on main
- Unknown status contract #682, agent wait #704, ACP removal #701
- Hotbar / Drop-pill / chrome fixes that live outside TaskList
- `state.ts` helpers (quiet, unknown, fleetSegments) kept for compatibility

## Drop (design-only after 672)
- #677 MusterBar / fleet gauge
- #684–#685 / #691 / #693 later row & attention-band dashboard UX
- #696–#708 control-panel / peg-rail / MusterBar restore series (design surface)

## Delegation decision
`Delegation decision: not delegated because mechanical design restore from known commit`

## Checklist
- [x] Restore TaskList + tests + swipe e2e from #672
- [x] Remove MusterBar component + muster CSS
- [ ] Commit / verify / open PR
