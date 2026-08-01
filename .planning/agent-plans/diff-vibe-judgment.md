# Diff Review vibe-judgment projection

Mode: Behavior Change.
Status: in progress.

## Delegation decision

`Delegation decision: not delegated because` the approved plan is multi-slice
(core + DTO + UI + architecture) and exceeds one bounded behavior; parent
implements as one coherent change set under the approved plan (R-STOP /
R-SIZE-SPLIT for multi-bounded work). Slice-1 packet remains on disk for
reference.

## Scope

- Core: `DiffJudgment` (totals, reading_order, flags) + `assess_diff_judgment`
- Web DTO + TS passthrough
- DiffReview UI: orientation, flags, guide chips
- architecture.md Diff Review judgment contract

## Non-goals

- LLM summary
- Ship/approve/comments
- AST semantic analysis
- CI observation on Diff Review
- Persist judgment in task metadata

## Task checklist

- [x] Slice 1 — core judgment (TDD)
- [x] Slice 2 — web DTO/TS passthrough
- [x] Slice 3 — DiffReview UI
- [x] Slice 4 — architecture.md + validation
- [x] Parent focused validation

## Validation

```bash
cargo test -p ajax-core --lib diff_review   # 14 passed
cargo test -p ajax-web --lib diff_review    # 7 passed
npm run web:test -- --run DiffReview        # 12 passed
npm run web:check                           # passed
npm run verify                              # passed (exit 0)
```

## Follow-ups

- [x] Restore DiffReview fetch fallback after Bugbot review
- [x] Local verify gate
- [x] Open PR — https://github.com/mossipcams/ajax-cli/pull/726

Status: complete.
