ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/commands/context.rs, crates/ajax-core/src/registry.rs, crates/ajax-core/src/task_operations/task_command.rs, crates/ajax-core/src/task_operations.rs]
  REASON: The architecture-sensitive backend lane was previously unavailable for this goal, so the bounded READY packet uses the user-requested Cursor fallback.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Make Repair explicitly adopt the named branch currently checked out at a
present mismatched task worktree. Adoption must require confirmation, revalidate
the planned expected/observed pair at execution, mutate task branch intent
without rekeying task identity, reconcile derived status, record one substrate
event, and run no external command. Detached or stale evidence must not adopt.

## Allowed files

- `crates/ajax-core/src/commands/context.rs`
- `crates/ajax-core/src/registry.rs`
- `crates/ajax-core/src/task_operations/task_command.rs`
- `crates/ajax-core/src/task_operations.rs`

## Forbidden changes

- Do not edit any other file or undo accepted Tasks 1–4.
- Do not modify any file under a `tests/` directory; focused inline tests belong
  in the allowed source modules.
- Do not run or plan `git switch`, `git checkout`, worktree, branch, tmux, test,
  or any other external command for mismatch adoption.
- Do not automatically adopt during refresh or without explicit confirmation.
- Do not adopt detached HEAD, missing worktrees, an observed branch that changed
  after planning, or task intent that changed after planning.
- Do not rename/rekey task ID, handle, title, worktree path, tmux session/window,
  lifecycle, attempts, receipts, or existing history.
- Do not overload plan title text as machine state. Use one concrete typed
  optional branch-adoption payload on `CommandPlan`.
- Do not add a schema migration, dependency, generic mutation framework, new
  operator action, or Git command.
- Do not change existing missing-worktree/task-window Repair behavior.
- Do not delete or weaken assertions or perform unrelated cleanup.

## Context evidence

- `CommandPlan` currently carries title, external commands, confirmation, and
  blockers. A concrete optional adoption payload is needed so execution can
  verify the exact expected/observed pair rather than infer from a title or
  blindly use a stale plan.
- `repair_task_plan` currently always composes task-window repair and Check;
  mismatch adoption must return through a distinct early plan with no commands.
- `execute_task_command_operation` currently marks Repair check-started, runs
  its external plan, repairs task-window evidence, and marks check success.
  Mismatch adoption must take a distinct early execution path before those side
  effects.
- CLI and Web refresh Git evidence before Repair planning. Core execution still
  must compare the plan payload to current registry evidence immediately before
  mutation so changing registry evidence or intent invalidates the plan.
- `Registry` already centralizes mutations and `SubstrateChanged` events. A
  concrete default registry operation can update branch intent and derived
  evidence without forcing boilerplate into test registry implementations.
- `Task::apply_git_status` and `refresh_runtime_projection` provide the existing
  reconciliation path. The observed named checkout proves the newly adopted
  expected branch exists, so stale `BranchMissing` evidence must clear.
- Registry snapshots serialize the whole Task; no SQLite schema column or store
  implementation is required for `task.branch` persistence.

## Code anchors

- `crates/ajax-core/src/commands/context.rs`: `CommandPlan` and
  `CommandPlan::new`.
- `crates/ajax-core/src/registry.rs`: `Registry`, `InMemoryRegistry`,
  `refresh_task_annotations`, and inline registry tests.
- `crates/ajax-core/src/task_operations/task_command.rs`:
  `plan_task_command_operation`, `execute_task_command_operation`, and
  `repair_task_plan`.
- `crates/ajax-core/src/task_operations.rs`: existing fixtures plus
  `repair_operation_promotes_task_to_reviewable_on_check_success` and
  `repair_operation_records_tests_failed_on_check_failure`.

## Test-first instructions

Make all test edits before production edits. Run every named RED command and
capture its intended failure; a command that runs zero tests is not evidence.

1. In `registry.rs`, add
   `registry_adopts_branch_intent_and_reconciles_without_rekeying_task`.
   Create a task with expected `ajax/fix-login`, present current branch
   `fix/pane-stuck`, `BranchMissing` set, mismatch runtime projection, and one
   existing event. Call the concrete registry adoption operation. Assert:
   branch becomes `fix/pane-stuck`; ID/handle/title/path/session/window/lifecycle
   and all other intent fields are unchanged; existing event history remains;
   exactly one new `SubstrateChanged` event says
   `task branch adopted from ajax/fix-login to fix/pane-stuck`; branch evidence
   is present, `BranchMissing` clears, and reconciled runtime health is not
   `CheckoutMismatch`.
2. In `task_operations.rs`, add
   `checkout_mismatch_repair_plans_confirmed_branch_adoption_or_blocks_detached`.
   For a present named mismatch, assert the Repair plan title remains
   `repair task: web/fix-login`, has zero commands/blockers, requires
   confirmation, and has exactly
   `{ expected_branch: "ajax/fix-login", observed_branch: "fix/pane-stuck" }`.
   For detached evidence, assert zero commands, no adoption payload, and exact
   blocker `cannot adopt a detached worktree; switch to a branch and refresh`.
3. Add `checkout_mismatch_repair_adopts_without_external_commands`. Snapshot
   the whole task and existing events, plan named adoption, execute confirmed
   with an empty recording runner, and assert: outputs empty; `state_changed`;
   runner commands empty; only branch intent plus derived Git/runtime/annotation
   state changed; stable identity/lifecycle/session/path/attempt/history fields
   remain; existing events remain in order; one exact adoption substrate event
   is appended; checkout mismatch is cleared after reconciliation.
4. Add `checkout_mismatch_repair_rejects_stale_or_declined_adoption`. Prove an
   unconfirmed valid plan returns `ConfirmationRequired` with no command/event/
   branch change. Clone that plan, forcibly set its public
   `requires_confirmation` field to false, and prove adoption still returns
   `ConfirmationRequired` when `confirmed` is false: the adoption executor must
   enforce confirmation independently of mutable plan chrome. Then table-test
   current evidence changed to a different named branch, detached, missing
   worktree, and task intent changed since planning. Each confirmed execution
   must return `PlanBlocked` containing exact reason `checkout changed since
   repair was planned; refresh and retry`, invoke no command, append no event,
   and leave task intent unchanged.
5. Run these RED commands before production edits:
   - `cargo test -p ajax-core registry_adopts_branch_intent_and_reconciles_without_rekeying_task -- --nocapture`
   - `cargo test -p ajax-core checkout_mismatch_repair_plans_confirmed_branch_adoption_or_blocks_detached -- --nocapture`
   - `cargo test -p ajax-core checkout_mismatch_repair_adopts_without_external_commands -- --nocapture`
   - `cargo test -p ajax-core checkout_mismatch_repair_rejects_stale_or_declined_adoption -- --nocapture`
6. At least the missing registry operation/plan payload, current commandful
   Repair plan, and unguarded execution assertions must fail before production
   edits. Do not proceed if tests are filtered out.

## Edit instructions

1. In `commands/context.rs`, add one concrete serializable
   `BranchAdoptionPlan { expected_branch, observed_branch }` and an optional
   `CommandPlan.branch_adoption`. Derive the same equality/debug/serde traits as
   the plan. Use `#[serde(default, skip_serializing_if = "Option::is_none")]`
   so non-adoption plan JSON remains backward compatible. Initialize it to None.
2. Add `Registry::adopt_task_branch(&mut self, task_id, expected_branch,
   observed_branch)` as a default concrete registry operation. It must reject a
   task whose current intent/evidence does not match the exact pair, whose
   expected and observed names are equal, or which no longer has a fresh
   checkout mismatch, returning a useful existing `RegistryError` or the
   smallest new concrete error variant.
   On success, update `task.branch`, reconcile the existing Git evidence so the
   newly expected observed branch is known present and stale `BranchMissing`
   clears, refresh runtime projection/annotations, and append exactly one
   `SubstrateChanged` event with the exact message above. Do not remove history.
3. In `repair_task_plan`, before normal task-window/check planning, inspect the
   task. A fresh present named mismatch returns a zero-command plan with the
   typed payload and `requires_confirmation = true`. A fresh detached mismatch
   returns the exact blocker and no payload/commands. All non-mismatch Repair
   behavior falls through unchanged.
4. In `execute_task_command_operation`, detect Repair with a branch-adoption
   payload before marking check-started. Reject blocked/unconfirmed plans using
   existing command errors. Adoption must return `ConfirmationRequired` whenever
   `confirmed` is false even if a caller tampers with
   `plan.requires_confirmation`. Revalidate a genuine fresh mismatch, task
   intent, present worktree, and exact current named branch against the payload
   at execution. Map any mismatch to the exact stale-plan `PlanBlocked` reason.
   On success call the registry operation, return empty outputs and
   `state_changed = true`, and do not touch check/task-window lifecycle reducers
   or the runner.
5. Do not infer adoption solely from current mismatch at execution: only an
   explicitly typed, confirmed plan may adopt. Do not add a second operator
   action or a generic in-process mutation system.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core registry_adopts_branch_intent_and_reconciles_without_rekeying_task -- --nocapture`
2. `cargo test -p ajax-core checkout_mismatch_repair_plans_confirmed_branch_adoption_or_blocks_detached -- --nocapture`
3. `cargo test -p ajax-core checkout_mismatch_repair_adopts_without_external_commands -- --nocapture`
4. `cargo test -p ajax-core checkout_mismatch_repair_rejects_stale_or_declined_adoption -- --nocapture`
5. `cargo test -p ajax-core repair_operation -- --nocapture`
6. `cargo test -p ajax-core task_window_repair_plan -- --nocapture`
7. `cargo test -p ajax-core checkout_mismatch -- --nocapture`
8. `cargo check -p ajax-core --all-targets`
9. `cargo fmt --check`
10. `git diff --check`

## Acceptance criteria

- Named mismatch Repair produces a typed, zero-command, confirmation-required
  adoption plan naming exact expected and observed branches.
- Detached mismatch Repair is blocked with recovery guidance and cannot mutate.
- Confirmed execution revalidates the exact pair, adopts through the registry,
  runs no command, preserves task identity/lifecycle/path/session/history, and
  appends one exact substrate-change event.
- Declined or stale plans make no state change and run no command.
- Reconciliation clears checkout mismatch and stale missing-branch evidence
  without a schema migration.
- Existing missing-worktree, task-window, test-running, and check-failure Repair
  behavior remains green.
- Only the four allowed files change and every packet command passes.

## Stop conditions

- Stop if adoption requires a Git/tmux command, task rekey/rename, SQLite schema
  migration, or automatic refresh-time intent mutation.
- Stop if `CommandPlan` cannot carry a backward-compatible optional typed
  payload without changing unrelated plan JSON.
- Stop if existing non-mismatch Repair behavior must be weakened.
- Stop on unrelated baseline failures without changing unrelated code/tests.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or any prose before
  or after it. Every command needs its own evidence item.

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
      OUTPUT_EXCERPT: <intended failure>
    - PHASE: GREEN
      COMMAND: <same focused command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
    - PHASE: VERIFY
      COMMAND: <remaining command; add one item per command>
      EXIT_CODE: 0
      OUTPUT_EXCERPT: <passing result>
  STOP_CONDITIONS_HIT: []
  REMAINING_RISKS: []
---DELEGATE_REPORT_END---

## Revision gate findings

- Round 1 was rejected and deterministically restored. Its adoption executor
  checked `plan.requires_confirmation && !confirmed`, so a caller could clear
  the public plan bit while retaining the typed adoption payload and adopt with
  `confirmed = false`. Confirmation is an execution invariant, not display
  metadata.
- The revised test must tamper that bit and fail red before production code,
  then prove unconditional confirmation green. Both executor validation and
  registry mutation must also require a genuine current mismatch.
- Round 1's raw response contained the full requested report without fences,
  but the adapter still emitted `MISSING_STRUCTURED_REPORT`. This is the only
  revision round. Do not edit this packet or run artifacts, and return the exact
  plain marker envelope above.

## Parent gate result

- Status: accepted after one rejected/restored round and one revised round.
- Delegate TDD evidence: all four focused commands exited 101 before production
  edits, then exited 0 after implementation.
- Parent verification: all ten packet commands exited 0.
- Safety review: adoption confirmation is unconditional on the mutable plan
  flag; executor and registry both require a genuine current mismatch.
- Scope: only the four allowed source files changed; generated run artifacts
  were removed after review.
- Report caveat: the revised raw response contained the complete plain schema,
  but the adapter still classified it as `MISSING_STRUCTURED_REPORT`; parent
  verification, not delegate pass claims, supplied acceptance evidence.
