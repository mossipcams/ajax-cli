# Fix: missing task-window scrollback spam

## Scope

Stop Web Cockpit from filling xterm scrollback with repeated `can't find window: task` (and similar) when the shared session exists but the `task` window does not.

## Non-goals

- Auto-repair / recreate the task window from the bridge
- Client-side consecutive-line dedupe
- Changing healthy-session delay-0 reconnect after a stable open
- ED2 / `scrollOnEraseInDisplay` latch (already fixed)

## Root cause

1. Bridge setup succeeds (`new-session -t <shared>`).
2. Bridge spawns `tmux attach-session -t ephemeral:task` into the PTY.
3. Missing window → tmux prints the error into PTY → streamed as binary output.
4. Client treats WS open as success, resets backoff, then uses delay-0 reconnect → infinite spam.

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Bridge: after setup, probe `{ephemeral}:{task_window}` before PTY attach; on failure `send_error_and_close` (no PTY spawn)
- [x] Client: do not delay-0 reconnect when the prior open died before a short stable window; latch unavailable after repeated unstable closes
- [x] Focused tests for probe command + reconnect unstable behavior
- [x] Parent review + validation

## Validation

```bash
cargo test -p ajax-web --lib -- terminal_pty   # 35 passed (parent)
cd crates/ajax-web/web && npx vitest run src/shared/lib/terminalConnection.test.ts  # 31 passed (parent)
```

## Deviations

- Round 1 `pi-delegate` / `opencode-go/glm-5.2` returned empty diff: OpenCode Go monthly usage limit (429). Escalated to `codex-delegate` / `gpt-5.6-sol`.
- Codex implemented correctly; structured report extract failed schema in runner but raw `ROUTER_REPORT` was COMPLETE. Parent accepted after re-running verification.
- Parent polish: empty probe stderr fallback; `second consecutive auto-reconnect` test advances past `STABLE_OPEN_MS`.

## Delegation decision

`Delegation decision: delegated via model-router` (round 2: codex after GLM unavailable)

