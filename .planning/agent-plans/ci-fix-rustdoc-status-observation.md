# CI fix: rustdoc StatusObservation link

## Scope

Fix Documentation CI failure on PR #591: broken intra-doc link in `live.rs`.

## Non-goals

- No behavior changes.

## Delegation decision

`Delegation decision: not delegated because R-LOCAL-TINY` — one doc link path fix.

## Checklist

- [x] Fix `[StatusObservation]` → `[crate::agent_status::StatusObservation]`
- [x] `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features -p ajax-core`
- [ ] Commit + push

## Validation

- `RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --all-features -p ajax-core` → OK
