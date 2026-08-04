# CI fix: File LOC on runtime_refresh.rs

## Failure

PR: https://github.com/mossipcams/ajax-cli/pull/750  
Check: **File LOC** (FAILED)  
Run: https://github.com/mossipcams/ajax-cli/actions/runs/30866254954/job/91858675053

```
crates/ajax-core/src/runtime_refresh.rs is 1006 lines (limit 1000). Split the file before merging.
```

Other changed files only warn (≥600). No other hard failures from this job.

**CI** (aggregate gate) also fails solely because File LOC failed
(`A required CI job did not pass (result: failure)`). All other required jobs
passed: Format, Clippy, Cargo Check, Nextest, Web, Documentation, Cargo Audit,
PR Title, CodeQL. Socket Security: SUCCESS (external).

## Root cause

PR1 alive-stamp edits pushed `runtime_refresh.rs` 6 lines over the hard max. Tests are already peeled (`#[cfg(test)] mod tests;` → `runtime_refresh/tests/`).

## Proposed fix (smallest)

Peel GitHub CI probe helpers into a sibling module under the existing directory:

1. Create `crates/ajax-core/src/runtime_refresh/github_checks.rs` with:
   - `refresh_github_check_evidence`
   - `github_probe_is_retired`, `should_probe_github_checks`
   - `apply_github_checks_observation`, `clear_github_ci_evidence`
   - `can_apply_github_override`, `is_unacknowledged_attention_gate`
   - `is_github_ci_failure`, `is_github_owned_ci`, `is_local_check_failure`
   - any tiny helpers those need (`unix_seconds` if only used there)
2. Convert `runtime_refresh.rs` to a thin module root **or** keep it as the main file and `mod github_checks;` from a `runtime_refresh/mod.rs` layout matching prior crate splits.
   - Prefer the pattern already used nearby (e.g. other ajax-core splits): if `runtime_refresh.rs` + `runtime_refresh/tests/` coexist, use `#[path]` / `mod github_checks { include! }` **or** rename to `runtime_refresh/mod.rs` + move body — follow the smallest existing pattern in this crate.
3. Keep public API unchanged (`refresh_runtime_context*`, `RefreshTier`, `AgentStatusSource`).
4. Do **not** split warning-only files in this pass unless LOC still fails.

## Validation

```bash
node scripts/check-file-loc.mjs   # expect 0 errors for this branch vs main
cargo nextest run -p ajax-core live_refresh_skips_github process_alive_stamp
cargo check -p ajax-core
```

## Non-goals

- Broad refactor of `refresh_runtime_context_with_tier`
- Fixing ≥600 warnings on other files
- Write-batching leftovers

## Approval

Approved by user 2026-08-03. Implementing.

## Checklist

- [x] Peel github helpers into `runtime_refresh/github_checks.rs`
- [x] `mod github_checks;` + wire call site
- [x] `FILE_LOC_*` check — 0 errors (`runtime_refresh.rs` now 809)
- [x] Focused ajax-core tests — 33 passed (github/ci/alive/live_refresh filters)
- [ ] Commit + push to PR branch

## Delegation decision

`Delegation decision: not delegated because mechanical LOC peel with approved CI fix plan; a relocate patch would trip R-SIZE-SPLIT (~400 churn lines) while net behavior is unchanged.`

## Results

Peel complete. Awaiting commit/push.
