# Fix occupied worktree repair

## Scope

When fresh Git evidence finds the task's expected worktree path occupied by a
different branch, preserve that branch in the task evidence and make Repair
stop with a precise conflict instead of issuing a doomed `git worktree add`.

Non-goals: switching branches, moving/deleting worktrees, changing task intent,
adding a new runtime-health state, or changing unrelated repair behavior.

Delegation decision: delegated via model-router with a complete TDD
implementation packet; rerouted to Cursor at the user's request after the
initial Pi lane stalled. The parent will review the diff and run validation.

Approval status: approved by user on 2026-07-20.

## Task checklist

- [x] Task 1 — Handle an occupied expected worktree path.
  - Test: add one focused behavior test in `crates/ajax-core/src/commands.rs`
    that refreshes a task whose expected path is occupied by another branch,
    then proves Repair emits no `git worktree add` and reports the occupying
    branch/path conflict. Run it first and capture the expected failure.
  - Implementation: in `refresh_git_substrate_evidence`, retain the branch
    observed at the exact expected path even when it does not match task intent;
    in `task_window_repair_plan_with_open_mode`, block worktree recreation when
    that evidence shows the path is occupied by another branch.
  - Verification: rerun the focused test to green, then run
    `cargo test -p ajax-core`, `cargo fmt --check`, and
    `cargo clippy -p ajax-core --all-targets --all-features -- -D warnings`.
- [x] Task 2 — Create and validate the pull request.
  - Test: run the repository's blocking local PR gate (`npm prepare`,
    `npm run verify`, `cargo build --release -p ajax-cli`, and
    `cargo install --path crates/ajax-cli --locked --force`) unless the enabled
    commit hook proves it ran the equivalent suite successfully.
  - Implementation: commit the approved source and planning artifacts with a
    valid `fix(core): ...` message, push the existing task branch, and create or
    update its GitHub PR targeting the default branch.
  - Verification: confirm the PR title/type, URL, head/base branches, merge
    state, and wait until all GitHub checks reach a terminal state.

## Risks

- The conflict must remain non-destructive: Repair must not switch, remove, or
  overwrite the occupying worktree.
- A truly absent worktree with an intact expected branch must retain the current
  recreation behavior.
- Existing wrong-branch classification remains intact; this change only makes
  its evidence and Repair response coherent.

## Deviations

- Initial Pi/GLM dispatch stalled without a report, then wrote a partial patch
  while being interrupted. Review rejected it because it modified an existing
  assertion contrary to the packet. The snapshot restore removed only Pi's two
  source-file edits; the approved plan and packet were preserved.
- User explicitly rerouted implementation to Cursor.
- Cursor proved RED and GREEN for the new regression, but the full core suite
  exposed an existing assertion that encoded `current_branch = None` for the
  same wrong-branch path. The packet now permits strengthening that assertion
  to the observed occupying branch; no other existing assertion may change.

## Validation results

- RED: `rtk cargo test -p ajax-core repair_plan_blocks_when_expected_worktree_path_is_occupied_by_another_branch -- --nocapture` exited 101 because refreshed `current_branch` was `None` instead of the occupying branch.
- GREEN: the same focused command passed (1 test, 826 filtered out).
- `rtk cargo test -p ajax-core` passed (827 tests across 2 suites).
- `rtk cargo fmt --check` passed.
- `rtk cargo clippy -p ajax-core --all-targets --all-features -- -D warnings` passed with no issues.
- `rtk cargo check --all-targets --all-features` passed (194 crates compiled).
- `rtk git diff --check` passed.
- Delegate-only failures: Pi stalled and returned no report; both Cursor runs completed the requested work but their reports failed the router's schema parser. The parent reviewed both filesystem deltas and independently ran all validation above.
- Diagnostic-only failure: `router-log --help` exited 2 because that script has no help flag; its source was inspected and subsequent logging commands succeeded.
- PR preparation: the first `rtk npm prepare` failed because `node_modules`
  was absent and `husky` was unavailable. `rtk npm ci` installed the locked
  dependencies with zero vulnerabilities, and the following `rtk npm prepare`
  passed.
- Commit `32267d3` (`fix(core): block repair for occupied worktree path`)
  passed the enabled Husky pre-commit hook, including `npm run verify`,
  `cargo build --release -p ajax-cli`, and
  `cargo install --path crates/ajax-cli --locked --force`.
- Branch `ajax/tmux-worktree-missing` pushed to origin and PR #597 created:
  `https://github.com/mossipcams/ajax-cli/pull/597`, targeting `main` with a
  valid `fix(core): ...` title.
- The first `gh pr checks 597 --watch --interval 10` ended with a transient
  GitHub API connection reset. A direct `gh pr view` poll then confirmed all
  checks successful: PR Title, Release PR Bypass, Format, Web, Cargo Check,
  Clippy, Nextest and Doc Tests, Documentation, Cargo Audit, and aggregate CI.
