ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: cursor-delegate
  MODE: implement
  MODEL: composer-2.5
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: 0
  ALLOWED_SCOPE: [crates/ajax-web/web/src/shared/lib/types.ts, crates/ajax-web/web/src/shared/lib/contracts.ts, crates/ajax-web/web/src/shared/lib/contracts.test.ts, crates/ajax-web/web/src/features/task/ActionBar.tsx, crates/ajax-web/web/src/features/task/ActionBar.test.tsx, crates/ajax-web/web/dist/app.js]
  REASON: This is a bounded browser transport/state change using an established component and API helper; the user explicitly requested Cursor delegation.
  ESCALATE_IF: [Cursor is unavailable, test-first evidence is missing, the delta leaves allowed scope, Rust or API helper source must change, or verification fails]

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Make Web Cockpit's two-tap Repair confirmation submit the exact typed branch
pair projected on the first tap. The first tap makes no request. The second tap
sends `confirmed: true` with the retained pair, even if a refreshed action prop
contains a different pair in between. Ordinary actions remain immediate and are
sent unconfirmed; Drop keeps its existing delayed undo behavior.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/types.ts`
- `crates/ajax-web/web/src/shared/lib/contracts.ts`
- `crates/ajax-web/web/src/shared/lib/contracts.test.ts`
- `crates/ajax-web/web/src/features/task/ActionBar.tsx`
- `crates/ajax-web/web/src/features/task/ActionBar.test.tsx`
- `crates/ajax-web/web/dist/app.js` (generated only by the required production build)

## Forbidden changes

- Do not edit Rust, `api.ts`, another browser file, or any file under a `tests/`
  directory. Inline `.test.ts(x)` files named above are allowed. Do not hand-edit
  `dist/app.js`; it may change only as output of `npm run web:build`.
- Do not derive checkout mismatch, compare expected/observed branches, or invent
  branch policy in the browser. Render and transport the Rust-projected fields.
- Do not retain only an action string for confirmation; that would let a prop
  refresh replace the displayed pair. Retain the exact `WebAction` object.
- Do not send `confirmed: true` on first tap or on ordinary one-tap actions.
- Do not change the API path, request ID behavior, result handling, Drop's
  two-tap plus delayed-undo flow, remediation behavior, button order, or CSS.
- Do not add a dependency, generic confirmation state machine, token protocol,
  new component, or unrelated cleanup. Do not delete or weaken assertions.

## Context evidence

- Task 6C1 now serializes optional `branch_adoption` on `WebAction` with exact
  `expected_branch` and `observed_branch`, and accepts serde-defaulted
  `confirmed` plus that pair on `/api/operations`.
- Rust refuses a bare confirmation boolean for adoption and passes a supplied
  old pair to core, whose stale guard rejects changed checkout evidence.
- `ActionBar` currently stores only `pendingAction: string`, so a re-render can
  replace the action data between taps. Its `run` request currently sends no
  confirmation fields.
- The existing `postOperation` helper already serializes the typed request and
  needs no change. JSON serialization omits optional properties whose value is
  `undefined`.
- Web production assets are tracked and embedded by Rust, so the required build
  legitimately updates `dist/app.js`; no other dist asset should change for
  this source edit.
- `contracts.ts` validates the required WebAction fields but currently ignores
  optional branch-adoption metadata.

## Code anchors

- `types.ts`: `WebAction` and `OperationRequest`.
- `contracts.ts`: `isObject` and `assertAction`.
- `contracts.test.ts`: `describe("assertCockpit")`.
- `ActionBar.tsx`: `pendingAction`, `label`, `run`, `armDrop`, and
  `handleClick`.
- `ActionBar.test.tsx`: existing Review/Drop fixtures and confirmation tests.

## Test-first instructions

Make all test edits before production edits. Run every named RED command and
capture its intended failure; a command that runs zero tests is not evidence.

1. Add `validates optional branch adoption metadata` to `contracts.test.ts`.
   Build a valid cockpit card action with exact string pair and assert it is
   accepted. Then make either pair field non-string or missing and assert
   `IncompatibleResponseError`. RED must fail because the malformed optional
   payload is currently ignored.
2. Add a Repair fixture with `confirmation_required: true`, non-destructive,
   and exact `branch_adoption`. Add
   `sends the retained branch adoption only on the confirming tap`. First click:
   assert no `postOperation` call and `Tap to confirm`. Second click: assert one
   request with task/action/request ID, `confirmed: true`, and the exact pair.
   RED must fail because the current request lacks both new fields.
3. Add `does not replace a pending adoption pair when actions refresh`. Render
   `fix/pane-stuck`, click once, rerender the same Repair action ID with observed
   `fix/new-checkout`, click the confirming button, and assert the request still
   contains `fix/pane-stuck`. RED must fail because current state retains only
   the action ID and executes the newly rendered object.
4. Add or strengthen
   `marks ordinary actions unconfirmed and runs them immediately`. Click Review
   once and assert one request with `confirmed: false` and no
   `branch_adoption`; no confirmation label is shown.
5. Before production edits run these exact commands separately:
   - `npm run web:test -- --run crates/ajax-web/web/src/shared/lib/contracts.test.ts -t "validates optional branch adoption metadata"`
   - `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "sends the retained branch adoption only on the confirming tap"`
   - `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "does not replace a pending adoption pair when actions refresh"`
   - `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "marks ordinary actions unconfirmed and runs them immediately"`

## Edit instructions

1. Add one `BranchAdoptionPlan` interface matching Rust's two snake_case string
   fields. Add optional `branch_adoption` to `WebAction`; add optional
   `confirmed` and optional `branch_adoption` to `OperationRequest`. The request
   field remains optional for existing typed callers and matches Rust's serde
   default, but `ActionBar` must always send an explicit boolean. Do not add an
   index signature or generic metadata field.
2. In `assertAction`, when `branch_adoption` is present, require a non-array
   object with string `expected_branch` and `observed_branch`. Reject null,
   missing fields, and non-string values. Leave actions without the optional
   field compatible.
3. Store `pendingAction` as `WebAction | null`. Comparison/label/class behavior
   can still use `pendingAction?.action`, but the confirmed execution must use
   the retained object, not the newly rendered argument.
4. Give `run` a concrete `confirmed` boolean. Send it on every operation.
   Include `branch_adoption` only when the action has it; use the existing
   `postOperation` and `requestId` helpers unchanged.
5. On a first confirmation-required activation, clear any older timer, retain
   that exact action object, arm the existing timeout, and return without a
   request. On the second activation for the same action ID, capture the retained
   object, clear confirmation state, then run it with `confirmed: true`.
   If it is Drop, pass the retained object into the existing delayed commit and
   have that eventual request remain confirmed. One-tap actions run the current
   object with `confirmed: false`.

## Verification commands

Run in this order and report every exit code:

1. `npm run web:test -- --run crates/ajax-web/web/src/shared/lib/contracts.test.ts -t "validates optional branch adoption metadata"`
2. `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "sends the retained branch adoption only on the confirming tap"`
3. `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "does not replace a pending adoption pair when actions refresh"`
4. `npm run web:test -- --run crates/ajax-web/web/src/features/task/ActionBar.test.tsx -t "marks ordinary actions unconfirmed and runs them immediately"`
5. `npm run web:test -- --run crates/ajax-web/web/src/shared/lib/contracts.test.ts crates/ajax-web/web/src/features/task/ActionBar.test.tsx`
6. `npm run web:check`
7. `npm run web:lint`
8. `npm run web:sg`
9. `npm run web:build`
10. `npm run web:build:check`
11. `git diff --check`

## Acceptance criteria

- Optional adoption metadata is typed and validated exactly at the JSON
  boundary; actions without it remain valid.
- First mismatch Repair tap makes no request and arms the existing timeout.
- Second tap sends `confirmed: true` with the exact pair retained from the first
  tap, not a replacement from refreshed props.
- Review and other one-tap actions remain immediate and send
  `confirmed: false`; no adoption payload is invented.
- Drop retains two-tap confirmation, delayed commit, and Undo behavior, with its
  eventual request marked confirmed.
- Only the five source/test files plus generated `dist/app.js` change and every
  verification command passes.

## Stop conditions

- Stop if safety requires browser branch comparison, a Rust/API-helper edit, or
  a seventh file.
- Stop if exact action retention breaks the existing Drop timeout/undo path
  rather than repairing that path within `ActionBar`.
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

- Round 1 was rejected and deterministically restored. Its five source/test
  files were correct and its raw report contained all requested RED/GREEN
  evidence, but the required build also modified tracked `dist/app.js`, which
  was outside `ALLOWED_SCOPE`; the raw file list omitted that scope violation.
  The adapter separately emitted `MISSING_STRUCTURED_REPORT`.
- Reapply the same bounded source behavior. Use realistic exact test metadata:
  `expected_branch: "ajax/fix-login"` and
  `observed_branch: "fix/pane-stuck"`; use a distinct observed branch only in
  the refreshed prop to prove retention.
- `OperationRequest.confirmed` may remain optional for compatibility with
  existing typed callers because Rust defaults omission to false. `ActionBar`
  must explicitly send false for one-tap actions and true for confirmed ones.
- Run the production build; retain only its tracked `dist/app.js` change. Then
  run `npm run web:build:check` and verify no other dist file changed. This is
  the sole revision round and all eleven verification commands are required.

## Parent gate result

- Round 2 accepted on 2026-07-20 after deterministic scope review showed exactly
  the five allowed source/test files plus generated `dist/app.js`, with no other
  build output changed. The raw report contained all four actual RED failures
  and all GREEN/VERIFY evidence, but the adapter again emitted
  `MISSING_STRUCTURED_REPORT`.
- Parent validation exited 0 for all eleven packet commands. The parent added
  one test-only assertion that delayed Drop sends `confirmed: true`; its first
  lint run exited 1 because two matchers violated
  `vitest/prefer-called-exactly-once-with`. The assertion was consolidated, then
  the 11-test ActionBar file, TypeScript check, lint, and diff check all exited 0.
