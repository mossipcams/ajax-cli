# Agent Watcher (automatic progress supervision)

Approval: user approved implementation; final C3/C4 worker explicitly authorized
in-process on `feat/agent-watcher/automatic-progress-supervision` (PR #1196).
No commit, push, branch switch, or PR change is authorized in this round.

## Current design and scope

- `ajax web` hosts the watcher; no new daemon or `/goal` mode. The socket is a
  wake-up only; canonical JSONL is durable evidence. Registry writes and delivery
  remain on the cockpit refresh lane.
- Core owns bounded watcher policy/state, separate from task lifecycle/status.
  Deterministic attention, child/tool, grace, dedupe and intervention guards
  precede the judge. The objective is the task title.
- First journal reads and events from host downtime rebuild state without
  judging or nudging. A restored cursor resumes live processing after replay.
- Judgment occurs at suspicious completed stops, repeated-signature edges and
  expired recovery grace; stale evidence, cooldowns and unavailable judges fail
  open. Pi's first activity-free stop is allowed without a judge.
- Laya is a persistent local Python sidecar. One progress choice question uses
  explicit settle/loop/grace checkpoint context. Loading and dummy inference
  finish before ready. Requests have deadlines; stale IDs are discarded and one
  malformed reply is tolerated. EOF/exit or two malformed replies retire a
  process; retry delays double from 1 second to 60 seconds, reset by a successful
  verdict. One retry warning combines bounded stderr and protocol diagnostics.
- Without a configured judge command, the host starts no watcher worker and
  reads/writes no watcher metadata. ACP delivery reuses the existing TaskSession
  queue; interactive delivery reuses fresh wrapper/process/tmux validation.
- CI and watcher delivery histories are independent. `delivery_for_task` is the
  CI-only cockpit projection; `watcher_delivery_for_task` reads the watcher store.
  Failed watcher delivery emits a warning with task ID, status and detail.
- Persisted state includes phase, episode counters, monotonic nudge sequence,
  intervention time, grace deadline, verdict and event cursor. The watcher store
  also owns the pending nudge and its delivery record. Old JSON fields are
  ignored; serde defaults and aliases preserve old metadata parsing.

## Non-goals and remaining risks

- Laya is tested with fake sidecars/fake `laya`, not a downloaded model. Real
  inference latency, model quality and dependency installation remain unverified.
- ACP detection is not implemented: ACP runs emit no native hooks. ACP nudge
  delivery is implemented and tested.
- Objective = task title; no separate richer objective model.
- No transcript storage, LLM-authored nudges, new transport/DB, task status
  variants, UI changes or multi-agent orchestration.

## Checklist

- [x] T1 core types/state/policy and inline tests.
- [x] T2 canonical signature/success evidence and translation tests.
- [x] T3 transport-neutral WatcherNudge and watcher notification store.
- [x] T4 notify wake-up, journal replay and host runtime integration.
- [x] T5 persistent Laya sidecar, explicit checkpoints and fail-open deadlines.
- [x] T6 architecture navigation and owning subsystem documentation are in the tree.
- [x] T7 invalid `apply_verdict` rustdoc link corrected in types.rs.
- [x] C3 resilient transport, diagnostic retries, inference warm-up and regressions.
- [x] C4 production delivery separation, warning and confirmed dead-state cleanup.
- [ ] C4 obsolete delivery assertions reconciled (clarification requested: C4a
  changes their contract but the additional instruction forbids assertion edits).
- [ ] Full requested gate and final review recorded below.

## Deviations / Review fixes

- Laya's Python decision API replaced the earlier HTTP chat-model assumption.
  Persistent loading and one contextual choice question replace per-request
  spawning and multiple redundant questions.
- Round A: episode budgets reset after meaningful recovery or a fresh user turn;
  monotonic nudge IDs and persisted phase preserve dedupe and escalation.
  C4 supersedes A's shared "most recent delivery" projection with CI-only labelling.
- Round B: startup/downtime replay is state-only; evidence freshness and per-task
  judge cooldown bound intervention. Stale nudges are cancelled, grace outcomes
  defer new judgments, and fresh runtime/session validation guards delivery.
  No configured judge means no watcher work or metadata churn.
- C1: hook evidence/signatures cover the supported harness payloads; Pi's first
  completed turn without activity evidence does not trigger premature-stop judgment.
- C2 (issue #1197): invalid UTF-8 is skipped; journal replacement/truncation and
  missing restore markers replay safely while retaining caps; child runs do not
  reset primary state; session close and orphan expiry clear blocked tools;
  cursor-only registry writes coalesce to one per minute. Loop checkpoints fire
  once per edge; attention/grace transitions and escalation retain correct guards.
- C3 (Fixes #1198): late and mismatched replies no longer reload a healthy model;
  malformed replies retire only on the second failure. Retry delay doubles/caps,
  successful verdicts reset it, stderr/protocol errors are visible, and the Python
  sidecar performs warm-up inference before ready.
- C4 (Fixes #1198): CI projection excludes watcher deliveries, stores retain
  independent histories, and cockpit warns on failed nudges. No TypeScript changes.
- Confirmed dead items deleted after `grep -rnE ... crates/`: `repeat_counts`,
  `bump_repeat`, `max_repeat_signatures`, `failure_count`, `settle_attempts`, and
  `last_intervention_event_index` (including persisted copy and write-only wiring);
  `WatcherPhase::{SuspectedLoop,PrematureStop}`;
  `WatcherReason::{RepeatedSignature,SuspiciousCompletion}`;
  templates for those two reasons plus `StalledAfterNudge`, `NeedsUser` and
  `InterventionCap`; and the now-unused `clear_watcher_delivery` helper/re-export.
  The latter three reasons remain live in Escalate/NeedsUser decisions. They
  cannot be queued as agent nudges. Removed only the two assertions on deleted
  counters and dead template cases; all assertions on live behavior are retained.
- State, persistence and template tests moved intact to owning inline modules;
  no standalone core tests.rs. Root agent_watcher.rs shrank below 600 lines.

## Validation results (C3/C4)

Commands run through `rtk proxy` where applicable.

- Baseline focused nextest with the five new C3/C4 regressions: all five failed
  before implementation (late reload, fixed retry cadence, single malformed
  reply, absent warm-up, watcher shown as CI).
- First post-change focused nextest compilation failed because `TaskId` does not
  implement Display in the new warning; fixed by using `task.id.as_str()`.
- `cargo nextest run -p ajax-cli -p ajax-core --all-features -E
  'test(late_reply_does_not_reload) | test(startup_error_uses_exponential) |
  test(single_malformed_and_wrong) | test(python_sidecar_warms_up) |
  test(watcher_delivery_never) | test(agent_watcher::)'`: 46/46 passed.
- Broader focused nextest and final fmt, py_compile, clippy, rustdoc, three-crate
  nextest and file-size checks: pending final results.
- Initial GitHub reads were blocked by sandbox networking; read-only escalated
  retries succeeded. Issue #1198 opened after deduplication. A diagnostic `ps`
  command was denied by the sandbox; verification did not depend on it.

Earlier plan validation (1395 tests and a rustdoc failure) is superseded by this
round's results. T6/T7 were already implemented in the input tree; the old
"no delegate route" blockers were stale.
