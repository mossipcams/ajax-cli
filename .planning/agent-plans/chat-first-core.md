# Chat-first core

## Approval status

- Direction approved by Matt on 2026-10-27: Chat is the main model over tmux;
  "Ajax chat is the main feature we should be designing around."
- Decisions recorded below (2026-10-27).
- Plan revised 2026-10-27 after a full reassessment (see Revision log).
- This plan: **approved for execution** by Matt on 2026-10-27 ("Delegate until
  finished"). Implementation runs on `ajax/architectural-flaws`, one commit per
  packet.
- Phases 2–4 change ownership, runtime authority or lifecycle semantics; each
  updates `architecture.md` and the owning focused doc in the same PR.

## Design principle

**Ajax Chat is the product.** A task is one agent conversation in its own
worktree, ending in a PR. Terminal is a tab (lazy tmux). Status is the chat's
turn state. Notifications are chat events. When chat and terminal-agent
behavior differ, chat is the default path and terminal-agent is the special
case. Terminal-agent tasks, the native TUI and tmux verbs are **frozen**:
supported, tested, no new features.

## Context

Usage: phone-first, Safari on iOS, Ajax Chat only, dev profile, web server under
launchd, frequent dev deploys, behind Cloudflare.

Measured problems, by user impact:

1. **Deploys kill agents.** Harness processes are children of `ajax-cli web`.
   On restart, startup recovery marks every prior-boot run interrupted
   (`orphan_sweep.rs`, `startup_recovery.rs`); in-flight turns are never resent.
   The user deploys dev often, so working on Ajax kills Ajax's agents.
2. **No ping when the agent finishes.** "Response ready" is not pushable
   (`ui_state.rs:289`); pushes are suppressed while the browser polled in the
   last 90 s (`runtime/state.rs:33`), and the page only polls while visible, so
   locking the phone leaves a 90 s dead zone. Push runs on a 30 s loop with a
   15 s two-observation dwell. The loop is started twice (`runtime/mod.rs:165-166`).
3. **Status lag and conflicts.** Every chat activity event clones the whole
   registry and rewrites every table (`runtime/state.rs:288-330` →
   `registry/sqlite/save.rs:28`). Chat status reaches the browser on two
   channels reconciled by 1 s polling (`TaskWorkspace.tsx:84`). Source of
   #1472, #1475, #1486 and the convergence fixes #1484, #1427, #1426, #1431.
4. **Wrong verbs for a chat task.** Ship is a local `git switch` + `git merge`
   at the repo root (`commands/merge.rs`). Fix CI / Resolve conflicts need a
   tmux pane (`remediation.rs:116-128`) although automatic CI failures already
   route through the chat queue. Resume and Repair are tmux window operations.
5. **Blocking.** Task start and harness switch hold `control_lane` for the whole
   external operation (`live.rs:382-383, 512-513`); chat attach and model saves
   wait on it (`state.rs:455, 484`). Chat attach returns `NotOrchestrationChat`
   on a revision race (`state.rs:510-512`).
6. **Data hazards.** A snapshot save missing a task deletes that task's chat
   runs in the same transaction (`save.rs:74-82`). Tasks-only loads followed by
   a save wipe events and step receipts (confirmed by probe).
7. **Chat is a flag.** `skip_interactive_agent` metadata is special-cased in 46
   places across 20 production files.

## Decisions (Matt, 2026-10-27)

1. tmux agent tasks and native TUI are **frozen**: supported and tested, no new
   features, fixes only for breakage. No removal is planned.
2. Agent-process durability may still interrupt turns when the durability layer
   itself is redeployed. **Accepted.** (With per-session shims, below, this
   only affects sessions started before the deploy, and only if the shim
   protocol changes.)
3. Notifications fire **only when Ajax web is not open**, where "open" means a
   visible page, not "polled in the last 90 s".

## Non-goals

- Removing the tmux agent path, native TUI, or any public CLI command.
- Changing the harness transports (Codex ACP, Claude SDK, Pi RPC, Cursor SDK).
- Security or network exposure changes.
- Replay/reconcile recovery for interrupted prompts (keep "interrupt, never resend").
- A typed `TaskKind` column/migration, a typed PR table, or row-level rewrite of
  all registry persistence. Removed from the plan as not needed for the goals
  above; see Revision log.

## Phases

Each phase ships independently under the 1500-line PR gate via model-router.

### Phase 0: chat-path defects and responsiveness

Defects need a GitHub issue + regression test (`docs/defect-process.md`).

- [x] Remove the duplicate `spawn_push_tick` (`runtime/mod.rs:166`).
- [x] Fix CI / Resolve conflicts on chat tasks go through the `TaskSession`
      FIFO (the automatic-CI path), not tmux send-keys.
- [x] Ship is not offered when the task has an open PR.
- [ ] Push on "Response ready" when Ajax web is not open. Browser reports
      `visibilitychange`; server "open" = a visible page within the last poll
      interval (replaces the 90 s TTL).
- [x] Task start / harness switch run external effects outside `control_lane`;
      take the lane only for the registry apply.
- [x] Chat attach retries once on revision mismatch instead of returning
      `NotOrchestrationChat`.
- [ ] Chat-run deletion moves out of `save_registry` into Drop only.

### Phase 1: contract

- [x] `PRODUCT.md`: Chat is the work surface; Terminal is a tab.
- [x] `architecture.md` + `AGENTS.md`: a "Chat-first" section stating the
      design principle, the frozen status of terminal-agent/TUI/tmux verbs, and
      that the live chat session is the authoritative substrate for turn state
      (same standing as Git and tmux). Detailed doc rewrites land with Phases 2–4.
- [x] `Task::kind()` accessor landed (`8cee4c3c`, `feat(core)`); call sites
      convert when touched.
- [ ] Add `Task::kind() -> TaskKind { Chat, TerminalAgent }` as a typed accessor
      over the existing metadata; no migration. New code uses it; existing call
      sites convert when touched.

### Phase 2: sessions survive web restarts (per-session shim)

The highest-value change, moved ahead of all refactoring.

- [x] `ajax-cli session-shim --handle <h>`: spawns the harness under nono,
      `setsid`s away from the web process, holds the harness's stdio, appends
      every outbound line to `~/.ajax*/sessions/<h>.log` with byte offsets, and
      serves one Unix socket (`<h>.sock`) that relays inbound lines and replays
      outbound lines from a requested offset. Transport-agnostic: bytes only, so
      ACP, Claude SDK, Pi RPC and Cursor sidecars all work unchanged.
- [x] `TaskSession` connects to the socket instead of pipes; on web startup it
      adopts a live shim (replay from the last committed offset) instead of
      marking the run interrupted. Interrupted applies only when no shim is alive.
- [x] Startup recovery / orphan sweep distinguish "shim alive" from "orphan".
- [ ] Dev-deploy and `ajax web service` restart only the web process; shims keep
      running on the old binary until their session ends.
- [ ] Live check: start a chat turn, restart the dev web server, confirm the
      turn finishes and the browser reconnects with no interrupted note.

#### Phase 2 design decisions (orchestrator, 2026-10-27, from discovery)

Discovery: `.planning/agent-plans/chat-first-core-phase2-discovery.md`.

- **Paths.** The code's per-session dir is `<state_dir>/web-session/`, not
  `sessions/`. Log: `web-session/<encoded>.shim.log`. Socket:
  `web-session/s-<16 hex hash of handle>.sock` (macOS caps socket paths at 104
  bytes; encoded handles can be long).
- **Shim process.** `ajax-cli session-shim --handle <h> --state-dir <d> -- <argv>`
  calls `nix::unistd::setsid()` first (no `pre_exec`/`unsafe`), then execs the
  argv it is given as a child with piped stdin/stdout, stderr to
  `web-session/<encoded>.shim.stderr`, cwd and env inherited. The web keeps
  building the final sandboxed argv/env per transport exactly as today, so the
  shim is transport- and sandbox-agnostic. The harness stays in the shim's
  process group, so `kill -- -<shim pid>` reaches it. Plain threads, no tokio. One socket client at a time. Socket
  protocol: the client's first line is `REPLAY <offset>`; the shim streams
  logged outbound lines from that byte offset, then live lines; every later
  client line goes to harness stdin, except the control line `SHUTDOWN`, which
  makes the shim kill the harness group and exit. Client disconnect never kills
  the harness. Harness exit: the shim keeps the log, closes the socket, exits.
- **Recorded child identity = shim pid** (group/session leader), so the
  existing group-kill orphan sweep still reaches everything.
- **Drop / eviction / replacement semantics unchanged.** Web teardown sends
  `SHUTDOWN`, then group-kills the shim after the existing grace.
- **Adoption.** Web-side clients gain an attached mode that owns no `Child`:
  no kill on drop, liveness from socket EOF. On startup, a run whose shim
  socket answers is adopted (skip handshake, reuse the persisted session id,
  replay from the last committed offset) instead of interrupted and swept;
  interruption and sweep apply only when no shim answers. Per-transport turn
  end on an adopted connection is the delegate's call inside the adoption
  slice, but it must not resend prompts (non-goal: no replay/reconcile).

- **Shim key = harness cwd** (each chat task has its own worktree), so spawn
  sites need no handle plumbing; `ajax-cli web` enables the shim process-wide.
  Without that call (all unit tests) spawning is unchanged.
- **Adoption = read-only replay client, then the existing resume path.**
  Adoption detail: `.planning/agent-plans/chat-first-core-phase2-adoption.md`.
  At prompt dispatch the web persists, on the prompt-ledger entry, the shim
  byte offset consumed so far and the transcript cursor. On adoption the web
  rolls the run's transcript back to that cursor, connects `REPLAY <offset>`,
  and folds raw lines through the existing pure mappers (ACP
  `map_acp_session_update` + a `stopReason` turn-end check; JSONL mappers via a
  `RecordFold` extracted from `RpcSession`). The adopted client never writes a
  prompt; new prompts queue. On turn end or socket EOF it sends `SHUTDOWN` and
  reports host exit, so the existing dead-host respawn resumes the stored
  session (issue_1179 path). An agent-to-client request (permission,
  elicitation) seen during adoption is not rebuilt: the shim is shut down and
  the prompt is interrupted exactly as today. Offset lives in the ledger blob
  (no migration); `prompt_capabilities`/`close_advertised` are not persisted.

Slices (one commit each, ≤ ~4 files): 0 pure move out of `acp/client.rs`;
1 shim byte log; 2 shim socket relay; 3 CLI `session-shim` subcommand;
4 shim process; 5 generic harness stdio; 6 spawn via shim; A1 ACP raw-line
fold; A2 `RecordFold` extraction; A3 consumed-offset counting + persist at
dispatch; A4 adopted replay `SessionClient`; A5 recovery/sweep skip live
shims; A6 acquire installs the adopted client + transcript rollback; A7 docs +
live check.

### Phase 3: one chat status source

- [ ] The live `TaskSession` (via `TaskSessionDirectory`) is the status source
      for chat tasks. Cockpit projection and push read it in-process through a
      pure core projector (`turn state + PR/CI state + worktree state → status`).
- [ ] Chat activity stops writing the registry snapshot. Only lifecycle
      transitions (turn end + PR state) and a last-known status for cold reads
      (no live session) are persisted, as single-row UPSERTs.
- [ ] Remove the deferred activity queue and the 1 s convergence polling; chat
      head and cockpit render the same projection.

### Phase 4: chat verbs

- [ ] Chat tasks show Continue (compose), Stop (cancel turn), Restart agent
      (re-spawn shim + restore session; re-provision worktree if missing),
      Fix CI, Resolve conflicts, Create PR, Merge PR, Drop. Resume / Repair /
      Ship appear only for `TerminalAgent`.
- [x] Create PR, Fix CI, Resolve conflicts are prompts through the chat queue.
- [x] Merge PR runs `gh pr merge` for the associated PR.
- [ ] `PullRequestRef` metadata gains additions/deletions for dashboard rows.

#### Phase 4 design decisions (orchestrator, 2026-10-28, from discovery)

Discovery: `.planning/agent-plans/chat-first-core-phase4-discovery.md`.

- New chat verbs (Create PR, Merge PR, Restart agent) are verb IDs alongside the remediation IDs, not new `OperatorAction` variants, so the pinned label/coverage tests (`models/tests.rs:580`, `ajax-cli lib/tests/suite_10.rs:115`, TUI glyph test) stay untouched.
- Fix CI / Resolve conflicts stay flagged-only (CI failure / conflict). Create PR is offered when the chat task has no associated open PR; Merge PR when it has one.
- Create PR is a prompt through the chat queue (the agent commits, pushes and opens the PR); the product does not call `scripts/gh-pr-create`.
- Merge PR runs `gh pr merge <number> --squash` through the core GitHub adapter/CommandRunner (squash keeps the PR title as the commit subject, matching release-please). No branch deletion; Drop owns cleanup.
- PR diff stats come from a separate `gh pr view <n> --json additions,deletions` call stored under a separate metadata key; `PullRequestRef` and the pinned `pr list` field string are unchanged.
- Restart agent = drop the live session and acquire again (respawn + stored-session resume), chat-only, no tmux. A missing worktree is reported, not re-provisioned (re-provision deferred).
- Ship hidden for chat tasks is BLOCKED: `crates/ajax-core/src/recommended.rs:703-720` asserts a chat task offers Ship; needs Matt.

## Verification

Per phase, at minimum:

```bash
npm run verify:arch
npm run verify:core
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace
pkill -f vite; CI=1 npm run web:smoke    # any phase touching web UI
```

Phase 2 also needs the live restart check above. launchd plist changes must be
applied by Matt (nono blocks `launchctl bootstrap`).

## Revision log

2026-10-27 reassessment, changes from the previous plan:

- **Reordered:** durability (old Phase 5) is now Phase 2, directly after
  defects. It is the user's main pain and does not depend on any refactor.
- **Replaced "agent host" with per-session shims.** One long-lived directory
  process would have to move `TaskSessionDirectory`, protocol v2 and ownership
  cleanup across an IPC boundary. A bytes-only shim per session keeps all of
  that in the web process, is transport-agnostic, and means deploys never
  interrupt running sessions at all.
- **Removed typed `TaskKind` migration and 46-site inversion.** No user-visible
  change; replaced with a typed accessor, converted opportunistically.
- **Removed typed PR table.** `PullRequestRef` metadata already drives
  lifecycle; add diff stats to it.
- **Removed full row-level persistence.** After Phase 3 the chat path no longer
  writes snapshots, which removes most writers. The two real hazards (chat-run
  deletion on save; event/receipt wipe) get narrow fixes: deletion moves to
  Drop (Phase 0); the event wipe is low impact for chat use and is left as a
  known issue.
- **Replaced "derive status from execution tables"** (speculative; first task
  was "find the gaps") with "the live session is the status substrate", which
  matches the existing Git/tmux authority model.
- **Added** the blocking fixes (control lane, attach retry) and the
  visibility signal that decision 3 requires.
- **Dropped** docs-only rewrites of every architecture doc up front; Phase 1 is
  a contract change, detailed docs land with the code that makes them true.

## Deviations

- Phase 0.4: presence signal landed (#1491); push on Response ready is pending Matt's sign-off because `crates/ajax-core/src/attention/tests.rs::response_ready_waiting_does_not_notify` asserts the opposite.
- Phase 0.5: harness switch keeps its control-lane hold (`swap_task_agent` is registry-only); task start checkpoints apply only the new task delta under the lane.
- Phase 0.6: second consecutive revision race still returns `NotOrchestrationChat` (no dedicated error variant).
- Phase 0.3: direct `ajax ship` CLI on a task with an open PR still merges locally; only the offered actions changed.
- Phase 0.7: pending Matt's sign-off because `crates/ajax-core/src/registry/sqlite/tests/execution_drop.rs` simulates CLI drop via snapshot omission.
- Phase 2 slicing: A3 split into A3/A3c (core `commit_execution` now applies prompt content updates before transitions so a Pending prompt can be rewritten in its dispatch commit); A5 split into A5a (core `truncate_events_after`, also lowers `next_event_seq`), A5b (acquire adopts), A5c (quiet release after the adopted turn), A5d (headless startup acquires adoptable handles; queued prompts behind an adopted turn respawn via `acquire`).
- Phase 2: shim `handle_client` tolerates a peer that writes and closes immediately (macOS `set_read_timeout` EINVAL), so write-then-close `SHUTDOWN` clients work.
- Phase 2: adopted clients keep default `prompt_capabilities`/`close_advertised`, skip `finalize_client_metadata`, and (JSONL) send a usage probe; config writes are not gated on adoption.
- Phase 3 (partial, `e8f75612`): the live `TaskSession` publishes the activity it computed; cockpit and task-detail reads overlay it (via the existing `apply_authoritative_observation_at` on a clone) while its registry report is pending or failed. Push still reads the registry. No new core projector: `derive_operator_status` over the overlaid task is the projection. Discovery: `.planning/agent-plans/chat-first-core-phase3-discovery.md`.
- Phase 3 remainder BLOCKED on Matt's sign-off (test contracts): stopping chat activity snapshot writes contradicts `crates/ajax-web/src/runtime/tests/suite_6.rs:79` (`issue_1227_session_activity_fails_when_registry_is_not_saved`); removing the deferred queue contradicts `runtime/tests/suite_5.rs:286` and deletes assertions in `runtime/activity_report_tests.rs` (5 tests) and `session_activity_directory_tests.rs:569`; removing the 1 s convergence deletes `TaskWorkspace.test.tsx:341-395` and changes `headView.test.ts` / `LiveHead.test.tsx:36`. The last-known-status UPSERT (discovery P3.5) is deferred until the snapshot-write removal is approved. Push dedupe keys live in `task.metadata` and need a persistence decision.
- Phase 4: chat tasks now offer Create PR (chat prompt), Merge PR (`gh pr merge --squash`, confirmation required), Restart agent (detach + acquire + release; resumes the stored session), alongside Fix CI / Resolve conflicts and Drop; Continue is the composer and Stop already lived in the chat head (LiveHead), now passing `keepQueue: true` explicitly. Verbs are remediation IDs, not `OperatorAction`s; the frozen TUI drawer hides the three web-only verbs. Merge PR runs under the control lane like Ship/Drop. Restart reports a missing worktree instead of re-provisioning.
- Phase 4 BLOCKED on Matt: hiding Ship for chat tasks contradicts `crates/ajax-core/src/recommended.rs:703-720`; PR diff stats contradict exact gh command-sequence assertions in `crates/ajax-cli/src/lib/tests/suite_2.rs:432` and `suite_3.rs:60,104` (a `tests/` directory). The finished stats patch (github `pr view --json additions,deletions` spec + parser, separate metadata key, ci_monitor fetch on discovery) is saved at `.planning/agent-plans/chat-first-core-p4e-pr-stats.patch`; the dashboard-row rendering (discovery P4.9) was not started.
- Phase 2: `adopted_eof_mid_turn_interrupts` drives the exit via an agent->client request (same path as socket EOF); `adoption_attach_failure_keeps_transcript` uses a `#[cfg(test)]` refusal list.

## Validation results

None yet.
