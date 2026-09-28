# ACP (codex) — first turn after a harness switch is cancelled

**Status:** investigation / handoff — root-cause mechanism proven; the exact
trigger of the mid-turn cancel is narrowed to one path but not yet closed.
**Repo:** `mossipcams/ajax-cli`
**Issue:** [#1189](https://github.com/mossipcams/ajax-cli/issues/1189) — `[defect] ACP (codex): first prompt after a harness switch is cancelled mid-turn`
**Task:** `SaySo/sayso-model` (Ajax Web Cockpit, ACP-backed chat, codex harness)
**Date:** 2026-09-27

## The wrong experience

In the Web Cockpit Ajax Chat, after switching the task's harness to **codex**
(via the harness "Switch"), the **first prompt** sent to codex starts
generating and is then **killed mid-turn**. The operator sees the agent begin
answering, then the turn ends with a `cancelled` state and no final answer.
Resending the same prompt (after the model has been applied) works.

Operator report: *"in codex the responses are not being sent back, it's
basically killing the turn."* The operator confirms they did **not** press the
Stop button.

This is a confirmed product defect (incorrect behavior relative to the
expected contract: a sent prompt should complete its turn). Per
`docs/defect-process.md` it needs a GitHub issue on `mossipcams/ajax-cli` and a
regression test once the trigger is closed.

## What is proven (hard evidence)

### 1. The turn is killed by an ACP `session/cancel`, not by the model, codex,
or the operator's Stop button.

Codex rollout
`~/.codex/sessions/2026/09/27/rollout-2026-09-27T11-12-17-01a0e3a3-7cc0-7691-b944-b6c44956267d.jsonl`:

```
16:12:47.740  task_started      turn_id=01a0e3a3-f2cf
16:12:48.424  USER  "Pause the livekit retraining. Start up qwen on the llama.cpp server"
16:12:49.465  token_count       (model actively generating)
16:12:49.469  turn_aborted      reason="interrupted"     ← 1.729 s after start
16:13:12.744  thread_settings_applied   (model applied on the RESend)
16:13:12.748  task_started      (resend works)
```

- `reason: "interrupted"` is codex's record of receiving an ACP
  `session/cancel`. A model refusal or a crash would be a different reason.
- The ACP host (codex-acp) **did not exit** — the same rollout continues after
  the abort. So the child did not die.
- The web-session transcript shows the agent had **already started streaming**
  before the cancel, so the cancel arrived **mid-generation**, not at dispatch.

### 2. It correlates with harness switches.

Web-session transcript
`~/.local/state/ajax/web-session/SaySo%2Fsayso-model.jsonl` (2001 events):

| line | event |
|------|-------|
| 620  | `message` "Client switched harness. Context reset." |
| 798  | `turn_end` `cancelled`  ← first prompt after the switch |
| 1690 | `turn_end` `cancelled`  ← **no** switch before it (separate issue) |
| 1821 | `message` "Client switched harness. Context reset." |
| 1828 | `turn_end` `cancelled`  ← first prompt after the switch |

There are exactly **3 cancelled turns** and **2 harness-switch notes**; both
switch-following cancellations are the **first prompt after the switch**. The
third (line 1690) has no switch before it and is a distinct, smaller issue.

### 3. A harness switch tears down and rebuilds the live ACP child.

`crates/ajax-web/src/slices/web_session/task_session_spawn.rs:202`
(`reset_harness_context`) runs, in order:

```
release_live_client(state, true)                          // cancel in-flight, kill old codex-acp
apply_cancel_to_queue(...)                                // drop queued prompts
clear_acp_session_id(...)                                 // new context
spawn_acp(agent, worktree_path, model, None)              // fresh codex-acp, no resume
append harness_switch_note                                // "Client switched harness. Context reset."
```

So after a switch there is a **brand-new** codex-acp process with a
**brand-new** ACP session (confirmed by the rollout: new `session_meta`, no
resume). The new prompt is then dispatched to that fresh child and starts
generating — then is cancelled 1.7 s later.

### 4. The cancel is not the browser's `stop_and_send`.

`stop_and_send` (`crates/ajax-web/web/src/features/chat/composer/submit.ts:50`,
`useComposer.tsx:383`) only fires when a follow-up is **already queued**
(`queued !== null`) **and** the composer is `busy`. In the affected turns the
prompt was sent directly (`prompt_accepted` in the transcript, not queued), so
`stop_and_send` did not fire. The operator also did not press Stop.

## Root cause (mechanism)

In this codebase only two things send an ACP `session/cancel` to a live child:

1. the browser's `Cancel` command (Stop button / `stop_and_send`) — **eliminated**
   above;
2. the server's `interrupt_active_prompt`
   (`crates/ajax-web/src/slices/web_session/task_session_exit.rs:186`), fired by
   the session command loop when the **WebSocket holder count drops to zero
   while a turn is in flight** (a client disconnect).

With (1) eliminated, the cancel came from **(2)**: the server treated "no
attached client" as "cancel the in-flight prompt." That is correct for a real
tab close, but it kills the turn if the socket blips during the switch window.

**Proven:** the server cancels the first post-switch codex turn via the
disconnect/`interrupt_active_prompt` path — not the operator, not the model,
not codex.

**Not yet proven (the one open link):** *why* the holder count hit zero in that
window. The web logs record only `operate` actions, not per-frame WebSocket
traffic, so the disconnect is not visible in the logs. Two candidate triggers
remain and cannot be distinguished from the artifacts alone:

- a browser **re-attach/reconnect race** around the harness reset, or
- a **stale holder count** carried out of the `release_live_client` teardown in
  the switch path.

## Why the artifacts cannot close the last link

- `~/.local/state/ajax/logs/ajax.log` and `web-stable.log` log `operate`
  actions (resume/start/etc.) but not WebSocket frame traffic, so the
  disconnect is not recorded.
- The web-session transcript has **no per-event timestamps**, so the cancel
  cannot be time-correlated against a reconnect in the transcript itself.
- Distinguishing "reconnect race" from "stale holder count" requires either
  capturing the WebSocket frames or a log line on the
  `interrupt_active_prompt` / `TaskSessionCommand::Cancel` path.

## Next steps to close the last link (then fix)

1. **Instrument (code change → route through model-router):** add
   `tracing::warn!` on `TaskSessionCommand::Cancel`
   (`task_session.rs:413`) and `interrupt_active_prompt`
   (`task_session_exit.rs:186`) with the holder count and generation. Rebuild,
   repro a harness switch → first prompt, and read the log to name the exact
   sender.
2. **Or repro with frames captured:** drive a harness switch → first prompt with
   the browser WebSocket console open (or `mitmproxy`/`tcpdump` on the session
   port) and watch for a `cancel` frame.
3. **Fix (after the trigger is named):** make the switch path not drop the
   holder / not fire `interrupt_active_prompt` for an in-flight turn that the
   operator just sent, and/or re-sync the browser `turn.busy` after a harness
   reset. Add a regression test: a prompt sent immediately after a harness
   switch must not be cancelled.
4. **Track:** GitHub issue [#1189](https://github.com/mossipcams/ajax-cli/issues/1189)
   is open; the fix PR must reference it with `Fixes #1189` / `Closes #1189`.

## Scope / non-goals

- **Scope:** the spurious mid-turn cancel of the first post-switch codex turn in
  ACP-backed Web Cockpit chat.
- **Non-goals:** the third cancelled turn (line 1690, no preceding switch) —
  separate issue; ACP v2; the harness "starts on Auto" model-selection behavior
  (by design, documented on the Switch sheet); terminal/task lifecycle.

## Artifacts

- `~/.local/state/ajax/web-session/SaySo%2Fsayso-model.jsonl` — chat transcript
- `~/.local/state/ajax/web-session/SaySo%2Fsayso-model.prompt-ledger.json` — prompt ledger
- `~/.codex/sessions/2026/09/27/rollout-2026-09-27T11-12-17-01a0e3a3-7cc0-7691-b944-b6c44956267d.jsonl` — codex rollout
- `~/.local/state/ajax/logs/ajax.log`, `web-stable.log` — web logs (no WS frames)
