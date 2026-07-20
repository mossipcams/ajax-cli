ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-cli/src/cockpit_actions.rs, crates/ajax-cli/src/cockpit_backend.rs, crates/ajax-cli/src/lib/tests.rs]
  REASON: The existing TUI already owns second-activation confirmation; the smallest safe change is a bounded CLI-adapter transport of the exact core plan, delegated to the user-requested Cursor lane.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Make native Cockpit Repair confirm and execute the exact branch-adoption plan
shown to the operator. First activation must name the observed/expected pair,
second activation must defer that same core plan, confirmed execution must adopt
without any external command, and changed evidence between prompt and execution
must be rejected by core without changing task intent.

## Allowed files

- `crates/ajax-cli/src/cockpit_actions.rs`
- `crates/ajax-cli/src/cockpit_backend.rs`
- `crates/ajax-cli/src/lib/tests.rs`

## Forbidden changes

- Do not edit ajax-core, ajax-tui, any other file, or any file under a `tests/`
  directory. Inline tests in the allowed CLI source test module are permitted.
- Do not add a field or variant to `ajax_tui::PendingAction`, `ActionOutcome`,
  `App`, or any public TUI type. The existing handler already lives across both
  Enter activations; reuse it.
- Do not derive checkout mismatch, compare branch names, or implement adoption
  policy in the UI/TUI. Ask core for `plan_task_command_operation` and pass the
  returned `CommandPlan` unchanged.
- Do not re-plan a confirmation-required Repair after the first prompt. The
  exact plan shown on first activation is the plan execution must receive.
- Do not run or plan Git switch/checkout, tmux, shell/check, or any external
  command for branch adoption.
- Do not alter Drop confirmation, ordinary Resume/Review/Ship/Repair behavior,
  public CLI behavior, task identity, or core confirmation/stale validation.
- Do not add a dependency, abstraction layer, global/thread-local state, or
  unrelated cleanup. Do not delete or weaken assertions.

## Context evidence

- `ajax-tui/src/input.rs` already detects first versus second activation and
  calls the same `CockpitEventHandler` object's `on_action` then
  `on_confirmed_action`. No TUI change is needed.
- `InteractiveCockpitHandler` is constructed once for one interactive run and
  remains alive across action and refresh callbacks. A local
  `Option<commands::CommandPlan>` borrowed by that handler can retain the exact
  plan until `run_interactive_with_flash_and_refresh` returns its existing
  `PendingAction`; it then remains available to the deferred executor.
- `tui_cockpit_action_with_confirmation` currently special-cases Drop only.
  Repair is grouped with ordinary deferred actions and therefore never asks for
  confirmation before leaving the TUI.
- Both deferred task-command execution paths currently create a fresh plan and
  set `confirmed = !plan.requires_confirmation`; mismatch Repair therefore
  fails confirmation and, if changed to a bare boolean, could adopt a different
  branch after re-planning.
- Core's typed adoption plan already contains exact expected/observed branches.
  Core execution independently requires confirmation and revalidates that exact
  pair, returning `checkout changed since repair was planned; refresh and retry`
  with no mutation or command when stale.
- Task 6A added `sample_context_with_named_checkout_mismatch` in
  `crates/ajax-cli/src/lib/tests.rs`; reuse it.

## Code anchors

- `crates/ajax-cli/src/cockpit_actions.rs`:
  `tui_cockpit_action`, `tui_cockpit_confirmed_action`,
  `tui_cockpit_action_with_confirmation`,
  `execute_pending_cockpit_action_with_task_session`, and
  `execute_pending_cockpit_action_with_task_session_and_checkpoint`.
- `crates/ajax-cli/src/cockpit_backend.rs`:
  `render_interactive_cockpit_command`, `InteractiveCockpitHandler`, and its
  `CockpitEventHandler` implementation.
- `crates/ajax-cli/src/lib/tests.rs`: existing Cockpit action tests around
  `cockpit_supported_actions_dispatch_without_shell_suggestions` and pending
  Repair/task-session tests around lines 7500–7900.

## Test-first instructions

Make all test edits before production edits. Run every named RED command and
capture its intended failure; a command that runs zero tests is not evidence.

1. Add `cockpit_mismatch_repair_prompts_for_exact_branch_adoption`. Use the
   existing mismatch context and a Repair `CockpitActionItem`. Call the
   first-activation action path with an empty retained-plan slot. Assert exact
   outcome message
   `press enter again to adopt branch fix/pane-stuck (expected ajax/fix-login)`,
   assert the slot contains a zero-command, confirmation-required plan with the
   exact typed adoption pair, and assert task branch remains `ajax/fix-login`.
   This must fail RED because Repair currently returns `Defer` and retains no
   plan.
2. Add `confirmed_cockpit_mismatch_repair_adopts_original_plan_without_commands`.
   Obtain the plan through the first-activation path, call the confirmed action
   path using the same slot, assert it returns the existing Repair
   `PendingAction`, then execute through the real task-session deferred path
   while passing `slot.as_ref()`. Assert branch becomes `fix/pane-stuck`,
   mismatch clears, `state_changed` is true, command runner and task-session
   runner are both empty, and task ID/path/tmux session remain unchanged.
3. Add `stale_cockpit_mismatch_confirmation_does_not_adopt_changed_checkout`.
   Capture the first plan for `fix/pane-stuck`, then change current Git evidence
   to another present named checkout before deferred execution. Execute the old
   confirmed plan through the task-session path. Assert the exact normal CLI
   blocked message contains
   `checkout changed since repair was planned; refresh and retry`; expected task
   branch remains `ajax/fix-login`; no event/state flag/command/task-session
   command is added; `state_changed` remains false.
4. Add `declined_cockpit_mismatch_confirmation_does_not_mutate_intent`. First
   activation only: assert the confirmation outcome, unchanged branch/event
   history, and no external runner involvement. Do not fake a decline API—the
   existing TUI decline is simply not performing the second activation.
5. Before production edits run:
   - `cargo test -p ajax-cli cockpit_mismatch_repair_prompts_for_exact_branch_adoption -- --nocapture`
   - `cargo test -p ajax-cli confirmed_cockpit_mismatch_repair_adopts_original_plan_without_commands -- --nocapture`
   - `cargo test -p ajax-cli stale_cockpit_mismatch_confirmation_does_not_adopt_changed_checkout -- --nocapture`
   - `cargo test -p ajax-cli declined_cockpit_mismatch_confirmation_does_not_mutate_intent -- --nocapture`
6. The first test must fail on the current `Defer` outcome; the other focused
   tests may initially fail to compile because the retained-plan transport does
   not exist. Capture those failures before production edits.

## Edit instructions

1. Keep the public `tui_cockpit_action` and `tui_cockpit_confirmed_action`
   wrappers for existing callers/tests. Give their private/shared helper one
   mutable `Option<commands::CommandPlan>` slot; wrappers may use a local slot.
   First activation must clear any older slot before dispatching a new action.
2. Split Repair from the ordinary deferred-action match arm. On first Repair
   activation, ask core for its plan with `OpenMode::NoAttach`. If the plan
   requires confirmation, build the message from `plan.branch_adoption`, store
   that exact plan in the slot, and return existing `ActionOutcome::Confirm`.
   Use a generic `press enter again to confirm repair` only if a future
   confirmation-required Repair plan lacks adoption metadata. Non-confirmation
   Repair remains an ordinary Defer.
3. On the confirmed Repair callback, if the slot already contains the plan,
   return the existing Defer outcome without re-planning or replacing it. A
   direct confirmed wrapper with no retained slot may plan once for backwards-
   compatible tests, but the interactive handler must use its retained slot.
4. In `render_interactive_cockpit_command`, create one local optional confirmed
   plan for each interactive run. Add a mutable reference to it on
   `InteractiveCockpitHandler`; route both action callbacks through the shared
   helper and pass `confirmed_plan.as_ref()` to the deferred task-session
   executor after the TUI returns. Add the field as None in the two inline
   handler test constructions.
5. Extend the two task-session deferred executor entry points with an optional
   confirmed plan, updating their bounded call sites and tests mechanically.
   For Repair only, use the retained plan unchanged when present; otherwise
   preserve current fresh planning. Pass `confirmed = true` only when that
   retained Repair plan is present (or when the plan does not require
   confirmation). Let core perform all stale-pair and confirmation validation.
6. Keep Drop and every non-Repair action exactly as before. Do not change the
   public TUI transport or add storage outside the existing interactive handler.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-cli cockpit_mismatch_repair_prompts_for_exact_branch_adoption -- --nocapture`
2. `cargo test -p ajax-cli confirmed_cockpit_mismatch_repair_adopts_original_plan_without_commands -- --nocapture`
3. `cargo test -p ajax-cli stale_cockpit_mismatch_confirmation_does_not_adopt_changed_checkout -- --nocapture`
4. `cargo test -p ajax-cli declined_cockpit_mismatch_confirmation_does_not_mutate_intent -- --nocapture`
5. `cargo test -p ajax-cli pending_cockpit_repair -- --nocapture`
6. `cargo test -p ajax-cli cockpit_ -- --nocapture`
7. `cargo test -p ajax-tui task_action_confirmation -- --nocapture`
8. `cargo check -p ajax-cli -p ajax-tui --all-targets`
9. `cargo fmt --check`
10. `git diff --check`

## Acceptance criteria

- First native Cockpit Repair activation shows the exact branch pair from the
  typed core plan and makes no mutation.
- Second activation defers the same plan; successful execution adopts without
  external or task-session commands and preserves task identity/path/session.
- Changed checkout evidence after the prompt is rejected by core and cannot
  adopt the new branch or mutate history.
- Decline is a no-op; ordinary Repair and every non-Repair action remain green.
- No ajax-tui or ajax-core source changes, no new public transport type, and no
  dependency or branch policy outside core.
- Only the three allowed files change and every verification command passes.

## Stop conditions

- Stop if safety requires changing ajax-tui public state/event types, core
  adoption logic, or a fourth source file; report the concrete reason.
- Stop if exact-plan retention cannot survive the existing handler's refresh
  callbacks or if execution must re-plan a confirmed Repair.
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

- Round 1 was rejected and deterministically restored. It edited this packet,
  which was outside `ALLOWED_SCOPE`, and its own `cargo check` evidence reported
  dead-code warnings that would fail the required `-D warnings` gate.
- Reapply the four test-first behaviors and the three-file implementation only.
  Do not edit this packet or any plan/run artifact.
- Use one shared `pub(crate)` action helper taking `confirmed: bool` plus the
  mutable retained-plan slot. Have `InteractiveCockpitHandler` call that helper
  for both callbacks. Keep the old no-slot wrappers only for tests and mark them
  `#[cfg(test)]`; do not add separate retained-plan wrapper functions.
- Inline the small Repair match arm or otherwise remove the redundant
  `if retained_plan.is_some() { return defer } return defer` branch. Confirmed
  Repair simply defers; unconfirmed Repair plans once and stores only a
  confirmation-required plan.
- Remove the unused `PanicRunner` construction from the declined test. The
  no-second-activation assertion itself proves the existing action path has no
  runner boundary.
- Run the original ten verification commands plus
  `cargo clippy -p ajax-cli -p ajax-tui --all-targets -- -D warnings`; report no
  warnings. This is the only revision round.

## Parent gate result

- Round 2 accepted on 2026-07-20 after deterministic scope review showed only
  the three allowed source files changed. Cursor's raw report contained the
  requested evidence, but the adapter emitted `MISSING_STRUCTURED_REPORT`, so
  the parent did not rely on the delegate's success claim.
- Parent validation exited 0 for all eleven packet commands: four focused
  adoption/decline/stale tests, `pending_cockpit_repair`, the 95-test
  `cockpit_` group, ajax-tui's five confirmation tests, focused `cargo check`,
  `cargo fmt --check`, `git diff --check`, and warning-free focused clippy.
