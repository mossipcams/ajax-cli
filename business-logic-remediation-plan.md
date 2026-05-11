# Business Logic Remediation Plan

This plan resolves the current business logic issues end to end. The target
model is that every user action flows through one operation pipeline:

1. Check operation eligibility.
2. Collect fresh substrate evidence when the operation depends on external
   state.
3. Build a command plan.
4. Execute external steps.
5. Update registry state after each meaningful step.
6. Preserve recovery state on failure.
7. Keep affected tasks visible to the operator.

## Guiding Model

Ajax state should separate three concerns:

- Lifecycle: what Ajax thinks the task is for, such as `Provisioning`,
  `Active`, `Waiting`, `Reviewable`, `Mergeable`, `Merged`, `Cleanable`,
  `Removed`, or `Error`.
- Substrate evidence: what exists outside Ajax, such as the worktree, branch,
  tmux session, worktrunk window, git dirty state, conflicts, and unpushed
  commits.
- Operation state: what Ajax is currently doing or last failed doing, such as
  provisioning, opening, merging, cleaning, refreshing, or supervising.

The current implementation mixes these concerns. The remediation work should
centralize lifecycle transitions, make external evidence explicit, and derive
Cockpit actions from the same core operation policy used by CLI commands.

## Phase 0: Baseline Safety Harness

### Task 1: Add Lifecycle Fixture Builders

- Test to write: fixture smoke tests for representative task states.
- Code to implement: small test helpers in core test modules.
- Verify: run the targeted core fixture test.

### Task 2: Add Command-Flow Fixture Builders

- Test to write: queued runner models partial success and failure.
- Code to implement: reusable CLI test helpers if missing.
- Verify: run the targeted CLI helper test.

## Phase 1: Lifecycle Core

### Task 3: Define Allowed Lifecycle Transitions

- Test to write: transition matrix for valid and invalid lifecycle changes.
- Code to implement: `ajax-core` transition helper.
- Verify: run core lifecycle tests.

### Task 4: Block Terminal-State Regressions

- Test to write: `Merged`, `Cleanable`, and `Removed` cannot become `Active`
  through a generic transition.
- Code to implement: transition rules.
- Verify: run core lifecycle tests.

### Task 5: Allow Explicit Recovery Transitions

- Test to write: `Error -> Active` and `Error -> Reviewable` are allowed only
  through named recovery or result transitions.
- Code to implement: transition reason or operation parameter.
- Verify: run core lifecycle tests.

### Task 6: Replace `update_lifecycle` Direct Assignment

- Test to write: invalid registry lifecycle update returns a blocked result or
  error.
- Code to implement: use transition helper in the registry update path.
- Verify: run registry tests.

### Task 7: Replace Production Direct Lifecycle Mutations

- Test to write: command transition tests fail first where production code
  still assigns lifecycle directly.
- Code to implement: route open, merge, remove, and live updates through
  lifecycle helpers.
- Verify: run targeted command tests and inspect production code with
  `rg "lifecycle_status =" crates/*/src -g '*.rs'`.

## Phase 2: Operation Eligibility

### Task 8: Add Operation Enum

- Test to write: operation labels or debug expectations for
  `Create`, `Open`, `Trunk`, `Check`, `Diff`, `Merge`, `Clean`, `Refresh`, and
  `Recover`.
- Code to implement: core task operation enum.
- Verify: run the core operation test.

### Task 9: Add Operation Eligibility Result

- Test to write: eligibility returns allowed or blocked with explicit reasons.
- Code to implement: core operation policy result type.
- Verify: run the core operation policy test.

### Task 10: Gate Open

- Test to write: open is blocked for `Removed` and allowed for visible
  nonremoved states without lifecycle regression.
- Code to implement: `open_task_plan` guard.
- Verify: run core open command tests.

### Task 11: Gate Merge

- Test to write: merge is allowed only for `Reviewable` or `Mergeable`.
- Code to implement: `merge_task_plan` guard.
- Verify: run core merge command tests.

### Task 12: Gate Clean

- Test to write: clean is allowed only for `Merged` or `Cleanable`, plus an
  explicit recovery cleanup path if added.
- Code to implement: `clean_task_plan` guard.
- Verify: run core cleanup command tests.

### Task 13: Gate Check, Diff, And Trunk On Substrate

- Test to write: missing worktree blocks check and diff; missing tmux allows
  trunk repair.
- Code to implement: operation guards for check, diff, and trunk.
- Verify: run core command tests.

### Task 14: Block Removed Direct Commands

- Test to write: direct open, merge, clean, check, and diff against `Removed`
  tasks fail.
- Code to implement: operational lookup separate from all-record lookup.
- Verify: run core and CLI direct-command tests.

## Phase 3: Visibility And Attention

### Task 15: Split Visible Vs Actionable Tasks

- Test to write: missing-substrate tasks appear in `tasks`.
- Code to implement: replace `is_operational_task` with a visibility predicate
  and separate operation eligibility.
- Verify: run core list tests.

### Task 16: Fix Repo Counts For Broken Tasks

- Test to write: repo attention count includes broken tasks.
- Code to implement: count visible tasks and attention separately.
- Verify: run repo summary tests.

### Task 17: Make Missing Tmux Attention Visible

- Test to write: `TmuxMissing` creates an inbox item.
- Code to implement: remove the missing-substrate early return in attention
  derivation.
- Verify: run attention tests.

### Task 18: Make Missing Worktrunk Attention Visible

- Test to write: `WorktrunkMissing` creates a recovery or open-trunk item.
- Code to implement: attention mapping for worktrunk recovery.
- Verify: run attention tests.

### Task 19: Make Missing Worktree And Branch Attention Visible

- Test to write: `WorktreeMissing` and `BranchMissing` create high-priority
  items.
- Code to implement: attention mapping for missing worktree and branch.
- Verify: run attention tests.

### Task 20: Deduplicate Flag And Live Attention

- Test to write: the same missing evidence via flag and live status yields one
  attention item.
- Code to implement: evidence-aware attention dedupe rules.
- Verify: run attention tests.

## Phase 4: Fresh Evidence

### Task 21: Add Git Evidence Refresh Primitive

- Test to write: runner output is parsed into `git_status` and side flags.
- Code to implement: core or CLI helper around `git status`.
- Verify: run CLI runner tests.

### Task 22: Refresh Git Evidence Before Cleanup

- Test to write: cleanup runs `git status` even when cached status exists.
- Code to implement: remove cached-status early return for cleanup.
- Verify: run CLI cleanup tests.

### Task 23: Refresh Git Evidence Before Merge

- Test to write: merge preflight requests fresh git evidence.
- Code to implement: merge preflight evidence path.
- Verify: run CLI merge tests.

### Task 24: Clear Recovered Worktree And Branch Flags

- Test to write: fresh git evidence clears stale missing flags.
- Code to implement: evidence application helper.
- Verify: run core or CLI evidence tests.

### Task 25: Preserve Unresolved Missing Flags

- Test to write: failed git status or absent branch keeps attention.
- Code to implement: evidence application helper.
- Verify: run core or CLI evidence tests.

## Phase 5: Create Flow

### Task 26: Record Provisioning Before External Commands

- Test to write: task exists before or after first command failure.
- Code to implement: change `execute_new_task_plan` ordering.
- Verify: run CLI queued-runner tests.

### Task 27: Mark Worktree Created After Git Success

- Test to write: git succeeds and tmux fails, leaving task with worktree
  evidence.
- Code to implement: per-step state update.
- Verify: run persisted CLI tests.

### Task 28: Mark Tmux And Worktrunk Created After Tmux Success

- Test to write: tmux success updates substrate evidence.
- Code to implement: per-step state update.
- Verify: run CLI tests.

### Task 29: Record Agent Attempt After Send Command

- Test to write: send-agent success appends a running attempt.
- Code to implement: agent attempt state helper.
- Verify: run core or CLI tests.

### Task 30: Mark Provisioning Failure As Visible Error

- Test to write: failed provisioning creates an inbox item and visible task.
- Code to implement: lifecycle, live status, and side-flag update on failure.
- Verify: run CLI and attention tests.

### Task 31: Persist Post-Mutation Create Errors

- Test to write: attach or open failure after record creation saves state to
  SQLite.
- Code to implement: wrap post-mutation errors as
  `CommandFailedAfterStateChange`.
- Verify: run context-path persistence tests.

## Phase 6: Open And Trunk Flow

### Task 32: Make Open Navigation-Only

- Test to write: opening `Reviewable`, `Merged`, or `Cleanable` preserves
  lifecycle.
- Code to implement: change `mark_task_opened`.
- Verify: run core and CLI open tests.

### Task 33: Route Missing Tmux To Trunk Repair

- Test to write: open on missing tmux blocks with repair recommendation or uses
  trunk flow.
- Code to implement: plan or attention adjustment.
- Verify: run core and TUI tests.

### Task 34: Trunk Repair Clears Tmux And Worktrunk Flags After Success

- Test to write: successful trunk command updates flags and substrate evidence.
- Code to implement: post-execute state update.
- Verify: run CLI trunk tests.

## Phase 7: Check And Review Flow

### Task 35: Check Records Running, Failure, And Success State

- Test to write: check command failure creates `TestsFailed` attention.
- Code to implement: post-check state update.
- Verify: run CLI check tests.

### Task 36: Check Success Can Promote Review State

- Test to write: `Active` or `Waiting` moves to `Reviewable` only after check
  success if policy allows it.
- Code to implement: transition helper usage.
- Verify: run core and CLI tests.

### Task 37: Do Not Mark Command Failure As Generic Lifecycle Corruption

- Test to write: failed check does not hide the task or mark it removed or
  merged.
- Code to implement: failure mapping.
- Verify: run CLI tests.

## Phase 8: Merge Flow

### Task 38: Merge Blocks Without Fresh Clean Evidence

- Test to write: dirty, conflicted, or missing branch evidence blocks merge.
- Code to implement: merge preflight policy.
- Verify: run core and CLI merge tests.

### Task 39: Merge Requires Confirmation For Risk Evidence

- Test to write: risky merge plan requires `--yes`; no command runs without it.
- Code to implement: plan confirmation logic.
- Verify: run core execute tests.

### Task 40: Successful Merge Updates Lifecycle

- Test to write: successful merge transitions to `Merged` or `Cleanable`.
- Code to implement: post-merge transition.
- Verify: run CLI merge tests.

### Task 41: Failed Merge Records Attention

- Test to write: merge conflict or failure leaves visible task with conflict or
  failure item.
- Code to implement: post-failure state update.
- Verify: run CLI queued-runner tests.

## Phase 9: Cleanup Flow

### Task 42: Wire Cleanup Confirmation Policy

- Test to write: `NeedsConfirmation` and `Dangerous` set
  `requires_confirmation`.
- Code to implement: use safety classification in cleanup plans.
- Verify: run core cleanup tests.

### Task 43: Block Unconfirmed Risky Cleanup

- Test to write: no cleanup command runs without `--yes`.
- Code to implement: rely on `execute_plan` once cleanup plans mark
  confirmation.
- Verify: run CLI cleanup tests.

### Task 44: Remove Cockpit Direct Destructive Clean

- Test to write: cockpit clean requiring confirmation does not run immediately.
- Code to implement: confirmation or deferred flow.
- Verify: run cockpit action tests.

### Task 45: Update State After Tmux Kill

- Test to write: tmux kill success clears tmux evidence even if later cleanup
  fails.
- Code to implement: step-aware cleanup executor.
- Verify: run partial cleanup tests.

### Task 46: Update State After Worktree Removal

- Test to write: worktree remove success marks worktree evidence removed.
- Code to implement: step-aware cleanup executor.
- Verify: run partial cleanup tests.

### Task 47: Update State After Branch Delete

- Test to write: branch delete success clears branch evidence.
- Code to implement: step-aware cleanup executor.
- Verify: run cleanup tests.

### Task 48: Mark Removed Only After Required Cleanup Completes

- Test to write: partial cleanup failure keeps a visible task with attention.
- Code to implement: final cleanup completion rule.
- Verify: run CLI partial-failure tests.

### Task 49: Sweep Uses Same Cleanup Executor

- Test to write: sweep partial failure persists completed task updates and
  leaves failed task visible.
- Code to implement: route sweep through the cleanup operation.
- Verify: run sweep persistence tests.

## Phase 10: Read Freshness

### Task 50: Define Refresh Contract For Read Commands

- Test to write: decision commands call the shared refresh path.
- Code to implement: wrapper for `tasks`, `repos`, `inbox`, `next`, `review`,
  `status`, and `cockpit`.
- Verify: run runner assertion tests.

### Task 51: Handle Refresh Command Failure Gracefully

- Test to write: missing tmux or git during read creates degraded visible state
  or visible error, not total disappearance.
- Code to implement: refresh error mapping.
- Verify: run CLI tests.

### Task 52: Keep Snapshot-Only Behavior Explicit If Needed

- Test to write: any no-refresh command is named, documented, or hidden.
- Code to implement: command option or docs.
- Verify: run CLI tests.

## Phase 11: Cockpit Actions

### Task 53: Expose Allowed Actions Per Task

- Test to write: summaries include or derive valid actions for each lifecycle.
- Code to implement: core output or action model.
- Verify: run core output tests.

### Task 54: Render Lifecycle-Aware Action Menu

- Test to write: active task does not show clean or merge; cleanable task does.
- Code to implement: TUI selectables.
- Verify: run TUI action-menu tests.

### Task 55: Add Confirmation State In Cockpit

- Test to write: pressing clean first asks for confirmation, and a second
  explicit confirmation proceeds.
- Code to implement: TUI pending confirmation model.
- Verify: run TUI tests.

### Task 56: Add Cockpit Recovery Actions

- Test to write: broken tmux task shows repair or open-trunk action.
- Code to implement: action catalog and recovery mapping.
- Verify: run TUI and core attention tests.

### Task 57: Keep Cockpit New-Task Partial Failure Visible

- Test to write: failed create returns to cockpit with flash and visible failed
  or provisioning task.
- Code to implement: cockpit pending action error handling.
- Verify: run CLI cockpit tests.

## Phase 12: Supervisor Integration

### Task 58: Associate Supervisor Run With Task

- Test to write: supervise with task handle updates that task.
- Code to implement: CLI arg or path and registry lookup.
- Verify: run CLI supervise tests.

### Task 59: Apply Agent Events To Live Status

- Test to write: started, thinking, waiting, and completed events update task
  live status and lifecycle.
- Code to implement: event application helper.
- Verify: run core or supervisor tests.

### Task 60: Apply Process Failure To Task Attention

- Test to write: nonzero exit creates command-failed attention.
- Code to implement: event application helper.
- Verify: run supervisor tests.

### Task 61: Apply Git Snapshots To Evidence

- Test to write: conflict snapshot sets conflict flag and attention.
- Code to implement: repo event application helper.
- Verify: run core or supervisor tests.

### Task 62: Persist Supervisor-Driven State

- Test to write: supervised run updates SQLite state.
- Code to implement: save context on event or state changes.
- Verify: run CLI context-path tests.

## Phase 13: Registry Events And Persistence

### Task 63: Record Lifecycle Events Centrally

- Test to write: every transition writes a registry event.
- Code to implement: registry transition method.
- Verify: run registry tests.

### Task 64: Record Substrate Evidence Events

- Test to write: worktree, tmux, and branch changes create events.
- Code to implement: evidence update method.
- Verify: run registry tests.

### Task 65: Persist Any New Operation Fields

- Test to write: SQLite roundtrip for new fields and statuses.
- Code to implement: schema version bump if needed.
- Verify: run registry SQLite tests.

### Task 66: Reject Incompatible Old Schemas Clearly

- Test to write: unsupported schema gets a clear error.
- Code to implement: migration guard if schema changes.
- Verify: run registry and context tests.

## Phase 14: Architecture And Validation

### Task 67: Update Architecture Invariants

- Test to write: markdown verification with `rg`.
- Code to implement: document lifecycle, evidence, and operation model.
- Verify: `rg "Operation" architecture.md`.

### Task 68: Document Destructive Command Policy

- Test to write: markdown verification.
- Code to implement: document cleanup and merge confirmation plus fresh evidence
  policy.
- Verify: `rg "fresh evidence|confirmation" architecture.md`.

### Task 69: Document Partial Failure Policy

- Test to write: markdown verification.
- Code to implement: document state persistence and recovery visibility rules.
- Verify: `rg "partial failure" architecture.md`.

### Task 70: Full Validation

- Test to write: none.
- Code to implement: formatting and lint fixes only if needed.
- Verify:
  - `cargo fmt --check`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo nextest run --all-features`, or `cargo test --all-features` if
    nextest is unavailable.
