PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Derive checkout mismatch from task intent plus observed Git facts, reduce it to
its own runtime-health verdict with correct missing/unobserved precedence, and
round-trip that verdict through the existing SQLite label codec without a
schema change.

## Allowed files

- `crates/ajax-core/src/models.rs`
- `crates/ajax-core/src/runtime.rs`
- `crates/ajax-core/src/attention.rs`
- `crates/ajax-core/src/registry/sqlite.rs`

## Forbidden changes

- Do not edit any other file or undo the accepted Task 1 diff already present.
- Do not add a field, side flag, database column, migration, dependency, or new
  checkout-state type.
- Do not classify checkout mismatch as missing substrate or as a
  `SubstrateGap`.
- Do not add user-facing wording/actions yet; that belongs to Task 3.
- Do not change branch intent, run Git/tmux commands, or alter operation policy.
- Do not perform unrelated cleanup, renames, or formatting churn.

## Context evidence

- `Task` owns expected branch intent in `task.branch` and observed Git facts in
  `task.git_status`; `Task` already derives missing-resource predicates from
  those fields. Anchor: `crates/ajax-core/src/models.rs`, methods around
  `has_missing_worktree` and `has_missing_branch`.
- `GitStatus.current_branch == None` represents detached checkout when a
  worktree is observed present; absence of `git_status` represents unobserved.
  Anchor: `GitStatus` plus accepted Task 1 parser/refresh behavior.
- Runtime reduction currently receives only `ObservedTaskRuntime`, checks
  missing worktree first, then tmux/window facts, and cannot compare checkout to
  task intent. Anchor: `crates/ajax-core/src/runtime.rs`, `reconcile_runtime`
  and `runtime_health`.
- `Task::refresh_runtime_projection_from_source` is the only production caller
  of `reconcile_runtime` and has direct access to `self.branch`. Anchor:
  `crates/ajax-core/src/models.rs`.
- Runtime-health storage writes `projection.health.as_str()` and loads through
  `RuntimeHealth::from_label`; therefore a new stable label needs no schema
  migration. Anchors: `crates/ajax-core/src/models.rs`, `RuntimeHealth`; and
  `crates/ajax-core/src/registry/sqlite.rs`, runtime projection write/load.
- `attention::substrate_gap_for_runtime_health` is the sole exhaustive
  presentation match and must explicitly keep checkout mismatch outside the
  missing-substrate taxonomy until Task 3 adds typed mismatch evidence.

## Code anchors

- `crates/ajax-core/src/models.rs`: `Task::has_missing_worktree`,
  `Task::refresh_runtime_projection_from_source`, `RuntimeHealth`,
  `tests::sample_task`, `task_status_updates_refresh_runtime_projection_health`,
  and `runtime_projection_labels_are_stable_for_storage_and_json`.
- `crates/ajax-core/src/runtime.rs`: `reconcile_runtime`, `runtime_health`, test
  helpers `git_status`/`observed`, and
  `runtime_reconciliation_collapses_substrate_evidence_into_one_health_verdict`.
- `crates/ajax-core/src/attention.rs`:
  `substrate_gap_for_runtime_health` and its inline tests.
- `crates/ajax-core/src/registry/sqlite.rs`:
  `sqlite_registry_store_round_trips_runtime_probe_failure` as the closest
  normalized-table round-trip pattern.

## Test-first instructions

Use two small red/green cycles, with no production edit before its red proof.

1. Add `models::tests::task_checkout_mismatch_requires_present_observed_worktree`.
   Cover these exact cases against expected branch `ajax/generated`: no
   `git_status` => false; present/aligned => false; present/other branch => true;
   present/detached (`current_branch: None`) => true; missing path even with an
   other current-branch value => false. Call a new
   `Task::has_checkout_mismatch()` helper. Run:
   `cargo test -p ajax-core task_checkout_mismatch_requires_present_observed_worktree -- --nocapture`
   and capture the expected compile failure that the helper does not exist.
   Then implement only the minimal helper and rerun the same command green.

2. Before changing `RuntimeHealth` or the reducer:
   - Add `runtime::tests::runtime_reconciliation_distinguishes_checkout_mismatch_from_missing_or_unobserved`
     covering aligned, other-branch, detached, missing-path, and no-Git-status
     cases. Pass expected branch `ajax/fix-login`; assert mismatch precedes
     missing tmux/window, but missing path precedes mismatch.
   - Add `(RuntimeHealth::CheckoutMismatch, "checkout_mismatch")` to the stable
     label test.
   - Add
     `sqlite::tests::sqlite_registry_store_round_trips_checkout_mismatch_runtime_health`
     following the nearby runtime-projection round-trip fixture and asserting
     the exact projection survives save/load.
   - Add a focused attention assertion proving
     `substrate_gap_for_runtime_health(RuntimeHealth::CheckoutMismatch) == None`.
   Run:
   `cargo test -p ajax-core runtime_reconciliation_distinguishes_checkout_mismatch_from_missing_or_unobserved -- --nocapture`
   and capture the expected compile failure for the missing enum variant and/or
   expected-branch reducer argument. Only then implement the runtime/model/codec
   changes and rerun it green.

Do not rewrite existing assertions except to pass the new required
`expected_branch` argument or add the new stable-label row.

## Edit instructions

1. Add exactly one derived helper to `Task`:
   `pub fn has_checkout_mismatch(&self) -> bool`. It is true only when
   `git_status` is observed, `worktree_exists` is true, and
   `current_branch.as_deref() != Some(self.branch.as_str())`. This makes detached
   true while missing and unobserved remain false.
2. Add `RuntimeHealth::CheckoutMismatch` with stable label
   `checkout_mismatch`. Keep it false for both `is_missing_substrate()` and
   `is_git_substrate_gap()` by leaving it out of those matches.
3. Add `expected_branch: &str` to `reconcile_runtime` and its private reducer.
   Preserve precedence: no Git evidence => `Unobservable`; absent path =>
   `MissingWorktree`; present path with different or detached current checkout
   => `CheckoutMismatch`; only then evaluate tmux and task-window evidence.
4. Pass `&self.branch` from `Task::refresh_runtime_projection_from_source` and
   update existing runtime tests/callers for the signature.
5. Extend `attention::substrate_gap_for_runtime_health` exhaustively with
   `CheckoutMismatch => None`. Do not surface it there yet.
6. Rely on the existing SQLite `as_str`/`from_label` path; add no SQLite
   production branch or schema change beyond the enum label methods.

## Verification commands

Run in this order and report every exit code:

1. `cargo test -p ajax-core task_checkout_mismatch_requires_present_observed_worktree -- --nocapture`
2. `cargo test -p ajax-core runtime_reconciliation_distinguishes_checkout_mismatch_from_missing_or_unobserved -- --nocapture`
3. `cargo test -p ajax-core runtime_projection_labels_are_stable_for_storage_and_json -- --nocapture`
4. `cargo test -p ajax-core sqlite_registry_store_round_trips_checkout_mismatch_runtime_health -- --nocapture`
5. `cargo test -p ajax-core runtime_reconciliation -- --nocapture`
6. `cargo check -p ajax-core --all-targets`
7. `cargo fmt --check`

## Acceptance criteria

- The derived helper returns false/aligned, true/other, true/detached,
  false/missing, and false/unobserved exactly as specified.
- Runtime health distinguishes checkout mismatch from missing worktree and
  unobservable evidence, with missing-path precedence.
- `CheckoutMismatch` serializes as `checkout_mismatch`, decodes from that label,
  and round-trips through SQLite with no migration or new column.
- Checkout mismatch is not missing substrate and produces no `SubstrateGap`.
- All existing runtime reducer cases still pass after the signature change.
- The diff stays within the four allowed files and preserves Task 1 changes.

## Stop conditions

- Stop if a database schema migration, persisted side flag, or new field is
  needed.
- Stop if a fifth production file is required to compile or preserve behavior.
- Stop if checkout mismatch must be treated as missing substrate to make tests
  pass.
- Stop on unrelated baseline failures and report them without changing
  unrelated code or tests.
