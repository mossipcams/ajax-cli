# Handoff: native harnesses through one shared layer + in-session durability

Written 2026-10-13 from task `ajax-cli/durability-v2`
(`/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-durability-v2`,
branch `ajax/durability-v2`). This task: `ajax-cli/native-harness-durability`,
branch `ajax/native-harness-durability`, based on `origin/main` @ `e872f534`.

## Goal (Matt's words)

In-session durability, native Cursor SDK, native Pi RPC, native Claude SDK
through a shared layer for Ajax web, while still supporting Codex ACP for
Ajax web.

Read as: every Ajax Chat harness except Codex runs on a native (non-ACP)
transport, all of them plug into ONE shared native layer
(`AgentEvent::Native(NativeEvent)` + `SessionClient`), Codex keeps ACP, and a
chat session survives an `ajax-web` restart/crash mid-session.

## Read first

1. `AGENTS.md` (delegation through model-router is mandatory; parent does not
   implement), then `architecture.md`.
2. `docs/architecture/web-session-behavior.md` — section "Native Claude SDK
   and Pi RPC events" is the shared-layer contract.
3. `.planning/agent-plans/cursor-native-sdk.md` — this session's plan, with
   findings, deviations, and the per-task checklist. **Authoritative for
   Cursor work.**
4. `.planning/agent-plans/durable-execution-tmux-removal.md` — the durability
   plan (stages 1–12).
5. `.planning/agent-plans/native-claude-pi-events.md`,
   `claude-agent-sdk-pi-rpc.md` — how Claude/Pi went native.

## Where each piece stands

| Piece | State | Evidence |
| --- | --- | --- |
| Claude via Agent SDK sidecar | Shipped on main (#1264) | `acp_launch_for_agent(Claude).transport == ClaudeSdk`; `crates/ajax-web/sidecar/claude_sdk_sidecar.mjs` |
| Pi via native RPC | Shipped on main | `transport == PiRpc`; `pi_rpc_*.rs` |
| Codex via ACP | Stays, by design | `transport == Acp`, `codex-acp` bridge |
| Cursor | **Still ACP on main** (`agent acp`); native in progress, unwired | see "This session's work" |
| Shared native layer | Exists and is guard-tested | `agent_event.rs`, `rpc_client.rs`, `rpc_session.rs`, `rpc_handshake.rs`, `jsonl_process.rs`, `config_model.rs`, `session_client.rs`; guard `native_harness_code_never_names_the_acp_protocol` in `crates/ajax-web/src/architecture_web_session.rs` |
| `/commands` for native harnesses | Done this session, uncommitted | Packet 1 below |
| In-session durability | Partly shipped (#1280 "durable chat execution") | durability plan: stages 1, 2, 4 checked; 3, 5, 6, 7, 8, 12 unchecked; 9–11 PARKED (terminal/tmux) |

Caution: the durability plan's stage 6 (Claude SDK) and 7 (Pi RPC) boxes are
unchecked although both transports are live in the launch table. Verify
against source before trusting either the boxes or this sentence.

## This session's work (uncommitted, NOT in this worktree)

All of it sits uncommitted in the old worktree
(`ajax-cli__worktrees/ajax-durability-v2`). That worktree can be reaped. A
copy is in this worktree as a patch:

```
git apply --check .planning/agent-plans/native-harness-wip.patch   # verified OK 2026-10-13
git apply .planning/agent-plans/native-harness-wip.patch
```

17 files, +1890/−12, generated against `e872f534` (== this branch's base).
It was NOT applied here. **Superseded by the SDK decision below: apply only
`native-commands-packet1.patch`.**

| Packet | What | Status |
| --- | --- | --- |
| 1 | Native `/commands` passthrough: one-shot `take_available_commands()` on `ClaudeSdkClient`/`PiRpcClient`, `AcpEventSource::take_available_commands` (default empty), `acp_drain` seeds `session_available_commands`, ACP update replaces it | ACCEPTED (independent gate + full hunk review) |
| 2 | `cursor_cli_map.rs`: pure Cursor stream-json → `NativeEvent` mapper, 11 tests | ACCEPTED |
| 3a | `cursor_cli_client.rs`: per-turn process client (`begin_prompt`, `poll_event`/`wait_event`, `set_model`, `shutdown`, `turn_argv`) + `tests/fixtures/fake_cursor_cli.js` | ACCEPTED |
| 3b | `cancel()` + process-group kill (`process_group(0)`, `nix::killpg`), `nix = { workspace = true }` added to `ajax-web` | **Gate green, hunk review NOT done.** Delegate timed out before reporting; do the review before accepting |

Gate at handoff (run by the parent, not taken from delegate reports):
`cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D
warnings` clean; `cargo nextest run -p ajax-web` 1063/1064.

Everything Cursor-native is `#![allow(dead_code)]` and unwired. Cursor chat
still runs on ACP, so nothing is broken in between.

## DECIDED 2026-10-13: Cursor goes through `@cursor/sdk`

Matt: "Use the sdk." This settles the earlier conflict (the mid-session
"native cursor global" instruction had been read as the global `agent` CLI,
and Packets 2–3 were built for that). Consequences:

- **Packets 2, 3a, 3b are discarded** (`cursor_cli_map.rs`,
  `cursor_cli_client.rs`, their tests, `fake_cursor_cli.js`, the `nix`
  dependency on `ajax-web`, and the two guard-list / `mod.rs` entries). Do not
  apply `native-harness-wip.patch`; it is kept only as reference.
- **Packet 1 is kept.** Apply only:
  `git apply .planning/agent-plans/native-commands-packet1.patch`
  (8 files, +277/−11, `git apply --check` verified 2026-10-13; gated green on
  its own before Packets 2–3 existed: nextest `-p ajax-web -p ajax-core`
  2094/2095).
- Accepted with the choice: Cursor chat authenticates with `CURSOR_API_KEY`,
  an `apiKey` option, or the SDK's own login store — **not** the `agent` CLI
  login ACP uses today. Node ≥ 22.13. `@cursor/sdk` is public beta (v1.0.37
  inspected). Ajax must never read or store the key: the sidecar passes
  nothing and lets the SDK resolve it from the host environment. Confirm with
  Matt how the key reaches the ajax-web host (launchd plist / docker compose /
  zshrc) before the flip in the last task.

What the SDK gives (read from `dist/esm/*.d.ts`, nothing executed):
`Agent.create({ local: { cwd, settingSources }, model: { id, params } })`,
`Agent.resume(agentId)`, `agent.send(string | { text, images? })` → `Run`
with `stream()`, `cancel()`, `steer?()`; events `SDKAssistantMessage`,
`SDKThinkingMessage`, `SDKToolUseMessage`, `SDKUsageMessage`,
`SDKStatusMessage`, `SDKTaskMessage`, `SDKSystemMessage`;
`Cursor.models.list()`; `SettingSource` = project|user|team|mdm|plugins|all.
No local slash-command list API.

## Remaining work (SDK path)

Mirror the Claude Agent SDK transport; prefer generalizing its plumbing over
copying it (`claude_sdk_client.rs`, `claude_sdk_spawn.rs`,
`sidecar/claude_sdk_sidecar.mjs`, fixture `tests/fixtures/fake_claude_sdk.mjs`).

- [ ] 0. Spike with a real `CURSOR_API_KEY`: event shapes, `agent_id` stable
      across turns, `Agent.resume` round trip, `run.cancel()`, model list
      shape incl. `params`, and whether `send("/cmd")` expands a
      `.cursor/commands/*.md` command. Gates tasks 2–4.
- [ ] 1. Apply Packet 1 patch, re-run the gate, commit when Matt asks.
- [ ] 2. `cursor_sdk_sidecar.mjs` + Rust client on the shared JSONL/RPC core
      (`jsonl_process.rs`, `rpc_session.rs`, `rpc_client.rs`): `init`
      (session id, models, commands), `prompt`, `cancel`, `set_model`; map
      events to `NativeEvent`. Tests against a `fake_cursor_sdk.mjs`.
- [ ] 3. `SessionClient::Cursor` + spawn (module resolution from the global
      npm root only, as Claude does; fail-closed restore via `Agent.resume`;
      Node ≥ 22.13 check without raising Claude's Node 20 floor).
- [ ] 4. Commands: SDK has no list API, so the sidecar scans
      `<worktree>/.cursor/commands/*.md` and `~/.cursor/commands/*.md` and
      returns them in `init`; they flow through Packet 1's
      `take_available_commands()`. Text is sent unchanged. If the spike shows
      the SDK does not expand them, STOP and ask Matt before adding
      Ajax-side expansion.
- [ ] 5. Models/config as `NeutralConfigOption` from `Cursor.models.list()`;
      persisted pins (`cursor-grok-4.6-high`, `model|k=v`) must still decode.
- [ ] 6. Flip `acp_launch_for_agent(Cursor)` to a new transport; delete
      Cursor-ACP-only code + the tests that pin only that mechanism (Matt
      approved those deletions); update `web-session-behavior.md`.
- [ ] 7. Durability: stages 3, 5, 8, 12 of the durability plan (9–11 parked).
      Stage 3 (`HarnessRuntime` trait + `Capabilities`) is where "one shared
      layer" and durability meet — check whether `SessionClient` should
      become that trait rather than adding a second abstraction.

## Verified facts worth not re-deriving

- ACP session ids are NOT native chat ids: 0 overlap between
  `~/.cursor/acp-sessions` (5500) and `~/.cursor/chats/*/*` (590). Stored
  Cursor sessions fail closed to the existing "Retry / Start fresh".
- `agent acp` exists but is hidden from `agent --help`'s command list.
- The supervisor's `crates/ajax-supervisor/src/agent/cursor.rs` parses Cursor
  stream-json for lifecycle only (no tool ids/args/deltas) — reference, not
  reusable for chat.
- Cursor ACP already passes `/commands` through
  (`available_commands_update`); the swap buys transport parity, not commands.

## Unverified (needs an authenticated Cursor)

This environment could not authenticate (`agent models`, `agent -p`, and
`agent acp session/new` all return "Authentication required"). Built against
the documented stream format and a fake CLI only. Still unknown:

- everything in task 0 of the SDK path (no `CURSOR_API_KEY` was available);
- the CLI-path unknowns are moot now that the CLI packets are discarded.

## Known traps

- **Queue-first delivery breaks Pi tests.** Delivering commands as an event
  ahead of the turn stream broke three existing tests
  (`abort_flow_finishes_cancelled`, `pi_variant_*`). Handshake data goes
  through an accessor, never the turn stream.
- **Persistent accessor spams snapshots.** `pump()` drains on every outbound
  poll and `apply_drain_outcome` sets `pending_commands_snapshot` on every
  `Some`. Anything read at drain start must be one-shot.
- **Known edge, unfixed:** `AcpSlot.session_available_commands` is not reset
  when a client is replaced; swapping into a harness with no commands keeps
  the old list.
- **Known test failures, not caused by this work:**
  `spawn_tests::live_cursor_initialize_advertises_load_session` (needs a
  logged-in `agent`; pins the ACP path Task 6 deletes);
  `task_session_reliability_tests::terminal_persist_failure_retries_before_fifo_advances`
  failed once in a combined `-p ajax-web -p ajax-core` run, then passed 5/5
  alone, 2 full runs, and 40/40 under 2× CPU stress — not reproduced, cause
  unproven (it sleeps a fixed 150 ms before asserting a phase).
- **Router state (2026-10-13):** Qwen timed out at 900 s on 3 of 3 packets but
  left substantive work each time — judge by delta + your own gate, not the
  report. Haiku route is dead (`claude-haiku-5-5` rejected by the Claude ACP
  adapter; fix belongs in `ajax-model-router`). GLM completed 3 rounds then
  returned "Authentication required". Cursor route down (keychain locked).
  Codex route untested this session.
- A timed-out round is reported `FAILED` with `CHANGED_FILES: []` even when
  files changed. Always check `git status`.
- RTK rewrites `git diff` output; use `rtk proxy git diff` for raw hunks.
- `ajax-cli` here: binary `~/.cargo/bin/ajax-cli`; live tasks are in
  `--profile stable` (the ambient `AJAX_PROFILE=dev` shows a different,
  stale task list).

## Rules that bit or nearly bit

- Never edit, delete, or weaken an existing test to get green; fix the
  implementation. Test deletions are approved ONLY for tests that pin the
  removed Cursor-ACP mechanism.
- No `git stash/checkout/restore/reset/clean` — in delegate prompts too.
- Rust files: ~600 lines target, 1000 hard cap. `client.rs` (913) and
  `config_options.rs` (816) are close.
- Nothing was committed or pushed this session. Matt has not asked for a
  commit or PR.

## Verification commands

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-web -p ajax-core --no-fail-fast
npm run verify        # PR gate
```
