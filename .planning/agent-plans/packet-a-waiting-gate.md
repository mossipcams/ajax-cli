# TDD Implementation Packet A — dwell-confirmed busy→waiting gate (ajax-core)

## 1. Goal

Busy→waiting live observations from the ordinary (pane) path must persist for a 4-second dwell window before they are applied to task state, so a transient "Waiting" misclassification while an agent is working never changes visible status, side flags, agent status, or fires a webhook. Includes the behavior-preserving `LiveStatusClass` enabler used by the gate and existing reducers.

## 2. Allowed files

Production (all edits are in these files; their embedded `#[cfg(test)]` modules are also where tests go):
- `crates/ajax-core/src/models.rs` — add `LiveStatusClass` enum + `LiveStatusKind::class()`
- `crates/ajax-core/src/ui_state.rs` — rewire one helper + add tests
- `crates/ajax-core/src/attention.rs` — rewire one mapping fn + add tests
- `crates/ajax-core/src/live_application.rs` — the gate + tests
- `crates/ajax-core/src/live.rs` — re-export only (one line in the existing `pub use application::{...}` at live.rs:6)
- `crates/ajax-core/src/runtime_refresh.rs` — two `continue`-condition edits + one test

Do NOT edit any file outside this list. Do NOT edit files under any `tests/` directory.

## 3. Forbidden changes

- No changes to `TaskStatus` variants, JSON/web contracts, `OperatorStatus`, projection structs, or any frontend/web/CLI crate.
- No changes to `classify_agent_pane` / `classify_pane` internals, `decide_hook_observation`, `select_status_observation`.
- No changes to `take_attention_transition` logic in attention.rs (only `annotation_kind_for_live_status` is rewired; tests may be added).
- No changes to `acknowledge_attention`, lifecycle transitions, SQLite schema, or migrations.
- No changes to `apply_authoritative_observation_at` / `apply_trusted_observation_at` behavior (they stay ungated).
- Do not weaken, delete, or rewrite any existing test or assertion.
- No formatting sweeps, renames, or drive-by cleanup.

## 4. Architecture context

(Boundaries from architecture.md, verified against source; graphify map not present — anchors below come from direct source reads.)

- Core owns task truth; `live.rs`/`live_application.rs` is the only path that applies observations to task state. UI/CLI/web only consume derived status — gating at apply automatically fixes all consumers.
- Three apply paths exist: ordinary pane (`apply_observation[_at]`), hook-authoritative (`apply_authoritative_observation_at`), trusted wrapper (`apply_trusted_observation_at`). ONLY the ordinary path is gated.
- `task.metadata: HashMap<String,String>` persists via `registry_task_metadata` (sqlite.rs:1181) — precedent: `attention::LAST_NOTIFIED_STATUS_KEY`. Use it for the candidate stamp; no schema change.
- `runtime_refresh.rs` short-circuits when a new observation equals current `live_status` (pane path ~line 426-433 `live_status_unchanged`, hook/wrapper path ~line 363-372). During a busy/waiting flap the busy samples never reach the apply path, so the short-circuit must fall through when a candidate is pending (else a stale candidate confirms falsely later).

## 5. Code anchors

- `models.rs:187` `pub enum LiveStatusKind` — 18 variants. Class membership MUST mirror existing lists exactly:
  - Waiting: `WaitingForApproval, WaitingForInput, AuthRequired, RateLimited, ContextLimit, Done` (mirrors `ui_state::live_evidence_is_acknowledged` list at ui_state.rs:127-135 and `canonical_waiting_explanation` at ui_state.rs:158-168)
  - Error: `CiFailed, MergeConflict, CommandFailed, Blocked` (mirrors `canonical_error_explanation` ui_state.rs:170-178)
  - Running: `AgentRunning, CommandRunning, TestsRunning` (mirrors `canonical_running_explanation` ui_state.rs:149-156)
  - MissingSubstrate: `WorktreeMissing, TmuxMissing, TaskWindowMissing`
  - Neutral: `ShellIdle, Unknown`
- `ui_state.rs:123` `fn live_evidence_is_acknowledged` — replace its 6-kind `matches!` with `live.kind.class() == LiveStatusClass::Waiting`.
- `attention.rs:181` `fn annotation_kind_for_live_status` — rewrite to match on `kind.class()`, keeping `LiveStatusKind::Done => Some(AnnotationKind::Reviewable)` as the leading explicit arm (Done is Waiting-class but maps to Reviewable, not NeedsMe).
- `live_application.rs:13` `pub fn apply_observation_at` — gate goes here, after `reduce_task_live_observation`, before `apply_reduced_observation`.
- `live_application.rs:66` `fn apply_reduced_observation` — candidate clearing goes at its end (after the match; note the early `return` in the `Unknown` arm at :142-147 must also clear the candidate — put clearing BEFORE the match or handle both exits).
- `live_application.rs:98-101` waiting arm sets `agent_status = Waiting`, adds `SideFlag::NeedsInput` — this is what the gate must prevent for unconfirmed observations.
- `live.rs:6` `pub use application::{...}` — add `has_pending_waiting_candidate` (and the key const if tests elsewhere need it).
- `runtime_refresh.rs:426-433` pane-path short-circuit: `if live_status_unchanged && !had_recoverable_missing_flag && !needs_agent_running_flag { continue; }`. Add `&& !live::has_pending_waiting_candidate(task)`.
- `runtime_refresh.rs:363-372` hook/wrapper short-circuit: `if live_status_unchanged && !needs_agent_running_flag { continue; }`. Same addition.
- Reuse existing test fixtures: `live_application.rs` tests `claude_active_task()` (:218), `active_task()` (:293); `attention.rs` tests `task_with_flags`, `waiting_task`; `ui_state.rs` tests `base_task()`, the `#[rstest]` truth table `canonical_status_maps_live_evidence`.
- Existing helper: `crate::live::apply_observation_at(task, LiveObservation::new(kind, "summary"), UNIX_EPOCH + Duration::from_secs(N))` is the established test idiom for timed observations.

## 6. Test-first instructions

Write tests in this order; each must FAIL before its production edit (Step 1's compile failure counts as the failing state for the new API).

Step 1 (models/ui_state/attention) — in `ui_state.rs` tests, new test `live_status_class_matches_canonical_explanations`: iterate all 18 `LiveStatusKind` variants; assert `canonical_waiting_explanation(kind).is_some() == (kind.class() == LiveStatusClass::Waiting)`, and the analogous equalities for error and running. (Make `canonical_*_explanation` visible to the test module — they are already in the same file, `super::` reaches them; widen to `pub(crate)` only if needed.)

Step 2 (live_application.rs tests, new tests, all via explicit timestamps):
1. `busy_task_defers_first_waiting_observation`: apply `AgentRunning` at t=100; apply `WaitingForInput` at t=110 → `live_status.kind == AgentRunning`, `agent_status == Running`, no `NeedsInput` flag, `metadata[WAITING_CANDIDATE_SINCE_KEY]` set.
2. `waiting_confirms_after_dwell`: continue with `WaitingForInput` at t=115 (≥4s after 110) → live_status becomes `WaitingForInput`, candidate key removed, `live_status_observed_at == t115`.
3. `waiting_within_dwell_stays_deferred`: `WaitingForInput` at t=111 → still `AgentRunning`, candidate value unchanged (first-seen 110 kept).
4. `busy_observation_clears_pending_candidate`: candidate set, then apply `AgentRunning` → candidate key gone; later `WaitingForInput` starts a fresh candidate.
5. `non_busy_task_applies_waiting_immediately`: task without running-class evidence + `WaitingForInput` → applied immediately, no candidate.
6. `trusted_waiting_bypasses_gate`: busy task + `apply_trusted_observation_at(Done)` and `apply_authoritative_observation_at(WaitingForInput)` → applied immediately.
7. `confirmed_waiting_after_acknowledgment_projects_waiting`: ack at t=200, busy at t=210, waiting at t=220, confirm at t=225 → `derive_operator_status(task).status == TaskStatus::Waiting`.

Step 3 (runtime_refresh.rs tests) — `pending_candidate_bypasses_unchanged_short_circuit`: seed a task whose `live_status` is `AgentRunning` AND `metadata[WAITING_CANDIDATE_SINCE_KEY]` pre-set; run a refresh whose pane classifies busy (copy the existing pane-fixture pattern used by neighbouring runtime_refresh tests); assert the candidate key is cleared afterward. Must fail before the short-circuit edit (the `continue` skips the clear).

Step 4 (attention.rs tests) — `busy_flap_does_not_fire_notification`: busy task (apply `AgentRunning` at t=100), single `WaitingForInput` at t=110 via ordinary path → `take_attention_transition == None`; second waiting at t=115 → fires exactly `Some(status == Waiting)` once, then `None`.

Focused failing-run command per step: `cargo nextest run -p ajax-core <test_name>` (or `cargo test -p ajax-core <test_name>` if nextest unavailable — say which you used).

## 7. Production edit instructions

Step 1: In `models.rs`, add
```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LiveStatusClass { Running, Waiting, Error, MissingSubstrate, Neutral }
```
and `impl LiveStatusKind { pub fn class(self) -> LiveStatusClass { ... } }` with the exact membership from §5. Rewire `ui_state.rs:123` and `attention.rs:181` as anchored. No other call sites change.

Step 2: In `live_application.rs`:
```rust
pub const WAITING_CANDIDATE_SINCE_KEY: &str = "waiting_candidate_since";
// ponytail: fixed dwell; make configurable only if a real agent needs tuning.
const WAITING_CONFIRMATION_DWELL: Duration = Duration::from_secs(4);
```
In `apply_observation_at`, after `reduce_task_live_observation`: if `observation.kind.class() == Waiting` AND task shows running-class evidence (`task.live_status.as_ref().is_some_and(|l| l.kind.class() == Running) || task.agent_status == AgentRuntimeStatus::Running || task.has_side_flag(SideFlag::AgentRunning)`):
- candidate absent or unparseable → insert `observed_at` as unix seconds, return (task untouched otherwise);
- candidate present and `observed_at` < candidate+dwell → return;
- else remove candidate and fall through to `apply_reduced_observation`.
Candidate clearing for non-waiting applies: in `apply_reduced_observation`, remove the key when `observation.kind.class() != Waiting` (must also cover the `Unknown` early-return path).
Add `pub fn has_pending_waiting_candidate(task: &Task) -> bool` (key present); re-export via `live.rs:6`.
Timestamp parse/format: plain `u64` seconds via `duration_since(UNIX_EPOCH)`, malformed → treat as absent.

Step 3: Add `&& !live::has_pending_waiting_candidate(task)` to both `continue` conditions anchored in §5.

Step 4: tests only.

## 8. Verification commands

```bash
cargo nextest run -p ajax-core live_application
cargo nextest run -p ajax-core ui_state
cargo nextest run -p ajax-core attention
cargo nextest run -p ajax-core runtime_refresh
cargo nextest run -p ajax-core
cargo nextest run -p ajax-cli notify
cargo fmt --check
cargo clippy -p ajax-core --all-targets -- -D warnings
```

## 9. Acceptance criteria

- Each new test failed (or failed to compile, Step 1) before its production edit and passes after — report the before/after for each step.
- All pre-existing tests in ajax-core and ajax-cli pass unmodified. In particular the ui_state `#[rstest]` truth table `canonical_status_maps_live_evidence` passes UNCHANGED (its cases apply waiting/error kinds to a task that is NOT busy, so the gate must not affect them — if any case starts failing, stop, do not edit the table).
- `git diff` confined to the six allowed files; no test weakened; patch well under 400 changed lines.

## 10. Stop conditions

- Any pre-existing test fails and the fix would require editing it → stop and report.
- The gate requires touching `apply_trusted_observation_at`/`apply_authoritative_observation_at` behavior → stop.
- A new test passes BEFORE its production edit (except Step 1 compile-gated test) → stop and report.
- runtime_refresh test fixture pattern can't be found/reused → stop, name what's missing.
- Required edit falls outside Allowed files → stop.
