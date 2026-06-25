# Stable Session Identity Reconciliation Plan

## Goal

Make stable Cockpit reconcile a registered task whose worktree now points at a
different `ajax/*` branch, instead of retaining the stale task identity and
reporting the current task's tmux session as missing.

## Task 1: Reproduce stale worktree identity during runtime refresh

- Test: Add a focused unit test in
  `crates/ajax-core/src/runtime_refresh.rs` where the registry contains an old
  task for a worktree, Git reports that same worktree on a renamed `ajax/*`
  branch, and tmux contains the session derived from the renamed branch.
- Expected failing behavior: Refresh keeps or evaluates the stale registered
  task before substrate recovery, causing a missing-session projection or
  otherwise failing to converge on only the renamed task identity.
- Implementation: Adjust runtime reconciliation so same-worktree branch
  replacement is recognized before stale identity status is exposed, preserving
  the live task/session derived from the current Git branch.
- Verification: Run the focused new unit test and the existing runtime-refresh
  orphan/stale-worktree tests.

## Task 2: Verify stable Cockpit behavior end to end

- Test: Use the existing stable registry/worktree state as a behavioral check;
  no test file under `tests/` will be modified.
- Implementation: No additional production change unless Task 1 exposes a
  boundary-specific defect in Cockpit refresh.
- Verification: Run the relevant `ajax-core` test target, restart the stable
  web process, request a fresh Cockpit payload, and confirm the worktree maps to
  the current task identity and a present tmux session.

