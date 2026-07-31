# Diff Review: signal/noise + auto-open top file

Mode: Behavior Change.
Status: in progress.

## Product decision (post pressure-test)

Functionality improvement to Diff Review itself (not Guide strip, not Ship/risk chrome):

1. Core labels each `DiffFile` as `signal` or `noise` via deterministic path heuristics.
2. Projection keeps `files` in parse order (never drop/reorder the array).
3. Browser sorts the **file list** signal-first (noise collapsed behind expand).
4. On ready load, auto-open the top signal file’s hunks (skip empty list when there is signal).

## Delegation decision

`Delegation decision: delegated via model-router` (pending packet + dispatch).

## Scope

- `ajax-core` role classification on `DiffFile`
- DTO/TS passthrough of `role`
- `DiffReview.tsx` sort + collapse + auto-open

## Non-goals

- Guide strip / reading-order carousel
- LLM summary
- Reordering or filtering `files` in the projection
- Persist guide/role into task metadata
- CI chips / Ship / nudge actions

## Task checklist

- [x] Persistent packet READY + check-packet
- [x] Delegate implementation (GLM 429 → escalated to Cursor Composer 2.5)
- [x] Parent review gate + focused validation
- [x] Update this ledger with results

## Validation

```bash
cargo test -p ajax-core --lib diff_review   # 9 passed
cargo test -p ajax-web --lib diff_review    # 5 passed
npm run web:test -- --run DiffReview        # 8 passed
npm run web:check                           # passed
```

## Review gate

`ACCEPT` — acceptance met; out-of-scope touch to `diff-review-bugbot-fixes.md` reverted.
Delegate report schema invalid (missing FILES_CHANGED) but delta present and parent-verified.
