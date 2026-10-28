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

- [ ] Remove the duplicate `spawn_push_tick` (`runtime/mod.rs:166`).
- [ ] Fix CI / Resolve conflicts on chat tasks go through the `TaskSession`
      FIFO (the automatic-CI path), not tmux send-keys.
- [ ] Ship is not offered when the task has an open PR.
- [ ] Push on "Response ready" when Ajax web is not open. Browser reports
      `visibilitychange`; server "open" = a visible page within the last poll
      interval (replaces the 90 s TTL).
- [ ] Task start / harness switch run external effects outside `control_lane`;
      take the lane only for the registry apply.
- [ ] Chat attach retries once on revision mismatch instead of returning
      `NotOrchestrationChat`.
- [ ] Chat-run deletion moves out of `save_registry` into Drop only.

### Phase 1: contract

- [ ] `PRODUCT.md`: Chat is the work surface; Terminal is a tab.
- [ ] `architecture.md` + `AGENTS.md`: a "Chat-first" section stating the
      design principle, the frozen status of terminal-agent/TUI/tmux verbs, and
      that the live chat session is the authoritative substrate for turn state
      (same standing as Git and tmux). Detailed doc rewrites land with Phases 2–4.
- [ ] Add `Task::kind() -> TaskKind { Chat, TerminalAgent }` as a typed accessor
      over the existing metadata; no migration. New code uses it; existing call
      sites convert when touched.

### Phase 2: sessions survive web restarts (per-session shim)

The highest-value change, moved ahead of all refactoring.

- [ ] `ajax-cli session-shim --handle <h>`: spawns the harness under nono,
      `setsid`s away from the web process, holds the harness's stdio, appends
      every outbound line to `~/.ajax*/sessions/<h>.log` with byte offsets, and
      serves one Unix socket (`<h>.sock`) that relays inbound lines and replays
      outbound lines from a requested offset. Transport-agnostic: bytes only, so
      ACP, Claude SDK, Pi RPC and Cursor sidecars all work unchanged.
- [ ] `TaskSession` connects to the socket instead of pipes; on web startup it
      adopts a live shim (replay from the last committed offset) instead of
      marking the run interrupted. Interrupted applies only when no shim is alive.
- [ ] Startup recovery / orphan sweep distinguish "shim alive" from "orphan".
- [ ] Dev-deploy and `ajax web service` restart only the web process; shims keep
      running on the old binary until their session ends.
- [ ] Live check: start a chat turn, restart the dev web server, confirm the
      turn finishes and the browser reconnects with no interrupted note.

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
- [ ] Create PR, Fix CI, Resolve conflicts are prompts through the chat queue.
- [ ] Merge PR runs `gh pr merge` for the associated PR.
- [ ] `PullRequestRef` metadata gains additions/deletions for dashboard rows.

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

None yet.

## Validation results

None yet.
