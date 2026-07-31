# Diff Review: long-press + swipe-right entry

Mode: Behavior Change.
Status: in progress.

## Product

- Task detail: **long-press then swipe right** opens Diff Review.
- Quick swipe left or right on task detail: **no navigation**.
- Diff Review: quick swipe does nothing; **long-press then swipe left** returns (mirror). Back button unchanged.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Packet READY
- [x] Delegate + parent review
- [x] Focused tests green
- [ ] Push to PR #716 if still open

## Validation

```bash
npm run web:test -- --run navigateSwipe TaskDetail DiffReview
npm run web:check
```
