# Slice Architecture Alignment Plan

Status: APPROVED by Matt 2026-07-11 — full approval to implement and create PRs.
Delegation decision: delegated via model-router (Phases 1 + 3 as one bounded
packet; Phase 2 doc rewrite done directly — non-code exception).
Branch-scan risk check done 2026-07-11: no branch consumes `slices::pane`.

## Scope

Align code, tests, and `architecture.md` on the real slice architecture:
`task_operations` is the core slice layer; `ajax-web::slices` is the browser
slice layer; the kernel stays layered by authority tier.

## Non-goals

- No physical rename of `task_operations/` to `slices/` (import churn, zero behavior value).
- No new crates, no per-slice crates.
- No splitting of `models.rs`, `live.rs`, `sqlite.rs`, or `lib/tests.rs` in this plan (ratchet rules only).
- No behavior changes anywhere.

## Granularity decision (the "should we get more granular?" answer)

More granular in **contracts and tests**, not in **directories**:

- New slice only for a new operator verb (core: `task_operations/<verb>.rs`;
  web: `slices/<capability>.rs`).
- Split a verb out of `task_command.rs` only when its logic exceeds ~400
  non-test lines or diverges from the shared plan/execute path. Today it is
  200 lines for four verbs — leave it.
- Never slice the kernel (models/lifecycle/live/policy/ui_state/registry):
  single-owner task truth.
- Within-slice contract (enforced by convention + Phase 3 tests):
  `plan_*` pure → `execute_*` owns effects + receipts → private reducers.

## Tasks

### Phase 1 — Prune dead slice scaffolding (Small Fix)

- [x] Delete `crates/ajax-core/src/slices/pane.rs` and `slices.rs`; remove
      `pub mod slices;` from `lib.rs`. (Done locally by parent — critique
      round 2 flagged the 700-line deletion as over the delegate line budget
      and directed a scope split.)
- [x] Remove SLICES-based tests from `crates/ajax-core/src/architecture.rs`;
      keep `task_operations_submodules_are_file_backed` and the
      commands/registry guards. (Delegated, see Phase 3.)
- Test: existing suite (deletion proven unused — no consumers found in
  workspace search 2026-07-11; mechanical, no new tests per AGENTS.md policy).
- Verify: `cargo check --all-targets`, `cargo nextest run -p ajax-core`,
  workspace grep shows zero `slices::pane` references.

### Phase 2 — Ratify docs (mechanical, same PR)

- [ ] Rewrite `architecture.md` "Vertical Slices" section:
      - `ajax-core::task_operations` is the vertical slice layer; slices are
        operator verbs (start, resume/review/repair/ship, drop, tidy).
      - Remove `rust_arkitect` and Aroeira-migration claims (dep removed in #391).
      - Document the per-operation contract: plan pure / execute effects +
        receipts / reducers private.
      - Document authority-tier rule: every type belongs to one tier
        (intent → events → observations → projection); mutation flows downward.
      - Document presentation import rule: DTO builders consume projections
        (`output::*`, `CockpitProjection`), not `Task` internals.
- [ ] Note visibility ratchet: new core items default `pub(crate)`.
- Test: none (docs). Verify: doc matches Phase 1/3 reality.

### Phase 3 — Enforce boundaries (test-only)

- [ ] Add sibling-isolation test for `task_operations` submodules in core
      `architecture.rs` (mirror of ajax-web slice test; `kernel` exempt as
      shared plumbing).
- [ ] Add operation-shape test: each of start/task_command/drop_task/
      sweep_cleanup declares `pub fn plan_` and `pub fn execute_` entry points.
- [ ] Optional (skip if brittle): guard that only `ui_state` constructs
      operator status values outside tests.
- Test: the new tests ARE the change; confirm they pass on current tree and
  fail when a synthetic violation is introduced locally.
- Verify: `cargo nextest run -p ajax-core architecture`.

### Phase 4 — Ratchet rules (no diff now; applied opportunistically)

- Split `ajax-cli/src/lib/tests.rs` by dispatch area at first merge conflict.
- Section `models.rs` by authority tier at first collision; split along tier
  lines only.
- New reducers as free functions in tier modules, not `impl Task` methods.

## Risks

- Phase 1: pane.rs could be consumed by an unmerged branch — check
  `git branch -a` grep before deleting; if found, coordinate instead.
- Phase 3 shape test may be too rigid for a future read-only operation;
  acceptable — edit the test knowingly when that operation arrives.

## Validation commands

```
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

## Execution record (2026-07-11)

- Phase 2 doc rewrite done directly (non-code exception): architecture.md
  "Vertical Slices" section now names task_operations as the slice layer,
  documents plan/execute/reducer contract, authority tiers, presentation
  import rule, pub(crate) ratchet; removed Aroeira + rust_arkitect claims.
- Phase 3 tests delegated. Round 1: opencode/minimax-m3 hung 10 min, empty
  diff + empty log (same failure mode as prior glm-5.2 hangs). Round 2:
  codex gpt-5.5 implementation mode COMPLETE; diff reviewed, in scope.
- Packet critique: 2 rounds BLOCK (evidence formatting, doc scope, match
  counts, line budget) → packet split; third critique skipped on explicit
  user order to dispatch.

## Deviations

- Sibling-isolation test needs an allowlist: sweep_cleanup legitimately
  imports drop_task (tidy sweeps drop teardown) — encoded as the single
  allowed exception rather than refactored away.
- sweep_cleanup exposes no plan_ entry point; shape test requires plan_ only
  for start/task_command/drop_task.
- Optional ui_state sole-constructor guard skipped as brittle (plan allowed).

## Validation results (run personally 2026-07-11)

- cargo fmt --check → pass
- cargo check --all-targets → pass
- cargo clippy --all-targets --all-features -- -D warnings → pass
- cargo nextest run -p ajax-core → 769 passed
- cargo nextest run --all-features → 1564 passed
