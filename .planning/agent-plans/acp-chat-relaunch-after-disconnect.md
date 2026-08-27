# Ajax Chat stays reachable after ACP / agent death

**Status:** approved; implementing
**Issue:** [#1092](https://github.com/mossipcams/ajax-cli/issues/1092)
**Scope:** Keep Ajax Chat relaunchable when an ACP-capable task’s agent dies.
Recover `ajax-cli/chat-tool-row-targets` through that path.
**Non-goals:** Fixing PR #1091 CI (Rust Test). Changing dashboard default for
a *live* tmux agent. ACP v2. Replacing Core task truth. Auto-provisioning
every interactive task at start.

## Why

Web Cockpit hides **Ajax chat** and redirects `#/session/<handle>` →
`#/t/<handle>` when `session_capable` is false. That flag is
`skip_interactive_agent && acp_launch_for_agent(selected_agent)`.

Live `ajax-cli/chat-tool-row-targets` (2026-08-27, stable registry):

| Fact | Value |
| --- | --- |
| Agent | Codex (ACP-capable) |
| `skip_interactive_agent` | **missing** |
| `session_model` | missing |
| Web-session transcript | **none** (`~/.local/state/ajax/web-session/` has no file for this handle) |
| tmux | `ajax-ajax-cli-chat-best-practices:task` exists; pane command is **zsh** |
| Agent wrapper snapshot | missing → CI delivery `fresh agent wrapper evidence unavailable` |
| `agent_status` | Blocked |

This task was started as interactive Codex (tmux send-keys), not Ajax Chat.
Codex has exited. The operator is on a raw shell with no way to launch chat.
Architecture still forbids a second agent while Codex is live in tmux; it does
not say a *dead* interactive task must stay chat-inaccessible forever.

Provisioned Cursor/Claude tasks on the same host still have
`skip_interactive_agent=1`. Their ACP child death is a different path (keep
the flag, respawn). This plan covers both: do not strand the operator.

## Decision

Keep dashboard routing: a **live** interactive tmux agent still opens Terminal.

Change reachability:

1. **Show Ajax chat** in Task details when orchestration chat is on and the
   task’s agent has an ACP entry point (Cursor / Codex / Claude / Pi), even if
   `session_capable` is false.
2. **Do not auto-redirect** `#/session/<handle>` to Terminal solely because
   `session_capable` is false when that agent is ACP-capable. Let ChatSurface
   attach.
3. **Host attach** (`prepare_task_session`): if the task is already
   provisioned, unchanged. If it is not, and the agent has an ACP launch, and
   the tmux task pane is **not** running that agent, set
   `skip_interactive_agent` (persist) then attach. If the tmux agent is live,
   keep HTTP 409 `NotOrchestrationChat` and the existing operator copy.
4. **ACP child death on an already-provisioned task** must not clear
   `skip_interactive_agent` or `session_capable` (regression; should already
   hold).

Live-agent check: tmux `#{pane_current_command}` on
`{tmux_session}:{task_window}` compared to the harness process name (same
idea as `ajax-cli` CI wrapper validation). Missing wrapper snapshot or a
shell (`zsh` / `bash` / `fish` / `sh`) means not live. Do not spawn ACP
beside a live `codex` / `agent` / `claude` / `pi` pane.

No new public CLI verb. No raw SQLite edit of the running stable registry
(ajax-web would overwrite it).

## Recovery of `ajax-cli/chat-tool-row-targets`

After the host promote-on-attach lands:

1. Confirm the pane is still not Codex (`tmux display-message -p -t
   ajax-ajax-cli-chat-best-practices:task '#{pane_current_command}'` → `zsh`
   or equivalent).
2. Open the task → Task details → **Ajax chat** (or `#/session/ajax-cli/chat-tool-row-targets`).
3. Host sets `skip_interactive_agent=1`, persists, spawns `codex-acp`.
4. Confirm `session_capable` is true on the next detail payload and the
   composer is usable.

Do not Drop the task. Worktree
`/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-chat-best-practices`
and PR #1091 stay.

## Implementation checklist

- [x] Task 1 — Host: promote-on-attach when ACP-capable and tmux agent not live.
      Persist `skip_interactive_agent`. Refuse while the pane agent is live.
      Test: dead-pane Codex attach succeeds and sets the flag; live-pane Codex
      still 409. Name #1092.
- [x] Task 2 — Projection: `session_capable` stays the provisioned bit (do not
      flip it from a dead pane). Browser shows Ajax chat for ACP-capable
      agents; session URL for those agents is not force-redirected to
      Terminal. Tests on `TaskDetailsSheet` / `shouldRedirectSessionToTerminal`
      / `App.session`.
- [x] Task 3 — Regression: unexpected ACP child exit on a provisioned task
      leaves `skip_interactive_agent` and `session_capable` true (ajax-web
      session + cockpit).
- [x] Task 4 — Docs: `web-session-behavior.md` Launch + Terminal view paths;
      `web-cockpit.md` session_capable paragraph. Interactive + live agent
      still Terminal-first; dead ACP-capable agent can be promoted to Chat.
- [ ] Task 5 — Recover `ajax-cli/chat-tool-row-targets` via Task details →
      Ajax chat. Confirm registry flag + chat attach. Manual check in the
      issue/PR.

## Approval status

Approved in chat 2026-08-27. Implementing.

## Material deviations

- `prepare_task_session_attach` uses revision CAS like
  `persist_task_session_model_on_control_lane`: in-memory apply first, persist
  only after a CAS win when promoted. A revision mismatch returns
  `NotOrchestrationChat` (409); no CAS retry, no reload-and-Ok, no persisting
  the stale clone.

## Validation

- Focused: `cargo nextest -p ajax-web` for `prepare_task_session` /
  cockpit session_capable / promote-on-attach tests; `npm run web:test --
  --run` for Task details + session redirect.
- Broader as needed: `npm run web:check`, `cargo fmt --check`.
- Manual: recover `ajax-cli/chat-tool-row-targets` as Task 5.

## Out of scope follow-up

- PR #1091 Rust Test / CI failure (the dead Codex pane was mid
  `gh-fix-ci`). Separate from #1092.
- CLI/`ajax start` remaining interactive by default (intentional).
