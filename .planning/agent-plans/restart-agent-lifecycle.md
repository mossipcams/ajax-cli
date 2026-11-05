# Plan: restart-agent-lifecycle

## Problem

Ajax Chat's agent-restart lifecycle has no single owner. The `TaskSession` actor
owns the ACP child, but "replace the child" is implemented five times across
three layers with different channels, rules, and status effects:

| Path | Where | Channel | Keeps conversation | Reports activity |
|---|---|---|---|---|
| Attach on dead slot | `task_session_spawn.rs` `acquire` dead-slot branch | attach | yes | no |
| Model change on dead child | `task_session_spawn.rs` `respawn()` via `apply_config_option` | WS | yes | no |
| **Restart agent** | `task_session_restart.rs` `restart_session`, outside the actor | HTTP `/api/operations` | yes | **no** |
| Retry (restore failed) | `ws_bridge.rs` `RetryRestore` → `acquire_typed`, pre-attach only | WS | yes | no |
| Fresh session / `/clear` / Start fresh | actor `ClearContext`, directory `start_fresh` | WS | no | n/a |

Sending to a dead child is refused (`task_session_exit.rs`: "ACP process exited
— reconnect to send prompts"); the browser says "The agent stopped. It will
restart when you reconnect." (`web/src/features/chat/session/errors.ts`) while
the operator is already connected.

Observed defects:

1. `restart_session` detaches the whole actor, re-acquires a synthetic holder
   count, edits the prompt ledger from outside the actor, and never reports
   activity. After a restart that resends nothing, the task keeps
   `LiveStatusKind::Blocked` ("Agent blocked", needs-you) and Core keeps
   offering `restart-agent`, contradicting the "Agent restarted." toast.
2. Core offers `restart-agent` only from `Blocked` (`remediation.rs`
   `offers_restart_agent`), inferring process liveness — host reality Core does
   not own — from a lagging status. After an idle child exit nothing is
   `Blocked`, so Restart is hidden exactly when it is needed.
3. Two Restart buttons with different rules: chat header `ActionBar` runs it
   without confirmation (server `remediation_action_state` sends
   `confirmation_required: false`); task details uses a client-side
   `RESTART_ACTION` with confirmation and also shows it whenever a turn is
   running (`sessionBusy`).

## Design (approved recommendation: lazy revival)

1. **One owner, one primitive.** The `TaskSession` actor owns child lifecycle
   through `replace_child(Resume | Fresh)`, built on the existing `respawn()`
   and the `ClearContext` path. Every entry point is a caller:
   - **Resume** (keep conversation): attach, prompt submit, or config change on a
     dead child; operator Restart (forced); Retry.
   - **Fresh** (new conversation): Fresh session, `/clear`, Start fresh;
     cross-harness switch already does this via `reset_harness_context`.
2. **Lazy revival.** Unexpected child exit keeps today's reconciliation
   (interrupt the active prompt, report `TurnFailed`) and leaves the slot dead.
   The next demand (attach, prompt submit, config change) revives with Resume.
   No eager supervisor restart: avoids crash loops and spawning children for
   idle slots that idle eviction would reclaim. A failed revival surfaces the
   existing typed restore failure (Retry / Start fresh).
   The never-resent rule is unchanged: interrupted prompts are not resubmitted;
   only prompts submitted after the exit trigger revival.
3. **Restart is a session-protocol command.** New WebSocket client message
   `{ "type": "restart" }` → actor command (sibling of `ClearContext`) →
   forced Resume. The actor already owns the ledger, FIFO, and transcript, so
   the in-flight resubmit (`<clientMessageId>-restart`, after queued prompts)
   happens in-actor; subscribers stay attached; the actor appends an
   `Agent restarted.` host note (and `Re-sent your last prompt.` when it
   resubmits), like `Context cleared.`. Pre-attach `retry_restore` routes to the
   same path. Restart leaves `/api/operations`.
4. **One activity report.** On every successful replacement the actor reports:
   `TurnStarted` when it resubmitted or dispatched a prompt; otherwise
   `TurnEnded` when the last reported activity was `TurnFailed`, returning the
   task from `Blocked` to the idle `Done` state every idle chat already uses.
   Core stays the owner of task truth; the host reports evidence.
5. **Core stops offering `restart-agent`.** Delete `offers_restart_agent`,
   the `RESTART_AGENT` remediation constant and its routing in
   `runtime/task_routes/live.rs` (`is_chat_remediation`,
   `restart_chat_agent`, `chat_remediation_response`) and
   `slices/actions.rs` (`supported_browser_action`), `restart_session` /
   `RestartError`, the TUI drawer filter, and the client `RESTART_ACTION`
   constants and filters.
6. **UI: two verbs.** Task details session tools in chat mode always show
   **Restart agent** (keeps conversation, one confirm, sends WS `restart`) and
   **Fresh session** (unchanged). Not shown in the chat header or task list.
   Restore-failure panel keeps Retry / Start fresh labels, mapped to the same
   two commands. Dead-agent copy: "Agent stopped. Send a message to restart it."

## Scope

- `crates/ajax-web/src/slices/web_session/`: actor restart command, revive on
  submit, activity settle, protocol message, removal of `restart_session`.
- `crates/ajax-web/src/runtime/task_routes/live.rs`, `crates/ajax-web/src/slices/actions.rs`:
  remove the HTTP restart route.
- `crates/ajax-core/src/remediation.rs`: remove the restart offer.
- `crates/ajax-tui/src/cockpit_state.rs`: drop `RESTART_AGENT` from the drawer filter test.
- `crates/ajax-web/web/src/`: details-sheet button over WS, header/list/mutation
  cleanup, dead-agent copy.
- `docs/architecture/web-session-behavior.md`: rewrite Restart, child-exit, and
  restore-recovery invariants.

## Non-goals

- Eager/automatic restart after child exit; crash-loop backoff.
- Changing the never-resent rule for interrupted prompts.
- Restart from the task list or native TUI (TUI and tmux verbs are frozen).
- Terminal-agent (tmux) tasks.
- Changing Fresh session, `/clear`, harness switch, or idle eviction semantics.
- New `LiveStatusKind` or Core status vocabulary.

## Constraints

- `task_session_directory.rs` is 994 lines (hard cap 1,000): new directory
  methods go in `task_session_restart.rs` as an `impl TaskSessionDirectory`
  block, not in the directory file. `task_session_exit.rs` (802) gets only the
  revive call site.
- Existing restart tests in `task_session_restart_tests.rs` are live coverage:
  retarget them to the actor command; keep every behavioral assertion
  (stored-session resume, transcript and queue kept, holder/eviction
  behavior, missing worktree error, wedged Claude SDK replacement, resend with
  new id and image blocks, no resend without in-flight prompt).
- Confirmed defect process: one GitHub issue covering defects 1-3; each defect
  gets a regression test that fails on `main`.
- Keep all edits to one file inside one PR.

## Delivery

- **PR 1 — server, additive** (`fix(web): restart agent lifecycle`):
  WS `restart` message + actor command, revive on submit, activity settle,
  retarget restart tests, regression tests, docs rewrite. HTTP route stays
  temporarily so the shipped web keeps working.
- **PR 2 — web switch and deletions** (`fix(web): one restart agent verb`):
  details-sheet Restart over WS, remove header/list/mutation restart handling,
  dead-agent copy, update `smoke.test.ts`; delete HTTP route, Core remediation,
  `restart_session`, TUI filter entry.

## Delegation decision

Delegated via model-router (one packet per PR; PR 1 may split server
command / revive-on-submit into separate packets).

## Task checklist

- [x] Approval to implement
- [x] Open GitHub issue (defects 1-3) on `mossipcams/ajax-cli`; record number here: #1574 (defects 1, 3); defect 2 already tracked as #1557
- [x] PR 1: regression test — restart with no in-flight prompt clears `Blocked` (fails on `main`)
- [x] PR 1: regression test — prompt submitted after child exit revives the child and dispatches (fails on `main`)
- [x] PR 1: regression test — WS `restart` keeps the socket attached and appends `Agent restarted.`
- [x] PR 1: actor restart command + `{ "type": "restart" }` protocol message; `retry_restore` routed through it
- [x] PR 1: revive dead child on prompt submit; remove "reconnect to send prompts"
- [x] PR 1: activity settle on successful replacement
- [x] PR 1: retarget `task_session_restart_tests.rs` to the actor command, assertions unchanged (`restart_session` is now a thin caller of `TaskSessionCommand::Restart`, so existing tests exercise the actor path; call sites move to `restart_agent` when PR 2 deletes `restart_session`)
- [x] PR 1: rewrite `web-session-behavior.md` Restart / child-exit / restore-recovery sections
- [x] PR 1: parent review gate + validation
- [x] PR 2: details-sheet Restart always visible in chat mode, confirm, sends WS `restart`
- [x] PR 2: remove `restart-agent` from `ActionBar`, `TaskList`, `taskMutations`, `TaskDetailsSheet` constants; update their tests
- [x] PR 2: dead-agent copy in `errors.ts` (+ `reducer.test.ts`)
- [x] PR 2: update `e2e/smoke.test.ts` restart case to the WS path
- [x] PR 2: delete HTTP restart route, `restart_session`/`RestartError`, Core `offers_restart_agent` + `RESTART_AGENT`, TUI filter entry; retarget the Core remediation test to assert no restart offer
- [x] PR 2: parent review gate + validation (independent Codex review: 5 findings verified and fixed; see Deviations)
- [ ] Commit, push, open PR (`Fixes #1574`, `Fixes #1557`) — approved 2026-11-05: branch renamed to `fix/web/restart-agent-lifecycle`, one PR

## Approval

Design recommendation (lazy revival) selected by the user 2026-11-05.
Implementation approved 2026-11-05 ("Delegate until finished").

## Deviations

- Defect 2 (Restart hidden after idle exit / dead child on open socket) was already open as #1557; #1574 covers defects 1 and 3 and links #1557. PRs reference both.
- `task_session_directory.rs`: `command_tx`/`send_command` made `pub(super)`; dead `attached_holders` deleted and `detach_session` made `#[cfg(test)]` (994 → 986 lines). `session_activity.rs` gained `last_reported()`.
- In-flight resubmit stays gated on attached holders, as before; with zero holders a resend would keep the child busy and break idle eviction.
- Pre-attach `RetryRestore` stays on `acquire_typed`: it already resumes through the same dead-slot primitive, and routing it through restart would respawn instead of adding the socket's holder.
- `SessionClientMessage::Restart` lives in `web_session/mod.rs` with one arm in `apply_client_message` (`task_session_directory.rs` 990 lines).
- Delivery collapsed to one PR: PR 1 and PR 2 both edit `task_session_restart.rs` and its tests (keep-one-file-in-one-PR rule), and the combined diff is under the 1500-line gate.
- `chat_remediation_response` / `is_chat_remediation` kept for fix-ci/create-pr/resolve routing; only the restart branch was deleted.
- `restart_agent` now rejects a missing worktree (`"worktree missing"`), keeping the old missing-worktree assertion; the WS path inherits the check.
- Independent review fixes: queued prompts held aside during `release_live_client` so none dispatches onto the outgoing child; `client.cancel()` failure no longer aborts teardown; `SessionActivityReporter::reset_turn` on the TurnFailed→TurnEnded settle; withdrawals/edits of queued rows after child exit do not revive; post-attach `retry_restore` → Restart and `start_fresh` → Clear. The lost-queue race and dead-pump cancel have no deterministic regression test (needs a fake-ACP seam for delayed cancel / closed command channel).
- `restart_session`'s evicted-session acquire/restart/release branch was deleted with it; WS restart requires a live entry (the socket holds one).

## Validation

Run before each PR (exact hook/CI commands):

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u RUST_LOG cargo nextest run --workspace
npm run web:check && npm run web:lint && npm run web:sg
npm run web:test -- --run
npm run web:build
pkill -f vite; CI=1 npm run web:smoke
```

Results (2026-11-05, final tree):
- `cargo fmt --check` pass; workspace clippy `-D warnings` pass
- `env -u RUST_LOG cargo nextest run --workspace`: 3481 passed, 1 skipped
- `web:check`, `web:lint`, `web:sg` pass; `web:test`: 1942 passed, 9 skipped
- `web:build` and `web:build:check` pass (dist rebuilt)
- `web:smoke` NOT run: Playwright WebKit cannot run inside the nono sandbox; rely on CI Web E2E
- Size: 1069 changed source lines (gate 1500), no file over 600 changed lines
