# Revert #718 Diff swipe fix — reproduce before re-fixing

Mode: Planning-only / revert.
Status: in progress.

## Why

#718 merged but the swipe-open fix still failed in practice. Undo it, then
characterize the failure in mobile-webkit before another code change.

## Findings (mobile-webkit simulator)

```bash
npm run web:smoke -- e2e/diff-review-swipe-repro.test.ts
```

1. **Quick swipe-right does not open Diff** — current product requires long-press first.
2. **≥8px move during the hold cancels open** — easy to hit on a real thumb.
3. **Clean hold (~475ms) then swipe-right does open Diff** in the simulator.

Likely user-visible failure modes: expecting a plain swipe-right, or failing the
long-press stillness budget.

## Checklist

- [x] Revert commit for #718 on branch
- [x] Simulator repro suite
- [ ] Revert PR opened
- [ ] Do not ship a new fix until product decision: plain swipe-right vs hardened long-press
