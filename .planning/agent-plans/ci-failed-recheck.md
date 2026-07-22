# Plan: Fix sticky CI failed after push/commit

## Scope

- Clear github-sourced `CiFailed` when PR checks are `Pending`
- Re-probe github CI failure on a 30s interval (default stays 300s)
- Update `architecture.md` to match

## Non-goals

- No git SHA / ahead-based invalidation
- No adapter classification changes
- No UI/web changes
- No commit/PR unless asked

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Persistent plan recorded
- [x] TDD packet created and routed
- [x] RED: Pending-clear + faster-failed-interval tests (added; parent GREEN proven)
- [x] GREEN: Pending clear + 30s failed probe interval
- [x] architecture.md updated
- [x] Parent validation (nextest + clippy)

## Validation

```bash
cargo nextest run -p ajax-core github_pending_checks_clear github_ci_failure_reprobes github_healthy
cargo nextest run -p ajax-core runtime_refresh
cargo clippy -p ajax-core --all-targets -- -D warnings
```

## Results

- Focused nextest: 3 passed (exit 0)
- `runtime_refresh` nextest: 49 passed (exit 0)
- clippy ajax-core: exit 0
- Parent ran `cargo fmt` on `runtime_refresh.rs` (delegate left rustfmt drift); fmt check exit 0; focused tests still pass

## Deviations

- GLM/MiniMax unavailable (opencode Go weekly limit) → escalated to cursor-delegate `composer-2.5`
- Delegate report envelope failed schema validation (markdown-fenced YAML); delta was in scope
- Delegate added refresh-start `github_ci_failure` snapshot so pane fallback cannot skip the 30s reprobe; within allowed files and justified by mid-impl test flake
