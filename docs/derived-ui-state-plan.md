# Ajax Derived UI State Plan

## Goal

Move all "what is this task?" classification into `ajax-core` and let
`ajax-tui` consume a single typed projection. The operator's five questions —
*what needs attention, what is running, what is review-ready, what is safe to
merge, what is cleanable, what should I do next* — are all answered by core
from durable state. The TUI renders the answers; it does not compute them.

The product model: **mobile tmux is the client, Ajax is the operator control
layer.** A compact task switcher + action router, not a dashboard.

## Non-Goals

- No change to durable lifecycle storage. `Task` fields stay as-is.
- No change to CLI JSON contracts (`CockpitResponse`, `TaskSummary`, etc.).
  Existing fields keep their shapes; new fields are additive.
- No new dependencies.
- No modifications under `crates/ajax-cli/tests/`.
- Not folding `attention.rs` priorities into a config file — priorities stay
  hardcoded ladder values, just bucketed by tier.
- Help view redesign, notice severity rework — tracked in
  `docs/tui-ux-rework-plan.md`. Independent of this plan.

## Design Decisions

### One derived `UiState` per task

A pure function `derive_ui_state(&Task) -> UiState`. No mutation, no I/O.
Operator-facing tiers, ordered most-urgent first:

```rust
pub enum UiState {
    Blocked,      // operator must act: needs input, conflict, auth, ci fail,
                  // missing substrate, agent dead, lifecycle Error
    Running,      // agent actively working: AgentRunning live status, or
                  // CommandRunning / TestsRunning, no blockers
    ReviewReady,  // lifecycle Reviewable, no blockers
    SafeMerge,    // merge_safety(task).classification == Safe
    Cleanable,    // lifecycle Cleanable, or Merged + clean git + no unpushed
    Idle,         // Active/Waiting lifecycle, no work in flight, no blockers
    Failed,       // lifecycle Error
    Archived,     // lifecycle Removed
}
```

Precedence rule: a single classifier walks the tiers in order. `Blocked`
always wins over `Running` (a task with a conflict that's still spawning
commands is *blocked*, not running). `SafeMerge` requires the policy check
to pass — derivation alone can't promote a Reviewable task to SafeMerge.

### Recommended action per task, derived from `UiState`

`fn recommended_action(&Task) -> RecommendedActionPlan`:

```rust
pub struct RecommendedActionPlan {
    pub action: RecommendedAction,
    pub reason: &'static str,            // "respond to question", "merge", …
    pub available_actions: Vec<RecommendedAction>, // for the action menu
}
```

Mapping (one source of truth, replaces the per-call-site logic in
`commands/projection.rs::task_actions` and the inbox synthesis in
`attention.rs::attention_for_*`):

| UiState     | Primary action  | Reason                       |
|-------------|-----------------|------------------------------|
| Blocked     | OpenTask        | concrete blocker (see below) |
| Running     | OpenTask        | "monitor"                    |
| ReviewReady | OpenTask        | "review"                     |
| SafeMerge   | MergeTask       | "merge"                      |
| Cleanable   | CleanTask       | "clean"                      |
| Idle        | OpenTask        | "open"                       |
| Failed      | OpenTask        | "recover"                    |
| Archived    | RemoveTask      | "remove"                     |

Blocker reasons stay strings the operator reads ("waiting for approval",
"merge conflict needs attention", "worktrunk missing"). They come from the
same priority ladder `attention.rs` already uses — single helper, shared
between projection and inbox derivation.

### Cross-task `next_recommendation`

`fn next_recommendation(&[Task]) -> Option<NextStep>` — the answer to
"what should I do next?" Picks the single highest-priority task across the
whole set:

1. Any `Blocked` (lowest attention priority value wins).
2. Otherwise any `SafeMerge` (operator-rewarding work).
3. Otherwise any `ReviewReady`.
4. Otherwise any `Cleanable`.
5. Otherwise `None` — nothing demands attention.

Replaces the current `NextResponse` which is just `inbox.items.first()`.

### Attention items become tiered

`attention.rs` keeps emitting `AttentionItem` (JSON contract intact) but its
internal priorities collapse to a typed tier:

```rust
enum AttentionTier {
    NeedsResponse, // waiting for input/approval, auth, rate, context
    Failure,       // ci/cmd failed, conflict, agent dead, tests failed
    Substrate,     // worktree/tmux/worktrunk/branch missing
    Opportunity,   // ReviewReady, SafeMerge surfaced for the attention line
    Cleanup,       // Cleanable
}
```

Priority ints derived from `(tier, sub_rank)`. `Opportunity` is new —
without it, `ReviewReady` tasks never surface in the attention line, only on
the projects list. The cockpit's attention row is the operator's reason to
keep the TUI open; it should highlight wins as well as blockers.

### Safety / merge gating

`policy.rs::cleanup_safety` already exists. Add a parallel
`merge_safety(task: &Task) -> SafetyReport`:

- `Blocked` — lifecycle not in `{Reviewable, Mergeable}`, missing substrate,
  branch missing.
- `Dangerous` — conflicted.
- `NeedsConfirmation` — dirty worktree, unpushed work.
- `Safe` — clean, ahead==0/pushed, no blockers.

`UiState::SafeMerge` derivation calls `merge_safety` and only promotes when
classification is `Safe`. This puts the "what is safe to merge?" answer in
one place, reusable by the merge command itself.

### `CockpitProjection` — what the TUI consumes

New struct in `output.rs`, returned alongside (not replacing) the existing
`CockpitResponse`:

```rust
pub struct CockpitProjection {
    pub counts: CockpitSummary,           // unchanged
    pub cards: Vec<TaskCard>,             // typed, ordered by UiState tier
    pub attention: Vec<AttentionItem>,    // unchanged shape
    pub next: Option<NextStep>,
}

pub struct TaskCard {
    pub id: TaskId,
    pub qualified_handle: String,
    pub title: String,
    pub ui_state: UiState,                  // typed
    pub lifecycle: LifecycleStatus,         // typed (was string)
    pub recommended_action: RecommendedAction,
    pub action_reason: &'static str,
    pub available_actions: Vec<RecommendedAction>,
    pub live_summary: Option<String>,       // pre-rendered for the row
    pub blocker_reason: Option<String>,     // None unless ui_state == Blocked
}

pub struct NextStep {
    pub task_id: TaskId,
    pub task_handle: String,
    pub ui_state: UiState,
    pub action: RecommendedAction,
    pub reason: String,
}
```

The TUI drops `TaskSummary.lifecycle_status: String` parsing, drops the
`is_waiting_for_input(&str)` sniff at `cockpit_state.rs:423`, and renders
the badge/glyph straight from `ui_state`.

### Architectural rules

- `ajax-core` is the only crate that mentions `LifecycleStatus`, `SideFlag`,
  `LiveStatusKind`, `AgentRuntimeStatus`. The TUI imports `UiState`,
  `RecommendedAction`, `TaskCard`, `CockpitProjection`.
- Mirror the existing
  `production_code_does_not_assign_lifecycle_status_outside_authority_module`
  test (lifecycle.rs:368): add a module-level test in `ui_state.rs` that
  `ajax-tui/src/*` does not pattern-match on `LifecycleStatus`,
  `SideFlag::`, or string-compare `"WaitingForInput"`. Enforces the seam.

## Phases

Each phase is independently mergeable; later phases assume earlier ones.

### Phase 1 — `UiState` enum + derivation

- Add `crates/ajax-core/src/ui_state.rs`. Define `UiState`,
  `derive_ui_state(&Task) -> UiState`, with all variants computed from
  existing fields (no new fields on `Task`).
- Re-export from `ajax-core::models` for ergonomics.
- Module-level tests covering each variant, plus the precedence cases:
  Blocked > Running, ReviewReady < SafeMerge (only with merge_safety),
  Cleanable < SafeMerge.
- No callers yet. Pure additive change.

### Phase 2 — `merge_safety` in `policy.rs`

- Add `merge_safety(&Task) -> SafetyReport` next to `cleanup_safety`,
  mirroring its style. Reuse `mark`/`severity` helpers.
- Wire `derive_ui_state` to call `merge_safety` for the `SafeMerge` gate.
- Tests modeled on the cleanup property/rstest table (policy.rs:221+).

### Phase 3 — Recommended-action engine

- Add `crates/ajax-core/src/recommended.rs` with `recommended_action(&Task)`
  and `next_recommendation(&[Task])`.
- Tier ladder + blocker reasons extracted into a private helper shared with
  `attention.rs`. Eliminates the duplicated `attention_for_flag` /
  `attention_for_live_status` mappings.
- `attention.rs` calls the shared helper; behavior preserved by existing
  attention tests (`equivalent_waiting_attention_collapses_to_one_open_task_item`,
  etc.).

### Phase 4 — `AttentionTier` + Opportunity items

- Introduce `AttentionTier` enum private to `attention.rs`. Replace magic
  priority ints with `(tier, sub_rank) -> u32`. No external API change.
- Emit Opportunity items for `UiState::ReviewReady` and `SafeMerge` so the
  attention line shows wins, not just blockers.
- Update inbox tests to assert tier ordering, not literal priority numbers.

### Phase 5 — `CockpitProjection` + `TaskCard`

- Add `CockpitProjection`, `TaskCard`, `NextStep` to `output.rs`.
- Add `cockpit_projection(...)` in `commands/projection.rs` next to
  `cockpit_summary`. The CLI's existing `CockpitResponse` shape stays;
  projection is built alongside.
- TUI's runtime / refresh path consumes `CockpitProjection`. Update
  `cockpit_state.rs`:
  - `App.tasks: TasksResponse` → `App.cards: Vec<TaskCard>`.
  - `find_task_for_handle` lookups become `TaskId`-keyed.
  - `is_waiting_for_input` and `task.lifecycle_status` string sniffs delete.
  - `SelectableKind::Task(TaskSummary)` → `SelectableKind::Task(TaskCard)`.
- `lib.rs` rendering reads `card.ui_state` for the badge color/glyph and
  `card.recommended_action` for the primary action label.

### Phase 6 — Enforce the seam

- Add the architectural test described above to `ui_state.rs`: scan
  `crates/ajax-tui/src/*.rs` source; fail if it mentions `LifecycleStatus`,
  `SideFlag`, `LiveStatusKind`, or `AgentRuntimeStatus` outside of imports
  of the projection types.
- Delete dead helpers in the TUI that the projection has replaced.

## Out of Scope (Follow-ups)

- Persisting per-task notice state in core (currently lives in the TUI's
  `notices` map per the rework plan). The projection has all the data the
  TUI needs to do this in-memory; core ownership is a later step.
- Per-row inline UiState badges in non-selected rows.
- Replacing `TaskSummary.lifecycle_status: String` in the CLI JSON output —
  would break existing consumers. Out of scope; the projection is the typed
  path forward, JSON stays string for compatibility.
