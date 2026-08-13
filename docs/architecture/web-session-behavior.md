# Web Session Behavior Contract

Falsifiable invariants for the optional flag-gated Cursor ACP orchestration chat
mode in Web Cockpit. Later PRs implement against this ledger; nothing here is
implementation how-to.

## Flag-off parity

When the Ajax web session preference is **off**, dashboard navigation, task
detail, embedded raw terminal, Diff Review, and operate flows behave exactly as
they do today. No session routes, WebSocket, or UI chrome may appear or alter
existing paths.

## Launch

- Provisioned Cursor starts skip tmux send-keys but still create the task tmux
  session. Non-Cursor agents cannot use that launch mode.

## Queue and cancellation across WebSocket reconnect

- At most one `session/prompt` is in flight on the ACP host at a time.
- Additional composer submits while a turn is in flight are queued in FIFO order
  (cap 8; oldest dropped when full).
- Cancel with `keepQueue: false` clears the queue and cancels the in-flight turn.
- Cancel with `keepQueue: true` cancels the in-flight turn but preserves queued
  prompts for the next flush.
- After a WebSocket drop and reconnect, the host replays the durable transcript
  from cursor; queued prompts and in-flight state remain host-owned — reconnect
  must not duplicate or lose queued work that survived on the host.

## Model switching across ACP process replacement

- `set_model` while idle respawns the ACP child with the new `--model` pin.
- The UI transcript on disk and in replay is unchanged except for host-emitted
  status/note events.
- A live `ready` event on an established socket must not reset browser reducer
  state; only reconnect-after-drop may clear and replay.

## Restart and transcript recovery

- UI transcript survives `ajax-web` restart via JSONL under `state_dir`.
- On acquire after restart, when Cursor advertises `loadSession`, the host calls
  `session/load` with the stored ACP session id.
- Cursor may emit `session/update` replay notifications before the load result;
  the host drains them so JSONL is not duplicated.
- If load is unsupported or fails, the JSONL transcript still reloads and exactly
  one agent-visible note states that model context reset; the composer keeps
  working.

## Permission persistence

- Operator answers to ACP permission requests are recorded as
  `permission_resolved` in the host transcript.
- Reconnect or full page reload replay must not resurrect a permission prompt
  whose `requestId` already has a matching `permission_resolved` entry.

## Duplicate process and prompt prevention

- At most one ACP stdio process may hold a given task session slot; acquire must
  not spawn a second child for the same handle while one is live.
- A second composer submit while a turn is in flight is queued (not a second
  in-flight `session/prompt`). Reconnect and double-Enter must not start a
  parallel prompt against the same slot.

## Terminal and Diff Review escape paths

- Opening the raw tmux terminal from a session is an overlay/sheet escape hatch;
  it attaches to the task tmux session, not the ACP chat process.
- Navigating to `#/t/<handle>` is terminal-first host attachment, not ACP chat
  continuation.
- Diff Review opened from a session returns to `#/session/<handle>` (chat-first),
  not the terminal-first task page.
- Session horizontal gestures must not steal Diff Review panes and vice versa.
