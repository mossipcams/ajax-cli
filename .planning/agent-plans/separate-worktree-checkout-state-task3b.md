PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Expose the canonical core checkout-mismatch status, explanation, and safe
actions through CLI JSON, CLI human output, and Web Cockpit without deriving or
rewriting mismatch state in any adapter.

## Allowed files

- `crates/ajax-core/src/output.rs`
- `crates/ajax-cli/src/render.rs`
- `crates/ajax-web/src/slices/cockpit.rs`

## Forbidden changes

- Do not edit any other file or undo accepted Tasks 1–3A.
- Do not compare branches, inspect `GitStatus.current_branch`, derive actions,
  or construct mismatch wording in CLI/Web production code.
- Do not add a DTO field/type; the existing status, explanation, and actions
  fields are sufficient.
- Do not remove or rename existing JSON keys. This change is additive.
- Do not alter browser action filtering, operation execution, or terminal code.
- Do not perform unrelated cleanup or formatting churn.

## Context evidence

- `commands::projection::task_summary` already fills `TaskSummary.status`,
  `status_explanation`, and `actions` from canonical core decisions, but
  `TaskSummary` marks all three `skip_serializing`, so CLI JSON hides them.
  Anchor: `crates/ajax-core/src/output.rs`, `TaskSummary`.
- CLI human rendering already formats `TaskSummary.status_explanation`; generic
  `render_response` serializes the same response for JSON. Anchor:
  `crates/ajax-cli/src/render.rs`, `render_task_summary` and `render_response`.
- Web Cockpit `BrowserTaskCard` already clones `TaskCard.status_explanation` and
  obtains actions from the core card. Browser detail does the same. Anchor:
  `crates/ajax-web/src/slices/cockpit.rs`; no Web production edit is expected.
- Task 3A guarantees the canonical named mismatch explanation and the
  `[Repair, Resume]` core action set.

## Code anchors

- `crates/ajax-core/src/output.rs`: `TaskSummary` serde attributes and
  `tests::read_commands_serialize_as_json_contracts`.
- `crates/ajax-cli/src/render.rs`: tests near
  `task_human_renders_probe_failure_status_instead_of_lifecycle`.
- `crates/ajax-web/src/slices/cockpit.rs`: tests near
  `browser_cockpit_surfaces_missing_substrate_tasks` and
  `task_detail_returns_missing_substrate_task_when_visible_in_cockpit`.

## Test-first instructions

Before changing the `TaskSummary` serde attributes:

1. Update the existing output JSON contract expectation so each serialized task
   includes `status`, `status_explanation`, and `actions`, preserving every
   existing key/assertion.
2. Add `render::tests::checkout_mismatch_renders_identically_in_human_and_json`.
   Build one `TasksResponse` with Error,
   `Worktree on fix/pane-stuck; expected ajax/fix-login`, and actions
   `repair`, `resume`. Assert the exact human line and JSON values for status,
   explanation, and actions.
3. Add
   `cockpit::tests::browser_cockpit_and_detail_pass_through_checkout_mismatch`.
   Create an active `web/fix-login` task, apply present Git evidence on
   `fix/pane-stuck`, and assert card JSON plus detail have Error, the exact core
   explanation, and ordered actions `repair`, `resume`. The test must not derive
   that wording itself in production code.
4. Run:
   `cargo test -p ajax-cli checkout_mismatch_renders_identically_in_human_and_json -- --nocapture`
   and capture the expected failure that JSON omits one or more hidden fields.
   No production edit may precede this red command.

## Edit instructions

1. In `TaskSummary`, remove only `skip_serializing` from `status`,
   `status_explanation`, and `actions`. Preserve `#[serde(default)]` on actions.
2. Do not change CLI/Web production logic. The new tests must pass through the
   existing core-derived fields unchanged.
3. Update the existing additive JSON contract expectation; do not weaken or
   delete any old-key assertions.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core read_commands_serialize_as_json_contracts -- --nocapture`
2. `cargo test -p ajax-cli checkout_mismatch_renders_identically_in_human_and_json -- --nocapture`
3. `cargo test -p ajax-web browser_cockpit_and_detail_pass_through_checkout_mismatch -- --nocapture`
4. `cargo check -p ajax-core -p ajax-cli -p ajax-web --all-targets`
5. `cargo fmt --check`

## Acceptance criteria

- CLI JSON additively includes lowercase `status`, canonical
  `status_explanation`, and ordered `actions` for every TaskSummary.
- CLI human output shows the same canonical mismatch explanation.
- Browser card and detail show the same Error/explanation/actions without Web
  branch logic.
- Existing JSON keys and missing-substrate adapter behavior remain intact.
- Only the one serde production edit is made; all other changes are focused
  tests within the three allowed files.

## Stop conditions

- Stop if adapter production code must inspect branches or construct mismatch
  wording/actions.
- Stop if a new DTO/type or destructive JSON contract change appears necessary.
- Stop on unrelated baseline failures without changing unrelated code/tests.
- Return the exact `DELEGATE_REPORT` schema inside marker lines as plain YAML.
  `FILES_CHANGED`, `STOP_CONDITIONS_HIT`, and `REMAINING_RISKS` must each be
  inline bracket lists so the router checker accepts them.
