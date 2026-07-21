# Fix CI rustdoc private-intra-doc-links

## Scope

- Remove the private `[`recognize`]` intra-doc link from public `project_pane_activity` docs so `cargo doc` passes under `-D warnings`.

## Non-goals

- No behavior changes
- No new tests (docs-only; rustdoc is the check)
- No architecture changes

## Approval

- Approved by user 2026-07-21 after CI failure summary on PR #626.

## Delegation decision

- `Delegation decision: not delegated because one-line docs-only correction smaller than a work order`

## Tasks

- [x] Rewrite `project_pane_activity` doc to avoid linking private `recognize`
- [x] Run `cargo doc -p ajax-core --no-deps` and confirm green

## Validation

```bash
RUSTDOCFLAGS="-D warnings" cargo doc -p ajax-core --no-deps
```

Result: exit 0 — Documenting ajax-core succeeded.

## Deviations

(none)

## Results

- Fixed `crates/ajax-core/src/live.rs` doc on `project_pane_activity`.
- Local rustdoc with `-D warnings` passes.
- Not committed/pushed (user did not request).
