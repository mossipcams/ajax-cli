# Implementation Packet — Drop always available regardless of status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Task

Make `OperatorAction::Drop` always present in `available_operator_actions` for
every non-`Removed` task, including checkout-mismatch / Error / Running /
Waiting / Idle cases, so Web Cockpit (and other consumers) can always offer
Drop as an escape hatch.

## Scope

### Allowed

- `crates/ajax-core/src/recommended.rs` (production + tests)
- `crates/ajax-core/src/commands/projection.rs` (tests only — update
  checkout-mismatch `available_actions` expectations)
- `crates/ajax-web/src/slices/cockpit.rs` (tests only — assert Drop on
  checkout-mismatch card/detail)
- `architecture.md` (one policy sentence: Drop stays available on mismatch;
  Ship/Clean remain blocked)
- `.planning/agent-plans/web-drop-always-available.md` (checklist only)

### Forbidden

- Do not change Drop execution, teardown force semantics, or substrate
  observation.
- Do not unblock `TaskOperation::Merge` / `Clean` on checkout mismatch.
- Do not change Ship/Review/Repair/Resume primary-action selection beyond what
  falls out of Drop being appended to available actions.
- Do not edit browser TypeScript/React UI except via Rust action lists.
- Do not add dependencies, renames, or drive-by cleanup.
- Do not commit, push, merge, rebase, or change branches.
- Do not edit files outside Allowed.

## Acceptance

1. For a checkout-mismatch task (worktree present, wrong branch),
   `operator_action(...).available_actions` contains `OperatorAction::Drop`
   alongside Repair and Resume.
2. Primary action for that mismatch case stays Resume (or existing primary
   policy); Drop is available but not forced primary.
3. Web cockpit JSON / `browser_task_detail_view` for a checkout-mismatch task
   includes an action with `action == "drop"`.
4. Existing Drop-only / Repair+Drop substrate cases still include Drop.
5. `architecture.md` no longer claims Drop is blocked until checkout
   reconciliation; it states Drop remains an escape hatch while Ship/Clean stay
   blocked.

## Constraints

- Smallest edit: fix the checkout-mismatch early return to include Drop, and
  ensure any other return path for a non-Removed task that can omit Drop also
  appends it (final `ensure` push is fine).
- Do not make Drop primary for healthy Active/Running tasks.
- Preserve confirmation_required / destructive web chrome for Drop (already
  driven by action id).

## Verification

```yaml
verification:
  methods:
    - type: test
      command: rtk cargo nextest run -p ajax-core checkout_mismatch_recommends_repair
      expected: pass; available_actions include Drop
    - type: test
      command: rtk cargo nextest run -p ajax-core checkout_mismatch_card_and_inbox
      expected: pass with Drop in available_actions
    - type: test
      command: rtk cargo nextest run -p ajax-web checkout_mismatch
      expected: pass; card/detail include drop
    - type: build
      command: rtk cargo check -p ajax-core -p ajax-web
      expected: success
  broader_checks:
    - rtk cargo nextest run -p ajax-core operator_actions
  reason: Focused recommended/projection/web cockpit tests cover the action-list hole; build confirms compile.
```

## Stop if

- Fix would require changing Drop execution eligibility or lifecycle transitions.
- More than ~3 production files or ~80 changed lines beyond tests/docs.
- Checkout-mismatch primary action would become Drop as a side effect and
  cannot be kept as Resume/Repair without a larger redesign — stop and report.

## Code anchors

- `crates/ajax-core/src/recommended.rs` `available_operator_actions` early return
  at checkout mismatch (~186–191) currently
  `vec![Repair, Resume]` — must include Drop.
- Test `checkout_mismatch_recommends_repair_and_only_safe_terminal_access`
  expects `[Repair, Resume]` — update to include Drop; rename if the name
  becomes misleading.
- `crates/ajax-core/src/commands/projection.rs`
  `checkout_mismatch_card_and_inbox_share_canonical_explanation` expects
  `[Repair, Resume]`.
- `crates/ajax-web/src/slices/cockpit.rs` checkout-mismatch test asserts
  actions `[repair, resume]` only (~407–415).
- `architecture.md` ~232–235: “Ship and Drop/Cleanup are blocked until
  reconciliation” — change to Ship/Clean blocked; Drop remains escape hatch.
- Drop execute already allowed on mismatch:
  `branch_sensitive_checkout_mismatch_drop_uses_remove_plan` in
  `task_operations.rs`; `TaskOperation::Remove` not blocked by mismatch in
  `operation.rs`.

## Edit instructions

1. Update/add focused test(s) in `recommended.rs` so checkout-mismatch
   `available_actions` contains Drop (and still contains Repair + Resume).
2. Implement the smallest production change in `available_operator_actions`.
3. Update projection + ajax-web cockpit tests.
4. Patch the one architecture sentence.
5. Run verification commands; return `DELEGATE_REPORT`.
