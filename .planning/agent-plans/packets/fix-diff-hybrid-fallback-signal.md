PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

When Diff Review silently falls back from PR patch to local `base...HEAD`,
surface that to the client instead of looking like the selected PR.

`project_task_diff` recursively returns `source: Local` on PR errors
(`diff_review.rs` ~334-347) while the UI still highlights the PR chip.

Add explicit fallback metadata on the projection/DTO (e.g.
`fell_back_from_pr: Option<u64>` or equivalent), set it on hybrid fallback,
serialize through the web slice, and show a short banner in `DiffReview` when
set. Do not change when fallback triggers.

## Allowed files

- `crates/ajax-core/src/diff_review.rs`
- `crates/ajax-web/src/slices/diff_review.rs`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- `crates/ajax-web/web/src/shared/lib/types.ts`

## Forbidden changes

- Changing optimistic/`run_read` concurrency
- Swipe / navigate gestures
- Removing hybrid fallback
- Commits, pushes, branch changes

## Acceptance

1. Hybrid PR→local fallback sets observable metadata that the selected/requested
   PR was unavailable and local diff was used.
2. JSON Diff DTO exposes that field; TS `TaskDiffView` types it.
3. DiffReview shows a visible banner/note when fallback metadata is present
   (testid e.g. `diff-fallback-banner`).
4. Core unit test covers fallback metadata; DiffReview test covers banner.
5. Explicit force-local / empty-PR local path does not claim fallback-from-PR.

## Constraints

- Smallest field addition; keep `DiffSource::Local` / `Pr` semantics.
- Estimated scope ≤ ~120 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-core --lib diff_review -- --nocapture
      expected: fallback metadata test passes
    - type: test
      command: cargo test -p ajax-web --lib diff_review -- --nocapture
      expected: slice/DTO tests pass
    - type: test
      command: npm run web:test -- --run DiffReview
      expected: banner test passes; existing DiffReview tests pass
  reason: Proves fallback is visible end-to-end without changing fallback triggers.
```

## Stop if

- Requires architecture change to remove hybrid fallback entirely
- Patch would exceed ~400 changed lines
