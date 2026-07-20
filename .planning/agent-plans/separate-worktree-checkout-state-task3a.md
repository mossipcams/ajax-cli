PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Make checkout mismatch a canonical core Error projection with detailed
expected/observed wording, typed Broken evidence, a Repair recommendation, and
only Repair plus Resume as currently safe actions—without calling it missing or
putting branch policy in an adapter.

## Allowed files

- `crates/ajax-core/src/models.rs`
- `crates/ajax-core/src/ui_state.rs`
- `crates/ajax-core/src/attention.rs`
- `crates/ajax-core/src/recommended.rs`
- `crates/ajax-core/src/commands/projection.rs`

## Forbidden changes

- Do not edit any other file or undo accepted Task 1/2 changes.
- Do not add a side flag, `SubstrateGap` variant, runtime-health variant, field,
  database change, dependency, or adapter-side branch comparison.
- Do not classify mismatch as missing substrate or emit “worktree missing.”
- Do not expose Ship, Drop, or Cleanup while mismatch remains. Review/Check
  behavior belongs to Task 4; this task exposes only Repair and Resume.
- Do not implement Repair adoption or run Git/tmux commands.
- Do not perform unrelated cleanup or formatting churn.

## Context evidence

- Task 2 added `Task::has_checkout_mismatch()` and
  `RuntimeHealth::CheckoutMismatch`, with mismatch explicitly outside
  `has_missing_substrate()` and `SubstrateGap`. Anchors:
  `crates/ajax-core/src/models.rs`, `Task` and `RuntimeHealth`.
- `ui_state::derive_task_status` is the canonical status/explanation decision
  and currently checks observation errors, then missing substrate, then other
  errors. It has task intent and observed Git evidence available.
- `attention::annotate` derives current annotations; a Broken annotation suggests
  Repair. Existing evidence types do not represent a non-missing checkout
  mismatch, so one static typed evidence variant is required.
- `recommended::operator_action` derives its reason from annotation evidence and
  filters the suggested action through `available_operator_actions`. Without an
  explicit mismatch action set, reviewable tasks can still expose Ship/Drop.
- `commands::projection::task_card` already carries canonical status,
  explanation, annotation, and core actions to every adapter. Its
  `attention_reason` is the right core location to reuse the dynamic explanation
  for mismatch inbox text.

## Code anchors

- `crates/ajax-core/src/models.rs`: `Evidence`, `Evidence::label`,
  `attention_label`, `Annotation::row_label`, and adjacent annotation tests.
- `crates/ajax-core/src/ui_state.rs`: `derive_task_status`,
  `canonical_missing_substrate_explanation`, `tests::base_task`, and canonical
  status tests.
- `crates/ajax-core/src/attention.rs`: `annotate`, `evidence_preference`, and
  the runtime-health/annotation tests.
- `crates/ajax-core/src/recommended.rs`: `evidence_label`,
  `available_operator_actions`, `primary_blocker_reason`, and
  `clean_reviewable_task` tests.
- `crates/ajax-core/src/commands/projection.rs`: `task_card`,
  `attention_reason`, `cockpit_projection`, and adjacent card tests.

## Test-first instructions

Use four small red/green cycles in this order.

1. Add
   `ui_state::tests::checkout_mismatch_status_names_observed_and_expected_checkout`.
   Use active `web/fix-login` tasks with a present Git status on
   `fix/pane-stuck` and with `current_branch: None`. Assert Error with exact
   explanations `Worktree on fix/pane-stuck; expected ajax/fix-login` and
   `Worktree detached; expected ajax/fix-login`; assert neither contains
   “missing” and `has_missing_substrate()` is false. Run:
   `cargo test -p ajax-core checkout_mismatch_status_names_observed_and_expected_checkout -- --nocapture`
   and capture the current Idle/non-mismatch failure. Then implement only the
   canonical status branch and rerun green.

2. Add a model test for `Evidence::CheckoutMismatch` labels and
   `attention::tests::annotate_emits_broken_for_checkout_mismatch_without_substrate_gap`.
   The latter must set present/other-branch Git evidence and assert exactly one
   `AnnotationKind::Broken` with `Evidence::CheckoutMismatch`, suggesting
   Repair, while `has_missing_substrate()` stays false. Run:
   `cargo test -p ajax-core annotate_emits_broken_for_checkout_mismatch_without_substrate_gap -- --nocapture`
   and capture the expected missing-variant compile failure. Then add the
   evidence variant/labels and annotation reduction, update exhaustive
   preference matches, and rerun green.

3. Add
   `recommended::tests::checkout_mismatch_recommends_repair_and_only_safe_terminal_access`.
   Start from a clean reviewable task, change only `current_branch` to
   `fix/pane-stuck`, then assert primary Repair, reason `checkout_mismatch`, and
   exactly `[Repair, Resume]`—no Ship or Drop. Run:
   `cargo test -p ajax-core checkout_mismatch_recommends_repair_and_only_safe_terminal_access -- --nocapture`
   and capture the current action-set failure. Then implement only the mismatch
   evidence label/action branch and rerun green.

4. Add
   `commands::projection::tests::checkout_mismatch_card_and_inbox_share_canonical_explanation`.
   Assert card Error, exact dynamic explanation, Broken mismatch annotation,
   primary Repair, `[Repair, Resume]`, and the same dynamic explanation in
   `cockpit_projection(...).next.reason`. Run:
   `cargo test -p ajax-core checkout_mismatch_card_and_inbox_share_canonical_explanation -- --nocapture`
   and capture the current static `checkout_mismatch` inbox-reason failure. Then
   make `attention_reason` reuse the card explanation for this typed evidence
   and rerun green.

Do not edit production logic for a cycle until its focused red command has run.

## Edit instructions

1. In `derive_task_status`, after observation-error precedence and before
   missing substrate, detect `task.has_checkout_mismatch()` or persisted
   `RuntimeHealth::CheckoutMismatch`. Return Error and build the exact named or
   detached explanation from `task.git_status.current_branch` and `task.branch`.
2. Add only `Evidence::CheckoutMismatch` (no payload). Its human label and
   attention label are `checkout mismatch`; its machine reason in
   `recommended::evidence_label` is `checkout_mismatch`.
3. When runtime health/helper indicates mismatch, `annotate` emits Broken with
   this evidence. Treat it with substrate-level preference for Broken but never
   wrap it in `Evidence::Substrate`.
4. In `available_operator_actions`, return exactly Repair then Resume for
   checkout mismatch before normal lifecycle eligibility can add Ship/Drop.
   `operator_action` should then naturally select Repair from the Broken
   annotation. Add a static primary blocker reason only if an existing caller
   requires it.
5. In projection `attention_reason`, use `card.status_explanation` for
   `Evidence::CheckoutMismatch`, just as workflow-boundary evidence already
   reuses canonical explanation. Do not reconstruct branch wording there.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core checkout_mismatch_status_names_observed_and_expected_checkout -- --nocapture`
2. `cargo test -p ajax-core annotate_emits_broken_for_checkout_mismatch_without_substrate_gap -- --nocapture`
3. `cargo test -p ajax-core checkout_mismatch_recommends_repair_and_only_safe_terminal_access -- --nocapture`
4. `cargo test -p ajax-core checkout_mismatch_card_and_inbox_share_canonical_explanation -- --nocapture`
5. `cargo test -p ajax-core checkout_mismatch -- --nocapture`
6. `cargo check -p ajax-core --all-targets`
7. `cargo fmt --check`

## Acceptance criteria

- Named and detached mismatches project Error with the exact approved wording
  and never mention “missing.”
- Mismatch remains outside `has_missing_substrate` and `SubstrateGap`.
- Exactly one Broken mismatch annotation suggests Repair.
- Core recommends Repair with only Repair + Resume available at this stage.
- TaskCard and Cockpit inbox reuse the same canonical dynamic explanation.
- All focused/grouped tests and core check pass within the five allowed files.

## Stop conditions

- Stop if a side flag, `SubstrateGap`, adapter change, or sixth file is needed.
- Stop if Ship/Drop must remain exposed to satisfy an existing test; report the
  conflicting policy rather than weakening the new safety assertion.
- Stop on unrelated baseline failures without editing unrelated code/tests.
- Return the exact `DELEGATE_REPORT` schema inside marker lines as plain YAML;
  do not use Markdown fences, alternate keys, or extra report sections.
