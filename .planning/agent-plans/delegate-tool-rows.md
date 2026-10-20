# Delegate tool rows in Ajax Chat

## Goal

While a frontier harness (Claude SDK, Pi, Codex, Cursor) runs a model-router
delegate (`scripts/run-delegate` → acpx), each tool call the delegate makes
(Read/Edit/Bash/...) appears **live** in the Ajax chat as an ordinary tool row,
identical in look to the frontier's own tool rows (user direction 2026-10-19:
"delegate's tool rows", flat, like ACP).

## Root cause

- `run-delegate` runs inside one frontier Bash tool call; the harness returns
  its stdout only when the call ends, so nothing streams to the chat.
- `run-delegate` already writes `subagent_status` NDJSON "for Ajax Chat"
  (model-router `1e3cc6d`), but ajax-cli has never read it
  (`git log --all -S subagent_status -- crates` is empty).
- The delegate's full ACP stream (`session/update` `tool_call` /
  `tool_call_update`) only lands in `$AJAX_ROUTER_RUN_DIR/raw.log`.

## Design

1. **Ajax → env.** Every web-session harness spawn sets
   `AJAX_DELEGATE_EVENTS_DIR=<state_dir>/web-session/delegates/<encoded worktree>`.
   Keyed by worktree (the spawners already receive `cwd`), so no spawn signature
   changes: a process-wide `OnceLock` holds the state dir, set once at web-server
   start; unset (tests) → no env. Bash calls and `run-delegate` inherit it.
   Spawn points: `adapters/web_session_acp/jsonl_process.rs` (Claude/Cursor/Pi
   native) and `adapters/web_session_acp/client.rs` (ACP, Codex).
2. **model-router → file.** When `AJAX_DELEGATE_EVENTS_DIR` is set,
   `run_acpx_process` appends each raw ACP stdout line it already writes to
   `raw.log` to `$AJAX_DELEGATE_EVENTS_DIR/<run_id>.jsonl` (`run_id` = `--run-id`
   or a generated uuid). Unset → no behavior change. Write failures never fail
   the delegate.
3. **Ajax → rows.** On each task-session `pump`, tail new complete lines from
   `*.jsonl` in the delegate dir for the session's worktree (per-file byte offsets in memory;
   bounded line length; malformed lines skipped). For `method == "session/update"`
   pass `params` to the existing `map_acp_session_update`, keep only
   `SessionServerEvent::ToolCall`/tool-update events, prefix their ids with
   `<run_id>:` so they cannot collide with frontier tool ids, and append through
   the normal log path. Delegate prose/thought/plan/usage is dropped so it never
   merges into the frontier message or replaces its plan.

## Non-goals

- Nesting rows under the `run-delegate` Bash row (needs a parent field + UI).
- Delegate text/thinking, delegate plans, `subagent_status` rendering.
- Claude SDK background-Agent auto-continuation turns (separate issue).

## Tasks

- [x] A. model-router: env-gated tee to `$AJAX_DELEGATE_EVENTS_DIR/<run_id>.jsonl` + test (set → file has the ACP lines; unset → no file).
- [x] B1. ajax-web: set + create `AJAX_DELEGATE_EVENTS_DIR` on all harness spawns + test.
- [x] B2. ajax-web: delegate-dir tailer → filtered, id-prefixed tool rows on pump + tests (tool_call row appears; update completes it; agent_message_chunk dropped; partial trailing line held until complete; malformed line skipped).
- [x] B3. docs: `docs/architecture/web-session-behavior.md` delegate tool rows bullet.
- [~] C. Live check: model-router half verified live (below); in-app check on a running web server NOT done yet.

## Approval

Approved by the user in chat 2026-10-19 ("Do it").

## Deviations

- 2026-10-19: dir keyed by worktree path instead of session handle — spawners
  take `cwd` but no handle, and threading one through would touch every spawn
  test caller. One worktree hosts one task session, so the key is equivalent.

## Evidence

- Live acpx `raw.log` lines are standard ACP: `{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":…,"update":{"sessionUpdate":"tool_call",…}}}`,
  then `tool_call_update` with `in_progress` / `completed` + `content` — the exact
  `params` shape `slices/web_session/acp_map.rs::map_acp_session_update` consumes.

## Validation

- model-router: `python3 -m pytest tests -q` → 220 passed, 1 skipped.
- ajax-cli: `cargo fmt --all -- --check` ok; `cargo clippy --locked --all-targets --all-features -- -D warnings` ok;
  `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features` ok.
- `cargo test -p ajax-web --lib`: 1277 passed, 19 failed. Clean HEAD (git archive, same machine) fails the same
  18 (host_crash, startup_recovery, orphan_sweep, task_session_child/commit/durability, prompt_capabilities,
  session_close …); each passes alone; not caused by AJAX_PROFILE/TMUX. 19th (`advertised_close_sent_on_slot_shutdown`)
  is a close-test flake. Pre-existing, out of scope.
- Live: real `run-delegate` (pi/Qwen) with `AJAX_DELEGATE_EVENTS_DIR` set → `livecheck.jsonl` received `tool_call`
  lines at t≈21s while the delegate was still running; file byte-identical to raw.log JSON lines.
- Architecture guard caught an adapter→adapter dependency (`encode_handle`); fixed by inlining the encoder.
