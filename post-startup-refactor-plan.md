# Post-Startup Performance and Reliability Refactor Plan

This plan turns the post-startup review findings into vertical slices. It is
test-led: explicit regression tests come first, then the implementation slices
that make those tests pass.

## Progress (2026-06-05)

| Slice | Status | Notes |
| --- | --- | --- |
| 9 Performance harness | **Done** | `CountingCommandRunner` exported; `context_with_many_active_tasks(24)` + bounded tmux budget test. |
| 1 Cheap cockpit polling | **Done** | `/api/cockpit` uses `RefreshTier::Live`; route test asserts tier. |
| 2 Status hydration | **Done** | `TmuxAgentStatusSnapshot` reads cache once per refresh. |
| 3 Pane / detail | **Done** | Server pane freshness window + adaptive `paneInterval()` + regression tests. |
| 4 Web concurrency | **Done** | Timeouts on probe commands; pane + cockpit routes release lock during external work; health/detail concurrency tests. |
| 6 Ship safety | **Done** | `refresh_ship_plan_before_execute` in core; stale-cache ship test blocks dirty refresh. |
| 7 Tidy safety | **Done** | Batched observation in sweep; `TeardownIncomplete` + command-count tests. |
| 8 Tmux identity | **Done** | Exact registered-session lookup; hyphenated-repo + orphan-gate tests. |
| 5 Persistence | **Done** | `TrackedContext` + merge on disk mtime; native/web merge + conflict tests. |

**Validation:** `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo nextest run --all-features` pass (1377 tests).

**Docs:** `architecture.md` updated for refresh tiers, web lock pattern, persistence merge, and core ship/tidy preflight.

Legend for test matrix below: **done**

## Explicit Test Matrix

All matrix items below are **done**. See git history and crate tests for assertions.

### Cockpit polling and cheap snapshots

- **done** Test: normal `/api/cockpit` polling uses cheap refresh, not `RefreshTier::Full`.
- **done** Test: steady `/api/cockpit` polling does not run orphan worktree discovery.
- **done** Test: steady `/api/cockpit` polling does not capture every task pane by default.

### Runtime status hydration

- **done** Test: agent status cache scans pane status files once per refresh.
- **done** Test: populated agent status cache prevents pane fallback.
- **done** Test: many active tasks use at most one `tmux list-sessions` and one `tmux list-windows -a`.

### Pane and detail interaction

- **done** Test: repeated `/api/tasks/{handle}/pane?since=current` requests avoid redundant `tmux capture-pane` inside a short freshness window.
- **done** Test: browser pane polling backs off when unchanged or idle.
- **done** Test: task detail polling does not require rebuilding full live cockpit state.

### Web runtime concurrency and command bounds

- **done** Test: slow pane capture does not block `/api/health`.
- **done** Test: refresh/status/pane shell-outs return a bounded timeout error.

### Persistence coordination

- **done** Test: native Cockpit final save preserves web companion changes.
- **done** Test: stale save conflict is explicit when merge is ambiguous.

### Ship operation safety

- **done** Test: Web Cockpit `ship` refreshes git evidence before planning merge.
- **done** Test: CLI and Web Cockpit ship share the same core preflight path.

### Cleanup and tidy

- **done** Test: `tidy` leaves `TeardownIncomplete` when final observation finds a remaining resource.
- **done** Test: tidy observations are batched across candidates.

### Tmux identity

- **done** Test: hyphenated repo names do not trigger false orphan discovery.
- **done** Test: exact expected session names drive session matching.

## Refactor Slices

All nine slices are **done**. See `architecture.md` for the resulting boundaries.

## Suggested Execution Order

All items complete.

## Validation

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```
