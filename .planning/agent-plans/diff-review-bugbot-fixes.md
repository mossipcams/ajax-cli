# Bugbot Diff Review fixes

Mode: Small Fix / Behavior Change (from Bugbot review).
Status: in progress.

## Delegation decision

`Delegation decision: not delegated because` model-router would `R-STOP`
(multi-bounded: RuntimeBridge persistence API + core git status + DiffReview
gesture guard). Parent implements the three review-ordered fixes as one
coherent set before PR.

## Scope

1. Persist `ajax_pull_requests` through `RuntimeBridge` → SQLite when observation
   changes metadata (same pattern as `acknowledge_operator_input`).
2. Local `git diff` non-zero status → `Unobservable` (not empty file list).
3. Exclude PR chip strip and hunk surfaces from Diff Review swipe-back.

## Non-goals

- Ship-time PR append (separate follow-up).
- Dashboard swipe-reveal restore.

## Task checklist

- [x] Failing/characterization tests for the three regressions
- [x] RuntimeBridge persist hook + CliRuntimeBridge impl + wire GET handlers
- [x] Core local diff status_code check
- [x] DiffReview gesture ignore for chips/hunks
- [ ] Focused validation
- [ ] Full `npm run verify` then PR

## Validation results

```
cargo test -p ajax-core --lib diff_review  # 7 passed
cargo test -p ajax-web --lib diff_review   # 5 passed
cargo test -p ajax-cli --lib persist_registry_snapshot_writes_diff_review  # 1 passed
npm run web:test -- --run DiffReview navigateSwipe  # 10 passed
npm run web:check  # passed
```
