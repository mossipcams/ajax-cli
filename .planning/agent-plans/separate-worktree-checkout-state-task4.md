ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/models.rs, crates/ajax-core/src/ui_state.rs, crates/ajax-core/src/operation.rs, crates/ajax-core/src/policy.rs, crates/ajax-core/src/commands/open.rs, crates/ajax-core/src/commands/diff.rs, crates/ajax-core/src/commands.rs, crates/ajax-core/src/task_operations.rs]
  REASON: The architecture-sensitive backend lane was previously unavailable for this goal, so the bounded READY packet uses the user-requested Cursor fallback.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Keep present checkout-mismatched worktrees usable for Open, Check, and Review,
while blocking Merge, Clean, Remove, and Drop with the canonical
expected-versus-observed checkout detail before any destructive command or
resource observation runs.

## Allowed files

- `crates/ajax-core/src/models.rs`
- `crates/ajax-core/src/ui_state.rs`
- `crates/ajax-core/src/operation.rs`
- `crates/ajax-core/src/policy.rs`
- `crates/ajax-core/src/commands/open.rs`
- `crates/ajax-core/src/commands/diff.rs`
- `crates/ajax-core/src/commands.rs`
- `crates/ajax-core/src/task_operations.rs`

## Forbidden changes

- Do not edit any other file or undo accepted Tasks 1–3.
- Do not run Git checkout/switch, worktree, branch, merge, cleanup, commit,
  push, rebase, or branch-changing commands.
- Do not add UI-specific mismatch guards or let adapters derive branch policy.
- Do not allow Merge, Clean, Remove, or Drop to proceed on a named or detached
  mismatch, even with confirmation.
- Do not block Open, Check, or Diff merely because the present worktree is on a
  different branch or detached.
- Do not change missing-worktree or missing-branch repair semantics.
- Do not add dependencies, fields, schema changes, or generic abstractions.
- Do not delete or weaken assertions. Where the existing other-branch Drop
  test encodes the superseded behavior, convert it into the stronger assertion
  that planning and execution are blocked before resource observation and no
  branch/worktree/tmux command runs.
- Do not perform unrelated cleanup or formatting churn.

## Context evidence

- `Task::has_checkout_mismatch` already derives mismatch only when the
  registered worktree path is present and its observed branch differs from
  task intent; detached `current_branch: None` is a mismatch.
- `ui_state::canonical_checkout_mismatch_explanation` owns the accepted exact
  wording but is private projection logic. A core operation guard also needs
  that wording, so the smallest shared source is a concrete `Task` method that
  the UI and operation/safety code reuse. UI precedence and operation safety are
  separate concerns: an unrelated missing tmux/window resource must not suppress
  a fresh Git checkout mismatch in branch-sensitive operations.
- `task_operation_eligibility` currently allows mismatched Merge, Clean, and
  Remove. `merge_safety` and `cleanup_safety` also omit mismatch.
- `open_task_plan` has an explicit physical-presence check that conflates
  `branch_exists` with worktree presence after eligibility has already handled
  independent missing substrate.
- `check_task_plan` already runs in `task.worktree_path` and does not need a
  production change.
- `diff_task_plan` currently constructs `base...task.branch`; worktree-local
  Review must compare `base...HEAD` so it reviews the actual checkout.
- `plan_drop_confirmation` falls back from blocked Clean to Remove. Therefore
  both operation kinds must share the same mismatch guard or Drop could bypass
  it. `plan_drop_task_operation` already returns a blocked plan before external
  observation when the confirmation plan is blocked.
- The existing `drop_operation_does_not_remove_other_branch_at_expected_path`
  test permits deletion of the expected branch while the registered path is
  on another branch. This is precisely the unsafe behavior this task replaces.

## Code anchors

- `crates/ajax-core/src/models.rs`: `Task::has_checkout_mismatch`.
- `crates/ajax-core/src/ui_state.rs`:
  `canonical_checkout_mismatch_explanation`.
- `crates/ajax-core/src/operation.rs`: `task_operation_eligibility` and its
  inline tests.
- `crates/ajax-core/src/policy.rs`: `merge_safety`, `cleanup_safety`, and their
  inline tests.
- `crates/ajax-core/src/commands/open.rs`: the explicit Git status check in
  `open_task_plan`.
- `crates/ajax-core/src/commands/diff.rs`: the `range` construction.
- `crates/ajax-core/src/commands.rs`: Open/Check/Diff/Merge/Clean/Remove tests
  near `check_task_plan_runs_configured_command_in_task_worktree`.
- `crates/ajax-core/src/task_operations.rs`:
  `drop_operation_does_not_remove_other_branch_at_expected_path`.

## Test-first instructions

Make all test edits below before any production edit, then run every RED command
and capture its intended assertion failure.

1. In `commands.rs`, update all three existing worktree-local Diff expectations
   from `main...ajax/fix-login` to `main...HEAD`. Add
   `checkout_mismatch_keeps_open_check_and_review_available` using present Git
   evidence with `current_branch: Some("fix/pane-stuck")`. Assert:
   Open has no blocker, Check runs the configured command at the registered
   path, and Diff runs `git diff --stat main...HEAD` at that path.
2. In `operation.rs`, add
   `branch_sensitive_checkout_mismatch_operations_are_blocked_with_details`.
   Cover named and detached present mismatches. Assert Open, Check, and Diff are
   Allowed. With a compatible lifecycle, assert Merge, Clean, and Remove are
   Blocked and contain exactly the canonical detail for the mismatch:
   `Worktree on fix/pane-stuck; expected ajax/fix-login` or
   `Worktree detached; expected ajax/fix-login`. Also assert a genuinely missing
   worktree still reports the existing missing-substrate reason rather than a
   mismatch reason. On the named mismatch, additionally set `TmuxMissing` and
   prove Merge remains blocked by the checkout detail even though
   `has_missing_substrate()` is true for the unrelated tmux gap.
3. In `policy.rs`, add
   `branch_sensitive_checkout_mismatch_safety_is_blocked_with_details`. Assert
   both `merge_safety` and `cleanup_safety` classify named and detached mismatch
   as `Blocked` and include the exact canonical detail. Include a named mismatch
   carrying `TmuxMissing` and prove the safety blocker remains. Preserve the
   existing missing-worktree safety tests.
4. In `task_operations.rs`, convert the superseded
   `drop_operation_does_not_remove_other_branch_at_expected_path` behavior into
   `branch_sensitive_checkout_mismatch_drop_stops_before_observation`. Arrange
   cached present Git evidence on `dependabot/pip/minor`, use an empty recording
   runner, and assert planning returns the exact blocker plus an all-Unknown
   observation without invoking the runner. Assert execution returns
   `CommandError::PlanBlocked`, task intent is unchanged, and the runner still
   contains no worktree removal, branch deletion, tmux kill, or other command.
5. Run these RED commands before production edits:
   - `cargo test -p ajax-core checkout_mismatch_keeps_open_check_and_review_available -- --nocapture`
   - `cargo test -p ajax-core branch_sensitive_checkout_mismatch_operations_are_blocked_with_details -- --nocapture`
   - `cargo test -p ajax-core branch_sensitive_checkout_mismatch_safety_is_blocked_with_details -- --nocapture`
   - `cargo test -p ajax-core branch_sensitive_checkout_mismatch_drop_stops_before_observation -- --nocapture`
6. At least the Diff assertion, the branch-sensitive eligibility/safety
   assertions, and blocked Drop assertion must fail for the intended missing
   behavior. Do not proceed if a named test runs zero tests.

## Edit instructions

1. Add one concrete `Task::checkout_mismatch_explanation() -> Option<String>`
   beside `has_checkout_mismatch`. The method formats mismatch evidence; it must
   not call `has_missing_substrate()` because that would let an unrelated
   tmux/window gap hide a dangerous Git mismatch. Return `None` unless fresh Git
   evidence or `RuntimeHealth::CheckoutMismatch` marks a mismatch. Return the
   exact accepted named/detached wording above.
2. Make `ui_state::canonical_checkout_mismatch_explanation` preserve its own
   early `has_missing_substrate()` precedence check, then reuse the Task method;
   do not change its public result.
3. In `task_operation_eligibility`, when the operation is `Merge`, `Clean`, or
   `Remove` and `task.has_checkout_mismatch()` is true, append the shared
   explanation. The fresh-evidence predicate prevents stale mismatch health
   from adding a second reason to a genuinely missing worktree. Do not add the
   blocker to Open, Check, Diff, Refresh, or Recover.
4. In `merge_safety` and `cleanup_safety`, use the same fresh-evidence predicate
   and mark the shared explanation as `SafetyClassification::Blocked`. Keep all
   existing safety evidence and reasons intact.
5. In `open_task_plan`, remove `branch_exists` only from the local explicit
   physical-worktree check. Do not bypass the independent eligibility handling
   for a genuinely missing branch.
6. Change Diff range construction to exactly `<base_branch>...HEAD`.
7. Rely on the shared Clean and Remove eligibility guard to stop the Drop
   fallback before observation; do not add a one-off Drop-only guard.
8. Make the smallest changes necessary. Do not refactor unrelated operation or
   policy code.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core checkout_mismatch_keeps_open_check_and_review_available -- --nocapture`
2. `cargo test -p ajax-core branch_sensitive_checkout_mismatch_operations_are_blocked_with_details -- --nocapture`
3. `cargo test -p ajax-core branch_sensitive_checkout_mismatch_safety_is_blocked_with_details -- --nocapture`
4. `cargo test -p ajax-core branch_sensitive_checkout_mismatch_drop_stops_before_observation -- --nocapture`
5. `cargo test -p ajax-core diff_task_plan -- --nocapture`
6. `cargo test -p ajax-core missing_worktree -- --nocapture`
7. `cargo test -p ajax-core checkout_mismatch -- --nocapture`
8. `cargo check -p ajax-core --all-targets`
9. `cargo fmt --check`
10. `git diff --check`

## Acceptance criteria

- Open, Check, and Diff remain available for a present named or detached
  checkout mismatch.
- Diff always reviews `<base>...HEAD` in the registered task worktree.
- Merge, Clean, Remove, and Drop block named and detached mismatches with exact
  expected/observed detail and no destructive commands.
- Drop cannot bypass a blocked Clean plan through the Remove fallback and does
  not observe external resources when already blocked.
- The canonical UI explanation and operation blocker share one core Task
  method, with missing substrate taking precedence.
- A genuinely absent worktree still follows existing missing-worktree safety
  and repair behavior.
- Only allowed files changed and every focused plus grouped command passes.

## Stop conditions

- Stop if safe Open/Check/Diff would require changing task identity, lifecycle,
  path, session, or branch intent.
- Stop if blocking Drop requires duplicating a UI or Drop-specific policy guard
  instead of fixing shared core eligibility.
- Stop if a genuine missing-worktree repair regression appears.
- Stop on unrelated baseline failures without changing unrelated code/tests.
- Return the exact `DELEGATE_REPORT` schema inside marker lines as plain YAML.
  `FILES_CHANGED`, `STOP_CONDITIONS_HIT`, and `REMAINING_RISKS` must each be
  inline bracket lists so the router checker accepts them.

## Revision gate findings

- Round 1 was rejected and deterministically restored. Its Task method called
  `has_missing_substrate()`, so a simultaneous `TmuxMissing` flag suppressed the
  checkout blocker while Merge intentionally ignores missing tmux substrate.
  That could permit a wrong-branch merge.
- Round 1 modified this packet with a false delegation ledger and returned a
  custom report without `SUMMARY`, `TEST_FIRST`, or `COMMAND_EVIDENCE`; no
  auditable RED/GREEN evidence was accepted.
- This is the only revision round. Do not edit this packet or any run artifact.
  Run the four tests red before production edits and return exactly:

```yaml
---DELEGATE_REPORT_START---
DELEGATE_REPORT:
  STATUS: COMPLETE
  SUMMARY: <one sentence>
  FILES_CHANGED: [<allowed source paths>]
  TEST_FIRST: PROVEN
  COMMAND_EVIDENCE:
    - PHASE: RED
      COMMAND: <exact focused command>
      EXIT_CODE: <nonzero>
      OUTPUT_EXCERPT: <intended assertion failure>
    - PHASE: GREEN
      COMMAND: <same focused command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing test result>
    - PHASE: VERIFY
      COMMAND: <each remaining verification command; add entries as needed>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---
```

## Parent gate result

- Status: accepted after one rejected/restored round and one revised round.
- Delegate TDD evidence: all four focused tests exited 101 for the intended
  missing behavior before production edits, then exited 0 after implementation.
- Parent verification: all ten packet commands exited 0.
- Scope: only the eight allowed source files changed; generated run artifacts
  were removed after review.
- Report caveat: the revised raw report contained the complete required schema
  but wrapped the marker envelope in a Markdown fence, so the adapter classified
  it as `MISSING_STRUCTURED_REPORT`. The parent did not rely on its pass claims.
