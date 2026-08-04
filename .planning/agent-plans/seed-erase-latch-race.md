# Fix far scrollback after seed-window latch

## Scope

Restore seeded history when scrolling far back. Latch `scrollOnEraseInDisplay`
off only after seed reveal, on the first post-reveal erase — not at reveal time
(race with attach `CSI 2 J`).

## Non-goals

- Do not restore permanent scrollOnErase
- Do not reintroduce server pad
- Do not change File LOC limits

## Delegation decision

`Delegation decision: not delegated because urgent latch-race hotfix (~15 lines;
parent owns RCA from live bridge timing)`

## Checklist

- [x] Task 1 — Latch off on first post-reveal erase; remove revealSeed latch-off
- [x] Task 2 — Update source-assert + contract; focused verify
- [ ] Task 3 — Rebuild dist; push if checks green

## Validation

```bash
npm run web:test -- --run …TaskTerminal.test.tsx …scrollbackOverwriteProbe.test.ts  # 36 pass
npm run web:check && npm run web:lint  # pass
wc -l TaskTerminal.tsx  # 984
```

## Deviations

None.
