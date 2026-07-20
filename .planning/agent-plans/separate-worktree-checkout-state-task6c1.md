ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-core/src/commands.rs, crates/ajax-web/src/slices/actions.rs, crates/ajax-web/src/slices/cockpit.rs, crates/ajax-web/src/slices/operate.rs, crates/ajax-web/src/runtime.rs, crates/ajax-cli/src/web_backend.rs]
  REASON: This is a bounded cross-adapter transport change with exact existing core types and safety behavior; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, core or browser source must change, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Carry a mismatch Repair's exact typed `BranchAdoptionPlan` from core planning
through Web Cockpit's Rust action projection and operation request. An
unconfirmed request must not adopt. A confirmed request must execute the exact
expected/observed pair supplied by the browser, so changed checkout evidence is
rejected by core rather than silently confirming a freshly planned branch.

## Allowed files

- `crates/ajax-core/src/commands.rs`
- `crates/ajax-web/src/slices/actions.rs`
- `crates/ajax-web/src/slices/cockpit.rs`
- `crates/ajax-web/src/slices/operate.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-cli/src/web_backend.rs`

## Forbidden changes

- Other than the one-line public re-export of the existing
  `context::BranchAdoptionPlan` from `ajax_core::commands`, do not edit ajax-core.
  Do not edit browser TypeScript, any other file, or any file under a `tests/`
  directory. Inline tests in the five allowed Rust source files are permitted.
- Do not define a second branch-adoption struct or serialize a `CommandPlan`
  through `serde_json::Value` to recover its typed fields. Reuse the re-exported
  core type directly.
- Do not derive checkout mismatch or compare branch names in Web code. Project
  confirmation and adoption metadata only from core's
  `plan_task_command_operation` result.
- Do not accept a bare confirmation boolean as sufficient for branch adoption.
  A confirmed mismatch Repair must carry the exact typed pair originally
  projected to the browser.
- Do not re-plan and execute a newly observed pair when the request supplied an
  older pair. Preserve the request pair in the plan passed to core so core's
  existing stale-pair guard makes the decision.
- Do not run or plan Git switch/checkout, tmux, shell/check, or another mutation
  command for adoption. The two Git substrate-observation commands are allowed.
- Do not change Drop confirmation/undo behavior, remediation behavior, ordinary
  Repair/Resume/Review/Ship behavior, task identity, or public route names.
- Do not add a dependency, new core abstraction, generic confirmation token
  system, or unrelated cleanup. Do not delete or weaken assertions.

## Context evidence

- Core's serde-enabled `BranchAdoptionPlan` contains `expected_branch` and
  `observed_branch`; mismatch Repair plans are zero-command and
  confirmation-required. The type is public inside `commands::context` but is
  not yet re-exported alongside `CommandPlan` from `commands.rs`.
- Core execution unconditionally requires confirmation for adoption and rejects
  the plan when task intent, mismatch presence, worktree presence, or observed
  branch no longer matches the typed pair. Its exact stale reason is
  `checkout changed since repair was planned; refresh and retry`.
- `browser_cockpit_view` and `browser_task_detail_view` have a core
  `CommandContext`, but `browser_actions` currently maps `TaskCard` actions
  statically and marks only Drop confirmation-required.
- `operate::execute_task_command` refreshes Git evidence, creates a fresh plan,
  and currently auto-confirms only plans that do not require confirmation.
- `MobileActionRequest` currently transports only task/action/request ID.
  `OperateRequest` is the typed runtime-to-slice boundary, and
  `CliRuntimeBridge::persist_operate` already persists any successful
  `state_changed` result.

## Code anchors

- `crates/ajax-core/src/commands.rs`: the existing `pub use context::{...}` line.
- `crates/ajax-web/src/slices/actions.rs`: `WebAction`, `web_action`, and
  `browser_actions`.
- `crates/ajax-web/src/slices/cockpit.rs`: `browser_cockpit_view`,
  `browser_task_card`, `browser_task_detail_view`, and
  `browser_cockpit_and_detail_pass_through_checkout_mismatch`.
- `crates/ajax-web/src/slices/operate.rs`: `OperateRequest`, `operate`, and
  `execute_task_command`.
- `crates/ajax-web/src/runtime.rs`: `MobileActionRequest`,
  `handle_action_request`, `TestBridge`, and existing `/api/operations` tests.
- `crates/ajax-cli/src/web_backend.rs`: `CliRuntimeBridge::execute_operate`,
  `persist_operate`, `reviewable_context`, and inline persistence tests.

## Test-first instructions

Make all test edits before production edits. Run every named RED command and
capture its intended failure; a command that runs zero tests is not evidence.

1. Add
   `browser_cockpit_mismatch_repair_projects_exact_adoption_confirmation` in
   `slices/cockpit.rs`. Build the existing named mismatch. Assert card JSON and
   detail Repair action both have `confirmation_required: true` and exact
   `branch_adoption: {expected_branch: "ajax/fix-login", observed_branch:
   "fix/pane-stuck"}`. Assert Resume remains unconfirmed and has no adoption
   payload. RED must fail because Repair is currently unconfirmed and has no
   typed payload.
2. In `slices/operate.rs`, add a tiny queued/recording runner plus mismatch Git
   observation outputs and three tests:
   - `operate_slice_mismatch_repair_requires_typed_confirmation`: request Repair
     without confirmation/pair; assert `ConfirmationRequired`, unchanged intent
     and history, and only the two read-only Git observation commands.
   - `operate_slice_confirmed_mismatch_repair_adopts_requested_pair_without_mutation_commands`:
     request `confirmed: true` with the exact typed pair; assert branch adoption,
     cleared mismatch, `state_changed`, empty operation output, preserved task
     identity/path/session, and only the two observation commands (no switch,
     checkout, tmux, or shell/check).
   - `operate_slice_stale_mismatch_confirmation_rejects_changed_checkout`:
     request the old `fix/pane-stuck` pair but make refresh observe a different
     named branch. Assert the exact stale reason, unchanged expected branch and
     history, false state change, and no mutation command.
3. Add `axum_operation_preserves_branch_adoption_confirmation` in `runtime.rs`.
   POST `/api/operations` with `confirmed: true` and the exact JSON pair. Assert
   `TestBridge` receives an equal typed `OperateRequest`, including both fields.
   Preserve serde defaults by keeping existing request bodies without the new
   fields green.
4. Add `web_bridge_persists_confirmed_mismatch_branch_adoption` in
   `web_backend.rs`. Save a named-mismatch context to SQLite, execute the bridge
   with confirmed exact pair and a runner returning mismatch refresh evidence,
   reload SQLite, and assert the observed branch became task intent. Assert only
   the two Git observation commands ran and identity/path/session were stable.
5. Before production edits run these exact commands separately:
   - `cargo test -p ajax-web browser_cockpit_mismatch_repair_projects_exact_adoption_confirmation -- --nocapture`
   - `cargo test -p ajax-web operate_slice_mismatch_repair_requires_typed_confirmation -- --nocapture`
   - `cargo test -p ajax-web operate_slice_confirmed_mismatch_repair_adopts_requested_pair_without_mutation_commands -- --nocapture`
   - `cargo test -p ajax-web operate_slice_stale_mismatch_confirmation_rejects_changed_checkout -- --nocapture`
   - `cargo test -p ajax-web axum_operation_preserves_branch_adoption_confirmation -- --nocapture`
   - `cargo test -p ajax-cli web_bridge_persists_confirmed_mismatch_branch_adoption -- --nocapture`

## Edit instructions

1. Add `BranchAdoptionPlan` to the existing `pub use context::{...}` list in
   `ajax-core/src/commands.rs`. This is the only allowed core edit.
2. Extend `WebAction` with an optional, omitted-when-None core
   `BranchAdoptionPlan`. Make `browser_actions` accept the existing core context.
   For Repair only, call `plan_task_command_operation` with `OpenMode::NoAttach`
   and copy `requires_confirmation` plus `branch_adoption` from that core plan.
   Keep Drop's existing static confirmation and every other action unchanged.
   Update bounded call sites and inline test fixtures mechanically.
3. Extend `OperateRequest` with `confirmed: bool` and optional
   `BranchAdoptionPlan`. Extend `MobileActionRequest` with serde-defaulted
   equivalents and pass them unchanged through `handle_action_request`. Add
   false/None to direct Rust request literals that predate the fields.
4. Pass the request confirmation and pair into the task-command execution
   helper. For Repair, when a request pair exists, place that exact pair on the
   plan passed to core, even if refreshed planning now reports a different or
   absent adoption. This is deliberate: core must reject stale evidence. If a
   freshly planned Repair has adoption metadata but the request omitted the
   typed pair, do not treat a bare `confirmed: true` as confirmation; execute it
   unconfirmed so core returns `ConfirmationRequired`. Ordinary non-adoption
   Repair remains executable without confirmation as before.
5. Let `execute_task_command_operation` remain the sole adoption validator and
   registry mutator. Do not compare expected/observed strings in Web code.
6. Keep bridge persistence generic: its existing `state_changed` path should
   persist adoption. Add only the focused regression and any minimal runner
   fixture needed to prove it.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-web browser_cockpit_mismatch_repair_projects_exact_adoption_confirmation -- --nocapture`
2. `cargo test -p ajax-web operate_slice_mismatch_repair_requires_typed_confirmation -- --nocapture`
3. `cargo test -p ajax-web operate_slice_confirmed_mismatch_repair_adopts_requested_pair_without_mutation_commands -- --nocapture`
4. `cargo test -p ajax-web operate_slice_stale_mismatch_confirmation_rejects_changed_checkout -- --nocapture`
5. `cargo test -p ajax-web axum_operation_preserves_branch_adoption_confirmation -- --nocapture`
6. `cargo test -p ajax-cli web_bridge_persists_confirmed_mismatch_branch_adoption -- --nocapture`
7. `cargo test -p ajax-web operate_slice_ -- --nocapture`
8. `cargo test -p ajax-web cockpit_ -- --nocapture`
9. `cargo test -p ajax-web axum_operations_are_idempotent_by_request_id -- --nocapture`
10. `cargo check -p ajax-web -p ajax-cli --all-targets`
11. `cargo fmt --check`
12. `git diff --check`
13. `cargo clippy -p ajax-web -p ajax-cli --all-targets -- -D warnings`

## Acceptance criteria

- Card and detail actions project core's exact named adoption pair and require
  confirmation only when the core Repair plan requires it.
- Runtime preserves the typed confirmation flag and pair without JSON
  round-trips or string encoding.
- Unconfirmed or pairless mismatch Repair cannot mutate intent.
- Confirmed exact-pair Repair adopts and persists without any mutation command.
- Changed checkout evidence after projection is rejected by core with the exact
  stale reason and cannot adopt the newly observed branch.
- Existing field-omitting clients remain deserializable through serde defaults;
  Drop and all non-adoption actions retain behavior.
- Only the six allowed files change and every verification command passes.

## Stop conditions

- Stop if safety requires Web code to compare branches, any ajax-core change
  beyond the named one-line re-export, or accepting confirmation without the
  exact typed pair.
- Stop if browser TypeScript or a seventh Rust source file is required; report the
  concrete reason for a follow-up packet.
- Stop on unrelated baseline failures without changing unrelated code/tests.
- Return the exact report below as the entire response. Start with
  `---DELEGATE_REPORT_START---`; do not use Markdown fences or prose before or
  after it. Every command needs its own evidence item.

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

- Round 1 was rejected and deterministically restored. Its five-file behavior
  delta was in scope, but it defined a duplicate Web `BranchAdoptionPlan` and
  converted the core `CommandPlan` through `serde_json::Value` merely to recover
  public fields. Its raw report also supplied RED evidence for only one of the
  six required focused tests, and the adapter rejected the otherwise complete
  envelope as `MISSING_STRUCTURED_REPORT`.
- Re-export the existing core `BranchAdoptionPlan` on the same line as
  `CommandPlan`, import that type directly in both Web DTOs, and copy
  `plan.branch_adoption.clone()` directly. No mirror struct, JSON conversion,
  string encoding, or extra helper is allowed.
- Reapply all tests before production edits and run all six focused commands in
  RED. Report the actual compile or assertion failure for each command; do not
  describe a hypothetical failure.
- This is the sole revision round. Run all thirteen verification commands and
  return the exact plain report envelope.

## Parent gate result

- Round 2 accepted on 2026-07-20 after deterministic scope review showed only
  the six allowed files changed. The raw report contained all six actual RED
  failures and all GREEN/VERIFY evidence, but the adapter again emitted
  `MISSING_STRUCTURED_REPORT`; parent review and validation supplied the gate.
- Parent validation exited 0 for all thirteen packet commands. The parent then
  strengthened the pairless test with a one-token test-only change from
  `confirmed: false` to `confirmed: true`, proving the boolean alone cannot
  adopt, and reran that test, the ordinary missing-worktree Repair regression,
  formatting, diff check, and warning-free clippy successfully.
