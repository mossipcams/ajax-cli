# Plan: Drop always available from Web Cockpit

## Scope

Ensure every visible Ajax task offers **Drop** as an operator escape hatch in
Web Cockpit (and other surfaces that consume `available_operator_actions`),
regardless of operator status / lifecycle / checkout-mismatch state.

### Non-goals

- Do not change Drop execution/teardown mechanics (already force-tears; already
  plans Remove on checkout mismatch).
- Do not unblock Ship/Clean on checkout mismatch.
- Do not change swipe/UI chrome beyond what falls out of action lists.
- Do not make Drop the primary action for healthy tasks.

## Root cause

`available_operator_actions` early-returns checkout-mismatch tasks as
`[Repair, Resume]` only (`recommended.rs`). Web strips `resume` via
`visibleTaskActions`, so the detail/action surface shows Repair only — no Drop.

Drop *execution* already works on mismatch (`branch_sensitive_checkout_mismatch_drop_uses_remove_plan`;
`TaskOperation::Remove` stays allowed). Architecture text that says Drop is
blocked on mismatch is stale relative to operation eligibility.

## Delegation decision

`Delegation decision: delegated via model-router`

## Task checklist

- [x] Packet READY at `.planning/packets/web-drop-always-available.md`
- [x] Impl: include `OperatorAction::Drop` on checkout-mismatch path in
      `available_operator_actions` (other statuses already get Drop via Remove)
- [x] Tests: checkout-mismatch available actions include Drop; web cockpit card /
      detail surfaces Drop; update projection expectations
- [x] Docs: `architecture.md` — Drop remains escape hatch on mismatch; Ship/Clean
      still blocked
- [x] Parent Review Gate + focused validation

## Approval

User request: “always be able to drop a task from ajax web regardless of
status” — authorized behavior + architecture doc alignment.

## Validation

```bash
rtk cargo nextest run -p ajax-core checkout_mismatch_includes_repair_resume_and_drop checkout_mismatch_card_and_inbox operator_actions_prefer_drop operator_actions_offer_repair
# → 4 passed (parent)
rtk cargo nextest run -p ajax-web checkout_mismatch
# → 1 passed (parent)
rtk cargo nextest run -p ajax-core safe_reviewable_task_primary_is_resume
# → 1 passed (parent)
npm run verify
# → exit 0 (parent, pre-commit gate)
```

Parent Review Gate: **ACCEPT** (round 2 `cursor-delegate` / `composer-2.5`).

## Deviations

- Round 1 `pi-delegate` / `opencode-go/glm-5.2` failed with OpenCode monthly
  usage limit (429). Empty diff. Escalated once to `cursor-delegate`.
- Production fix is the checkout-mismatch early return only (the known hole);
  other statuses already get Drop via `TaskOperation::Remove` eligibility.
- Delegate structured report failed schema validation; parent accepted after
  inspecting delta and re-running focused tests.
