# Agent Watcher (automatic progress supervision)

Approval status: **APPROVED** by user ("Delegate until finished"). Delegation via model-router.

## Findings that shape the design
- Normal tasks run via tmux/agent_runtime; hooks -> `__agent-event` -> canonical JSONL + `notify.sock`.
- The only long-lived process is `ajax web` (`web_backend.rs:182` starts `start_agent_event_notify_listener`, which currently *drains* lines: `sink = None`). The watcher therefore lives in the web process; no new daemon.
- Existing delivery: `AgentNotification` (ajax-core) -> `cockpit.rs::deliver_agent_notifications` -> tmux: `ajax-cli/ci_agent_delivery.rs` (validates wrapper snapshot, `ps` comm, pane foreground) / ACP: `ajax-web/slices/ci_agent_delivery.rs` -> `TaskSessionDirectory::submit_prompt_with_id` (TaskSession queue). Pending/delivery state is stored in the CI monitor metadata (`ajax_ci_monitor`), which is CI-specific and must be generalised, not copied.
- Activity events carry only `activity_id` (tool_call_id first), which is unique per call: useless for loop detection. A stable signature is needed.
- ACP-backed tasks emit no native hooks; their only evidence is `SessionActivity` (TurnStarted/AwaitingOperator/TurnEnded/TurnFailed).
- **Laya is not a chat model.** It is an open-weight (Apache-2.0) ModernBERT decision model, Python-only (`pip install laya`; `laya.load(..).predict(state, questions)`), with typed questions `choice` / `score` / `noul` (P(true)) and per-option probabilities + confidence. No HTTP server. This supersedes the earlier "OpenAI-compatible endpoint" answer: use a Python sidecar like `scripts/ajax-moonshine-sidecar` (STT precedent).

## Scope
1. ajax-core `agent_watcher/{types,state,policy}.rs` (pure): `TaskFrame{objective}`, `WatcherSnapshot`, `WatcherDecision{NoAction,Nudge,NeedsUser,AllowStop,Escalate}`, `WatcherReason`, `ProgressState{Progressing,Stuck,OffTrack,NeedsUser,ProbablyDone,Uncertain}`, `WatcherVerdict{state,confidence}`, `AgentProgressJudge` trait (sync; host runs it off-thread), bounded `WatcherState` (ring of recent events/signatures, last meaningful activity, open tools, open children, pending attention, settle attempts, intervention count, last intervention event index, grace deadline, last verdict), `WatcherPhase` (separate from task status, e.g. `SuspectedLoop`). Policy: deterministic first (attention/child/exit/grace/cap/duplicate event_id), then "needs judge" at checkpoints only (TurnSettled::Completed that is suspicious, repeated signature >= N, no meaningful change for T, grace expiry).
2. Event evidence, smallest extension: optional `signature` (tool name + FNV-1a of normalised command summary) and `success` on `CanonicalEventDetail::Activity`; serde-default so old JSONL parses. No outputs/transcripts stored.
3. Nudge text: deterministic templates keyed by `WatcherReason`; Laya never authors text.
4. Delivery: add `AgentNotification::WatcherNudge{id,task_id,reason}` and move pending/delivery bookkeeping behind a generic per-task notification store (CI keeps its state shape). Reuse both delivery paths unchanged, so tmux validation and TaskSession queueing are inherited.
5. ajax-cli `agent_watcher_runtime.rs`: notify listener gets a real sink -> watcher thread; JSONL is durable evidence (tail replay on start, dedupe by `event_id`); judge runs with a timeout, errors/timeouts => NoAction. Active for every task with an identity; no `/goal`, no `ajax supervise`.
6. ajax-cli `laya_judge.rs` + `scripts/ajax-laya-sidecar` (Python, one-shot or persistent JSON line protocol). Questions: progress (choice), looping (noul), drifting (noul), premature stop (noul). Maps to `ProgressState`. Config `[watcher] enabled, laya_command`; unset => deterministic only (fail open).
7. Persist minimal state in task metadata key `ajax_watcher`: intervention count, last intervention event index/time, grace deadline, last verdict, last seen event_id.
8. Caps: 1 premature-stop nudge, 1 loop nudge, 2 total per run; second stop without meaningful activity => Escalate (leave for user). Meaningful activity after a nudge clears pressure.
9. Docs: `architecture.md` navigation, `docs/architecture/cli-supervisor.md` (watcher lives in CLI/web host, not supervisor), `core-subsystems.md`, `web-session-behavior.md` (ACP nudge path).

## Non-goals
Goal mode, `ajax supervise` integration, transcript storage, LLM-authored prompts, multi-agent orchestration, a second event transport or DB, new task-status variants.

## Open questions for approval
- Sidecar protocol: one-shot spawn per evaluation (simple, ~1-2s model load) vs persistent (like STT). Default proposed: persistent, started lazily.
- ACP tasks: feed the watcher from `SessionActivity` (turn-level only, so premature-stop detection but no loop signatures) in V1?

## Tasks (each a bounded delegate EXECUTION; gate = exact ci.yml/husky verify commands, incl. rustdoc -D warnings + clippy --all-targets --all-features)
- [x] T1 core types/state/policy + tests 1,2,3,4,5,8,9
- [x] T2 canonical event signature/success extension + translate tests
- [x] T3 generalise notification store + WatcherNudge; tests 6,10,11
- [x] T4 notify sink + watcher runtime wiring; tests 6,12
- [x] T5 laya_judge + sidecar; test 7
- [ ] T6 docs + architecture guard checks (BLOCKED: no delegate route available)
- [ ] T7 fix rustdoc link in agent_watcher/types.rs (BLOCKED, same)
- [ ] Verify: cargo fmt --check, clippy, nextest (workspace), rustdoc gate, file-LOC check, guard tests

## Deviations
- Laya is a Python library (not an HTTP chat endpoint): integrated as a persistent Python sidecar (`scripts/ajax-laya-sidecar`) behind `AgentProgressJudge`.
- Policy changed in review: a suspicious first stop asks the judge first (Step::NeedsJudge); no judge => no nudge (fail open).
- Runtime reads durable JSONL on socket wake (socket is only a trigger); objective = task title; no startup JSONL replay beyond per-task cursor.
- Moved ajax-core config tests to config/tests.rs (config.rs was over the LOC limit); no tests lost (fn-name diff).
- V1 limitation: ACP-backed tasks emit no native hooks, so the watcher only observes interactive/tmux runs; ACP nudge *delivery* is implemented and tested.

## Validation results
- fmt, clippy --workspace --all-targets --all-features -D warnings, py_compile: pass.
- nextest ajax-core+ajax-cli --all-features: 1395/1395 pass. ajax-web: 1 failure `restore_can_outlive_initial_handshake_budget_without_session_new`, reproduced on a clean HEAD export (pre-existing).
- RUSTDOCFLAGS=-D warnings cargo doc: FAIL, unresolved intra-doc link `apply_verdict` at crates/ajax-core/src/agent_watcher/types.rs:77 (T7, open).
- T6 docs: not done (all delegate routes unavailable: codex quota, cursor out of usage, local Qwen tunnel down).
