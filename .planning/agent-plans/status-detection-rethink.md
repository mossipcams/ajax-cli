# Plan: Rethink status detection (false-positive elimination)

Status: **approved — implementing tasks 1–3**
Delegation decision: not delegated — `model-router` skill unavailable in this
environment (reported to user); implementing directly per AGENTS.md exception.

User decisions (2026-07-16):
- Worst false positives: agent-working, blocked, idle, done.
- Generic keyword fallback is deleted outright (no config flag).

## Problem

Operator statuses (Running / Waiting / Idle / Error) are full of false
positives. Root causes found in current code:

1. **Scrollback pollution.** `adapters/tmux.rs::capture_pane` grabs 80 lines of
   scrollback (`capture-pane -p -S -80`). `live.rs::classify_recent_evidence`
   scans *all* meaningful lines bottom-up and returns the first keyword hit
   anywhere in those 80 lines. Only the "stuck status" path
   (`project_pane_stuck_status`) restricts to the 8-line tail; the main
   evidence path does not.
2. **Content vs. chrome confusion.** Keyword tables in `pane_evidence`
   ("merge conflict", "ci failed", "exit code", "failed with", "continue?",
   "did you mean", "thinking", "running ", …) match agent-authored prose, not
   just terminal UI. Agents routinely write these words while working.
3. **Error-class pane evidence bypasses the dwell gate** and applies
   immediately — one scrollback "exit code 1" from a command the agent already
   recovered from flips the task to Error and fires an actionable webhook.
4. **Accreted mitigations treat symptoms.** Dwell windows, confidence
   degrades, current-line promotion, and stuck-tail windows are patches over a
   classifier whose inputs are unreliable. Classification is also spread
   across `live.rs`, `agent_status.rs`, `events.rs`, and
   `ajax-supervisor/agent/cursor.rs` (which reuses `classify_pane`).

## Target architecture

Principles:

1. **Pane text is demoted to weak hints.** Pane captures may only ever
   produce two non-actionable hints — `Busy` and `IdlePrompt` — derived from
   the *visible screen tail only*, never scrollback.
2. **Actionable statuses require structured evidence.** Waiting/Error
   statuses that fire attention webhooks may only be asserted by: provider
   hooks, structured lifecycle events, wrapper exit snapshots, or git/`gh`
   substrate evidence. Pane text alone never sets an actionable status.
3. **Structural recognition, not keyword search.** Per-agent recognizer
   modules (`live/recognize/{claude,codex,cursor}.rs`) parse screen zones
   (bottom prompt line, status bar, stream-json) with explicit positional
   anchors. The generic cross-agent keyword tables are deleted.
4. **One classification entry point.** Supervisor stops reusing
   `classify_pane`; stream-json parsing has a single owner.

## Non-goals

- No change to the operator status vocabulary (Running/Waiting/Idle/Error).
- No change to lifecycle semantics, registry truth, or the reducer's
  run-graph/precedence model (`agent_status.rs` stays; its inputs change).
- No new notification channels or UI changes.

## Task checklist

- [x] 0. Collect false-positive classes from operator: working, blocked,
      idle, done. No real captures provided — corpus seeded from realistic
      synthetic panes (Claude/Codex/Cursor chrome) + existing fixtures.
- [x] 1. Characterization: `live_recognize.rs` ships with a golden corpus
      (true positives: Claude/Codex/Cursor anchored chrome; FP regressions:
      prose about merge conflicts, CI, exit codes, quoted questions,
      completion prose, generic busy words, scrollback-anchored busy lines).
      Additional projection-level FP tests in `live.rs`.
      - verify: `cargo nextest run -p ajax-core live` → 173 passed
- [x] 2. Pane evidence demoted: `capture_pane` is visible-pane only (no
      `-S -80` scrollback); `project_pane_activity` emits only Busy /
      IdlePrompt / ApprovalPrompt hints via structural recognition;
      `project_pane_stuck_status` and all generic keyword tables deleted;
      runtime_refresh stuck fallback removed; monitor-event text payloads no
      longer keyword-classified (tool-call test-runner invocations still
      yield `TestsRunning`); supervisor cursor adapter no longer reuses
      `classify_pane`.
      - verify: `cargo nextest run -p ajax-core -p ajax-supervisor` → green
- [x] 3. Structural recognizers in `live_recognize.rs` (footer-anchored busy,
      bottom-anchored prompt chrome, stream-json). Single classification entry
      point (`recognize_pane`); `classify_pane` / `classify_agent_pane`
      deleted; per-agent modules were not split into separate files — one
      focused module keeps the recognizer set cohesive (deviation from the
      original sketch, recorded).
      - verify: `cargo nextest run -p ajax-core -p ajax-supervisor` → green
- [x] 4. `architecture.md` updated: Task Authority Model precedence +
      Live Status section now record visible-pane-only capture, three-hint
      pane contract, no keyword fallback, and structured-source ownership of
      failure/stuck/completion statuses.
- [ ] 5. Full verify gate: `npm run verify` + pre-commit suite per AGENTS.md
      (run before PR creation; not yet run — no PR requested yet)

## Validation results (2026-07-16)

- `cargo nextest run --workspace --all-features` → 1707 passed, 0 failed
- `cargo clippy --all-targets --all-features -- -D warnings` → clean
- `cargo fmt --check` → clean
- `cargo check --workspace --all-targets` → clean

## Self code review (2026-07-16, review-only, not delegated)

Found and fixed:
1. **Terminal stream-json short-circuit bug**: `recognize_stream_json`
   scanned bottom-up with `find_map`, skipping terminal `result`/status
   events and resurrecting stale `thinking` events above them — a finished
   Cursor run kept classifying Busy (a working false positive, the exact
   class being eliminated). Fixed with an explicit
   `NotStreamJson / Terminal / Hint` tri-state; terminal events stop the
   scan. Regression test added.
2. **`FOOTER_WINDOW` 4 → 8** (old `BUSY_WINDOW`): Codex draws multi-row
   status chrome below the busy line; 4 was too tight and would have
   produced working false negatives.
3. Removed redundant `esc to interrupt` needle (subsumed by `to interrupt`).
4. Added missing coverage: cross-agent structural prompt recognition
   (fallback path was untested), assistant-statement busy hint, terminal
   suppression.
5. Deleted dead `reduce_agent_status_values` + `agent_status_priority`
   (no production callers; self-tests only) per cleanup policy.

Accepted / documented, not changed:
- `RateLimited`, `AuthRequired`, `ContextLimit`, `ShellIdle` now have no
  producers (pane was the only source). Kinds remain for stored-row
  compatibility; follow-up candidates are hook/wrapper-based detection.
- `working (` busy needle can match prose in the footer region (pre-existing
  needle, now footer-anchored so the blast radius is small).
- `invokes_test_runner` substring match can misfire on e.g. `pip install
  pytest` in a tool-call command; narrow, kept simple.
- Cursor `--print` mode: run completion is owned by the wrapper exit; pane
  terminal events intentionally yield no hint.

## Risks

- **False negatives**: missed waiting states for agents without hook coverage
  (`AgentClient::Other`). Mitigation: pane hints still drive non-actionable
  display; idle is the safe default.
- **Hook coverage gaps** become more visible once pane text can no longer
  assert waiting/approval. May need hook setup for more agents (follow-up).
- Behavior change is user-visible (statuses change) — intentional, but expect
  a settling period of corpus additions.

## Validation strategy

Corpus-driven golden tests from real `capture-pane` output, focused
`cargo nextest run -p ajax-core` / `-p ajax-supervisor`, then full verify gate
before PR.

## Deviations

- Recognizers kept in one `live_recognize.rs` module instead of per-agent
  files; the recognizer set is small and cohesive.
- `TestsRunning` detection retained for tool-call invocations (structured
  command evidence, not prose) after supervisor tests pinned the contract.
- `AgentClient::Other` keeps the structural cross-check over Claude/Codex
  prompt shapes (existing pinned behavior), but never keyword matching.
- No real operator captures were provided; corpus is synthetic-but-realistic
  plus fixtures mined from existing tests. Real-world captures should be
  added as they surface.
