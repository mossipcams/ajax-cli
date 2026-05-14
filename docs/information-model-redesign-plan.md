# Information Model Redesign Plan

## Status

This document is a proposal for reshaping Ajax's operator-facing information
model. It is not an implementation record. Before implementation, confirm the
open questions below and follow the repository workflow in `AGENTS.md`.

The current architecture source of truth is `architecture.md`. If this redesign
is implemented and changes architecture boundaries, update `architecture.md` in
the same work.

## Goals

Reshape Ajax around three operator-facing principles:

1. **Tasks are the primary object.** Every operator concern hangs off a task.
2. **Attention reasons are task annotations.** Attention is derived state on the
   task row, not a parallel list that can drift from the task.
3. **Commands read like operator actions.** The public vocabulary should use
   `start`, `resume`, `review`, `ship`, `drop`, `repair`, and `tidy`, rather
   than implementation-shaped verbs such as `new`, `open`, `diff`, `merge`,
   `clean`, `cleanup`, `remove`, `check`, `trunk`, and `sweep`.

## Information Model

### Task

`Task` remains the primary durable object. Registry reads should return tasks
with derived annotations attached, but annotations should not be persisted to
SQLite. Persist the underlying lifecycle, live status, side flags, and substrate
evidence instead.

Task properties that describe useful context but do not require operator
attention stay as task properties. Examples:

- `Stale`
- `Dirty`
- `Unpushed`
- `CiFailed`
- `TestsFailed`

These properties may render on rows, cards, or detail views, but they should not
create an attention item or change the primary operator action by themselves.

### Annotation

An annotation is derived task state that explains why an operator should look at
a task now.

Proposed shape:

```rust
pub struct Annotation {
    pub kind: AnnotationKind,
    pub severity: u32,
    pub evidence: Evidence,
    pub suggests: OperatorAction,
}
```

The `suggests` field is derived from `kind`. It is included in rendered and JSON
surfaces so clients do not duplicate the mapping.

### AnnotationKind

| Kind         | Meaning                                                                           | Suggested action |
|--------------|-----------------------------------------------------------------------------------|------------------|
| `NeedsMe`    | Agent is waiting on input, approval, auth, rate limit, context, or appears dead   | `Resume`         |
| `Broken`     | Substrate is wrong: worktree, tmux, branch, worktrunk, or conflicts need repair   | `Repair`         |
| `Reviewable` | Work is done enough for operator review                                           | `Review`         |
| `Cleanable`  | Work has merged or is otherwise safe to remove                                    | `Drop`           |

Recommended severity order:

1. `NeedsMe`
2. `Broken`
3. `Reviewable`
4. `Cleanable`

Lower severity numbers should win when selecting a task's primary action.

### Evidence

`Evidence` records the source fact that produced an annotation. It exists for
rendering, diagnostics, and JSON clients.

Proposed shape:

```rust
pub enum Evidence {
    LiveStatus(LiveStatusKind),
    SideFlag(SideFlag),
    Lifecycle(LifecycleStatus),
    Substrate(SubstrateGap),
}
```

When multiple facts produce the same annotation kind, collapse them to one
annotation and choose the clearest evidence for the operator. Suggested evidence
preference for `NeedsMe`:

1. `LiveStatus`
2. `SideFlag`
3. `Lifecycle`
4. `Substrate`

### OperatorAction

`OperatorAction` replaces `RecommendedAction`.

Proposed variants:

```rust
pub enum OperatorAction {
    Start,
    Resume,
    Review,
    Ship,
    Drop,
    Repair,
}
```

`Start` is available as a command but is not suggested by an existing task
annotation. It creates a task.

## Command Vocabulary

The CLI should expose operator verbs. Internal module names may lag the public
vocabulary during the transition, but public command names and rendered action
labels should converge on this table.

| Old command                         | New command          | Meaning |
|-------------------------------------|----------------------|---------|
| `ajax new ...`                      | `ajax start ...`     | Start a new task |
| `ajax open <task>`                  | `ajax resume <task>` | Re-enter an active or blocked task |
| `ajax diff <task>`                  | `ajax review <task>` | Inspect task changes |
| `ajax merge <task>`                 | `ajax ship <task>`   | Merge completed work |
| `ajax clean <task>`                 | `ajax drop <task>`   | Remove one task's local substrate |
| `ajax cleanup <task>`               | `ajax drop <task>`   | Alias removed in the clean-break option |
| `ajax remove <task>`                | `ajax drop <task>`   | Alias removed in the clean-break option |
| `ajax check <task>`                 | `ajax repair <task>` | Diagnose or repair broken substrate |
| `ajax trunk <task>`                 | `ajax repair <task>` | Fold into repair-oriented substrate handling |
| `ajax sweep`                        | `ajax tidy`          | Drop all cleanable tasks |
| `ajax review` or `ajax review --json` | `ajax ready`       | List tasks ready for review |

`review` is the single-task inspection action. `ready` is the list action.

`drop` is the single-task cleanup action. `tidy` is the bulk cleanup action.

## Open Questions

1. **CLI rename strategy.** Should the project do a clean break, or keep hidden
   aliases for one release? This plan assumes a clean break.
2. **`ajax start` flags.** Should `ajax start` accept exactly the same repo,
   title, and agent flags as today's `ajax new`? This plan assumes yes.
3. **Repair scope.** Should `ajax repair` fully absorb `check` and `trunk`, or
   should any read-only diagnostics remain separate? This plan assumes repair is
   the public operator verb and may include read-only diagnosis.
4. **JSON compatibility.** Should JSON fields be renamed in a single breaking
   change, or should compatibility fields be carried temporarily? This plan
   assumes a breaking rename to the new model.

## Implementation Phases

Each code task below should be implemented with TDD. Write the failing behavior
test first, run it to show the failure, implement the smallest passing change,
then rerun the focused test.

Markdown-only tasks are exempt from TDD, but still need an explicit verification
check.

## Phase 1: Core Model (`ajax-core`)

### Task 1.1: Introduce `OperatorAction`

- Test: add `operator_action_labels_are_operator_facing` in
  `crates/ajax-core/src/models.rs`. Assert `as_str()` returns `start`,
  `resume`, `review`, `ship`, `drop`, and `repair`.
- Implementation: add `OperatorAction` in `models.rs` with `as_str()`,
  `all()`, and `from_label()` behavior matching the current
  `RecommendedAction` style.
- Verify:

```sh
cargo nextest run -p ajax-core models::tests::operator_action_labels_are_operator_facing
```

### Task 1.2: Introduce annotations and evidence

- Test: add `annotation_kind_suggests_one_operator_action` in
  `crates/ajax-core/src/models.rs`.
- Implementation: add `AnnotationKind`, `Evidence`, and `Annotation`. Derive
  `Serialize` and `Deserialize` where needed by the existing output contracts.
- Verify:

```sh
cargo nextest run -p ajax-core models::tests::annotation_kind_suggests_one_operator_action
```

### Task 1.3: Add annotations to `Task`

- Test: add `task_carries_empty_annotations_by_default` in
  `crates/ajax-core/src/models.rs`.
- Implementation: add `pub annotations: Vec<Annotation>` to `Task`. Update
  constructors so newly built tasks start with an empty annotation list.
- Verify:

```sh
cargo nextest run -p ajax-core models::tests::task_carries_empty_annotations_by_default
```

### Task 1.4: Replace attention derivation with annotation derivation

- Tests: add focused tests in `crates/ajax-core/src/attention.rs`:
  - `annotate_collapses_blocker_evidence_into_needs_me`
  - `annotate_emits_broken_for_missing_substrate`
  - `annotate_emits_reviewable_when_lifecycle_reviewable`
  - `annotate_emits_cleanable_when_lifecycle_cleanable`
- Implementation: expose `pub fn annotate(task: &Task) -> Vec<Annotation>`.
  Collapse duplicate annotation kinds and apply the severity order documented
  above. Replace `derive_attention_items`.
- Verify:

```sh
cargo nextest run -p ajax-core attention::tests
```

### Task 1.5: Populate annotations at registry read boundaries

- Test: add `listed_tasks_carry_annotations` near the existing registry read
  tests in `ajax-core`.
- Implementation: call `annotate(&task)` on registry read paths that return
  tasks to command, JSON, or Cockpit surfaces. Do not persist annotations.
- Verify:

```sh
cargo nextest run -p ajax-core registry::
```

### Task 1.6: Make recommendations annotation-driven

- Test: add `operator_action_uses_lowest_severity_annotation` in the existing
  recommendation module.
- Implementation: replace `RecommendedActionPlan` with an
  `OperatorActionPlan` or equivalent. Select the primary action from the lowest
  severity annotation. Replace blocker/action reason strings with evidence
  rendering.
- Verify:

```sh
cargo nextest run -p ajax-core recommended::
```

### Task 1.7: Update Cockpit projection contracts

- Tests: add projection tests in `crates/ajax-core/src/commands/projection.rs`:
  - `task_card_carries_annotations`
  - `cockpit_projection_drops_parallel_attention_list`
- Implementation: add `annotations: Vec<Annotation>` to `TaskCard`; rename
  `recommended_action` to `primary_action`; remove `blocker_reason`,
  `action_reason`, and `CockpitProjection.attention`.
- Verify:

```sh
cargo nextest run -p ajax-core commands::projection::
```

### Task 1.8: Remove legacy attention and action types

- Test: compilation is the behavior check after all callers move to
  annotations and `OperatorAction`.
- Implementation: remove `AttentionItem`, `RecommendedAction`, and constants
  that encode the old action vocabulary.
- Verify:

```sh
cargo check --all-targets --all-features
```

## Phase 2: CLI Vocabulary (`ajax-cli`)

### Task 2.1: Add new command names while old names still dispatch

- Test file requiring approval under repository rules:
  `crates/ajax-cli/tests/live_cli.rs`.
- Test: add `ajax_parses_new_operator_verbs`. Assert help succeeds for
  `start`, `resume`, `review`, `ship`, `drop`, `repair`, `tidy`, and `ready`.
- Implementation: extend CLI parsing with the new verbs while old verbs remain
  temporarily available for transition stability.
- Verify:

```sh
cargo nextest run -p ajax-cli --test live_cli ajax_parses_new_operator_verbs
```

### Task 2.2: Dispatch new verbs to existing behavior

- Test file requiring approval under repository rules:
  `crates/ajax-cli/tests/live_cli.rs`.
- Tests:
  - `ajax_start_creates_task_like_new`
  - `ajax_resume_dispatches_like_open`
  - `ajax_review_dispatches_like_diff`
  - `ajax_ship_dispatches_like_merge`
  - `ajax_drop_dispatches_like_clean`
  - `ajax_repair_dispatches_like_check`
  - `ajax_tidy_dispatches_like_sweep`
  - `ajax_ready_dispatches_like_review`
- Implementation: wire each new parser branch to the current command
  implementation. Preserve behavior until the old commands are removed.
- Verify:

```sh
cargo nextest run -p ajax-cli --test live_cli
```

### Task 2.3: Rewrite smoke flows to the new vocabulary

- Test file requiring approval under repository rules:
  `crates/ajax-cli/tests/smoke_user_flows.rs`.
- Test: existing smoke tests remain the behavior tests. Update invocations only.
- Implementation: replace command calls:
  - `new` -> `start`
  - `open` -> `resume`
  - `diff` -> `review`
  - `merge` -> `ship`
  - `clean`, `cleanup`, `remove` -> `drop`
  - `check`, `trunk` -> `repair`
  - `sweep` -> `tidy`
  - list-style `review` -> `ready`
- Verify:

```sh
cargo nextest run -p ajax-cli --test smoke_user_flows
```

### Task 2.4: Remove old public verbs

- Test file requiring approval under repository rules:
  `crates/ajax-cli/tests/live_cli.rs`.
- Test: add `ajax_rejects_old_verbs`. Assert old verbs fail parsing.
- Implementation: remove old verbs from CLI construction and dispatch. Remove
  obsolete dispatch branches only after new verbs are covered.
- Verify:

```sh
cargo nextest run -p ajax-cli
```

## Phase 3: Cockpit Alignment (`ajax-tui`)

### Task 3.1: Render task annotations

- Test: add `cockpit_row_shows_annotation_label` near existing Cockpit state or
  rendering tests.
- Implementation: render annotations from task cards instead of consuming a
  separate attention list. Map `Evidence` to concise operator-facing strings.
- Verify:

```sh
cargo nextest run -p ajax-tui cockpit_state::
```

### Task 3.2: Replace inbox attention with an annotation digest

- Test: add `cockpit_inbox_lists_annotated_tasks_sorted_by_severity`.
- Implementation: build the inbox from `cards.iter().filter(|card|
  !card.annotations.is_empty())`. Sort by the primary annotation severity.
- Verify:

```sh
cargo nextest run -p ajax-tui cockpit_state::
```

### Task 3.3: Rename action chrome

- Test: add `action_chrome_uses_operator_verbs` in
  `crates/ajax-tui/src/actions.rs`.
- Implementation: update action labels, command names, and keybinding copy to
  use `Resume`, `Review`, `Ship`, `Drop`, and `Repair`.
- Verify:

```sh
cargo nextest run -p ajax-tui actions::
```

## Phase 4: Documentation

### Task 4.1: Update architecture documentation

- Documentation: update `architecture.md` to describe task annotations instead
  of attention projection. Document the public operator command vocabulary.
- Verify:

```sh
grep -n "AttentionItem\|RecommendedAction" architecture.md
```

The verification should return no matches after the redesign is implemented.

### Task 4.2: Update README examples

- Documentation: replace command examples in `README.md` with the new public
  verbs.
- Verify: read through the command examples and search for removed verbs.

### Task 4.3: Mark superseded planning docs

- Documentation: add a short status note to older planning docs that still
  reference `AttentionItem` or `RecommendedAction`, such as:
  - `docs/derived-ui-state-plan.md`
  - `docs/tui-ux-rework-plan.md`
- Verify: read through each note and confirm it points to this redesign plan.

## Test Files Requiring Explicit Approval

The implementation plan modifies these files under `tests/`:

- `crates/ajax-cli/tests/live_cli.rs`
- `crates/ajax-cli/tests/smoke_user_flows.rs`

No other files under a `tests/` directory should be modified unless a later
approved plan names them explicitly.

## Final Validation

Run the strongest applicable validation from the repository root:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

If documentation changes are included, also verify affected Markdown by
read-through and targeted searches for stale names.

Do not report any command as passing unless it was actually run and passed.
