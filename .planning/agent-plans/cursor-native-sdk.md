# Cursor: ACP → native (`@cursor/sdk` sidecar) + shared /commands passthrough

## Status

APPROVED 2026-10-13 by user ("Approved"); transport re-confirmed 2026-10-13
("Use the sdk"). Implementing via model-router, packet by packet.
Branch: `ajax/native-harness-durability` (base `origin/main` @ `e872f534`).
Handoff: `.planning/agent-plans/native-harness-handoff.md`.

## Request

"Swap cursor from ACP to native. Native should be in a shared layer and native
should allow pass through of /commands to Ajax web." Wider goal (handoff):
in-session durability, native Cursor SDK, native Pi RPC, native Claude SDK
through one shared layer, Codex stays on ACP.

## Findings (evidence, not assumptions)

- "Native" in this repo = a harness that emits `AgentEvent::Native(NativeEvent)`
  and never names ACP types (Claude Agent SDK sidecar, Pi RPC). Cursor is the
  only remaining ACP-over-stdio harness besides Codex
  (`acp_launch_for_agent(Cursor)` → `HarnessTransport::Acp`, spawns `agent acp`).
- The shared native layer already exists and is guard-tested
  (`architecture_web_session.rs::native_harness_code_never_names_the_acp_protocol`):
  `agent_event.rs`, `rpc_client.rs`, `rpc_session.rs`, `rpc_handshake.rs`,
  `jsonl_process.rs`, `config_model.rs`, plus `SessionClient` as the single
  dispatch point. Cursor-native plugs into this, not a second layer.
- The wire/browser side needs no change: snapshot `availableCommands`
  (`name`, `description`, `inputHint`), composer completion, and "send text
  unchanged on prompt" already exist and are contract
  (`web-session-behavior.md` Launch section).
- `@cursor/sdk` (TypeScript, public beta, v1.0.37 inspected via `npm pack`,
  `dist/esm/*.d.ts` read, nothing executed; Node ≥ 22.13):
  `Agent.create({ local: { cwd, settingSources }, model: { id, params } })`,
  `Agent.resume(agentId)`, `agent.send(string | { text, images? })` → `Run`
  with `stream()`, `cancel()`, `steer?()`; events `SDKAssistantMessage`,
  `SDKThinkingMessage`, `SDKToolUseMessage`, `SDKUsageMessage`,
  `SDKStatusMessage`, `SDKTaskMessage`, `SDKSystemMessage`;
  `Cursor.models.list()`; `SettingSource` = project|user|team|mdm|plugins|all.
- **Auth is not the CLI login.** Resolution is `apiKey` option →
  `CURSOR_API_KEY` env → the SDK's own credential store
  (`Cursor.auth.login()`, `FileCredentialStore`). ACP rode `agent`'s own
  login/keychain.
- **No local slash-command list API** in the SDK types (`GetRepoSlashCommands`
  is a cloud RPC). The sidecar must scan `.cursor/commands` itself to *list*;
  whether `send("/x")` expands locally is unknown until the spike.
- ACP session ids are not SDK agent ids → stored Cursor sessions fail closed
  to the existing "Retry / Start fresh".
- Cursor ACP already passes `/commands` through; the swap buys transport
  parity (thinking, usage, images, long-lived host), not commands.
- This host (2026-10-13): Node v22.23.1 (meets the floor);
  `@anthropic-ai/claude-agent-sdk` is in the global npm root, `@cursor/sdk`
  is NOT; `CURSOR_API_KEY` is not in the environment.

## Decision

Transport: **`@cursor/sdk` Node sidecar**, mirroring the Claude Agent SDK
transport. Accepted with it (Matt, 2026-10-13): Cursor chat authenticates
through `CURSOR_API_KEY` / the SDK's login store instead of the `agent` CLI
login; Node ≥ 22.13 for Cursor chat; public-beta dependency.

**Ajax never reads, logs, or stores the key.** The sidecar passes no `apiKey`
and lets the SDK resolve it from the host environment.

Skipped on purpose (YAGNI): renaming `web_session_acp/` → `web_session_native/`
(≈16k lines of churn, no behavior).

## Scope

1. Shared: surface native `commands` to `availableCommands` (Packet 1).
2. Cursor SDK sidecar + Rust client on the existing JSONL/RPC core.
3. `SessionClient::Cursor`, spawn, fail-closed restore.
4. Commands listing; models/config as neutral options.
5. Flip the launch table; delete Cursor-ACP-only code and docs.
6. Durability stages 3, 5, 8, 12 (`durable-execution-tmux-removal.md`).

## Non-goals

- Codex stays on ACP; ACP stack, `acp_map`, elicitation/permission plumbing stay.
- No browser/UI change; no wire-protocol change.
- Terminal-mode Cursor (tmux, `agent_hooks.rs`, `__agent-event`) untouched.
- No new command semantics: Ajax advertises + sends text unchanged; it does not
  parse or expand commands itself unless Matt signs off after the spike.
- The global-CLI transport (`cursor_cli_map.rs`, `cursor_cli_client.rs`,
  `fake_cursor_cli.js`, `nix` on `ajax-web`), built last session: DISCARDED.
  `native-harness-wip.patch` is reference only; never apply it.

## Tasks

### 0. Spike (needs a real `CURSOR_API_KEY`; gates 2–5) — [ ]

- [ ] Local run from a Node script: event shapes, `agentId` stable across
      turns, `Agent.resume` round trip, `run.cancel()`.
- [ ] `agent.send("/<custom command>")` with `.cursor/commands/x.md` present
      and `settingSources` including `project` + `user`: does Cursor expand it?
  - Outcome A (expands): sidecar only *lists* commands, text sent unchanged.
  - Outcome B (does not): STOP, ask Matt before adding Ajax-side expansion.
- [ ] `Cursor.models.list()` shape incl. `params`; image input; where run logs
      land (affects the `ajax cost` log scan).
- [x] Record results here before starting Task 2.
- STATUS 2026-10-13: RUN with Matt's key (env only, never written to disk),
  `@cursor/sdk` 1.0.37 in a scratch dir outside the repo, Node 22.23.3, model
  `default` (Auto). Still untested: image input. `@cursor/sdk` is still NOT in
  the global npm root.

Spike results (observed, 2026-10-13):

- **Events.** One run streams `status` RUNNING → … → `status`
  FINISHED|ERROR|CANCELLED. `thinking` and `assistant` arrive as **deltas**
  (one small text block per event, concatenate); a `thinking` with
  `thinking_duration_ms` and empty text closes a thought. `tool_call` arrives
  twice per call (`status: running`, then `completed` with `result`), keyed by
  `call_id` — which **contains a newline** (`call-…-0\nfc_…`). One `usage` per
  turn (`inputTokens`, `outputTokens`, `cacheReadTokens`, `cacheWriteTokens`,
  `totalTokens`). No `system`/init event was seen. Every event carries
  `agent_id` + `run_id`.
- **Errors are not exceptions.** Quota and provider refusals come back as
  `status: ERROR` with `message`, and `run.wait()` → `{status:"error",
  error:{message}}`; nothing throws. Seen: "You're out of usage. Switch to
  Auto…" (composer-2.5 on this account) and "Request blocked … usage
  guidelines" (twice, on the first send after a resume; a later resume whose
  first send was trivial worked — cause unproven).
- **Ids.** `agentId` = `agent-<uuid>`, stable across turns and across
  `Agent.resume` in a new process. `run.id` = `run-<uuid>`, new per send.
- **Resume.** `Agent.resume(id, { local })` works and keeps context (recalled
  a file read in turn 1 from a fresh process). **It does not restore the
  model**: `agent.model` is `undefined` and `send` throws "Local SDK agents
  require an explicit `model`". Ajax must pass the persisted model on resume.
  Unknown id → throws `agent_not_found` at `resume()` (fail-closed is free).
- **Cancel.** `run.cancel()` resolved in ~9 ms; stream ends with `status:
  CANCELLED`, `wait()` → `cancelled`; the next send on the same agent worked.
  `run.steer` is a function on local runs.
- **Slash commands: Outcome B.** `send("/spikecmd")` is NOT expanded by the
  SDK. The model received the literal text and went looking (glob + grep, 10
  tool calls, 154k tokens) until it found `.cursor/commands/spikecmd.md` and
  followed it. It "worked" by accident and expensively.
  **DECIDED 2026-10-13 (Matt, option 1): Ajax-side expansion.** The sidecar
  replaces a leading `/name` with that command file's text before
  `agent.send`; plain text substitution only (project file wins over user
  file; no argument templating until asked for).
- **Models.** `Cursor.models.list()` → array of 46: `{ id, displayName,
  aliases?, parameters?: [{ id, displayName, values:[{value, displayName?}] }],
  variants: [{ params:[{id,value}], displayName, isDefault? }] }`. e.g.
  `grok-4.6` has `effort` (low|medium|high|xhigh) × `fast` (false|true);
  `composer-2.5`'s default variant is `fast=true`. `default` = Auto, no params.
- **On-disk state.** Transcripts land in
  `~/.cursor/projects/<cwd-slug>/agent-transcripts/<agentId>/` (relevant to
  the `ajax cost` scan).
- **Noise.** With `settingSources` including `user`, the SDK loaded 65 user
  skills and logged `[hooks]`/INFO lines; where those go (stdout vs stderr)
  was not checked — the sidecar must keep its JSONL channel clean.

### 1. Shared commands passthrough — [x] APPLIED 2026-10-13

Applied `native-commands-packet1.patch` (8 files, +277/−11) on this branch;
uncommitted (Matt has not asked for a commit).

- [x] One-shot `take_available_commands()` on `ClaudeSdkClient`/`PiRpcClient`;
      `AcpEventSource::take_available_commands` (default empty);
      `SessionClient` converts `NativeCommand` → `AvailableCommandDescriptor`;
      `acp_drain` seeds `session_available_commands` before the poll loop and
      ACP `AvailableCommandsUpdated` replaces it.
- [x] Tests: Claude fake sidecar `supportedCommands`; Pi `get_commands`;
      degraded handshake → empty; second drain yields `None`.
- [x] Gate re-run on this branch: see Validation log.

**Deviation (kept from last session):** delivery is a typed one-shot accessor,
not a `NativeEvent::AvailableCommands` variant — queueing it ahead of the turn
stream broke three Pi tests (`abort_flow_finishes_cancelled`, `pi_variant_*`);
a persistent accessor re-emitted the snapshot on every `pump()`.

**Known edge (not fixed):** `AcpSlot.session_available_commands` is not reset
when a client is replaced. Fix with Task 3's spawn-report work.

### 2. Sidecar + Rust client — [ ]

- [ ] `crates/ajax-web/sidecar/cursor_sdk_sidecar.mjs`: `init` (agent id,
      models, commands), `prompt`, `cancel`, `set_model`; no `apiKey` passed.
- [ ] Rust client on `jsonl_process.rs` / `rpc_session.rs` / `rpc_client.rs`;
      mapper to `NativeEvent` (thinking, tool use, usage, assistant text,
      turn finished). Prefer generalizing `claude_sdk_client.rs` plumbing over
      copying it. No ACP type names (guard test).
- [ ] Tests against `tests/fixtures/fake_cursor_sdk.mjs`.

### 3. `SessionClient::Cursor` + spawn — [ ]

- [ ] Module resolution from the global npm root only (as
      `claude_sdk_spawn.rs` does).
- [ ] Node ≥ 22.13 check for Cursor only; Claude's Node 20 floor unchanged.
- [ ] Fail-closed restore via `Agent.resume` (a failed resume is an error,
      never a silent fresh agent).
- [ ] Missing key / missing module → a clear spawn error naming the fix.
- [ ] Reset `session_available_commands` on client replacement (known edge).

### 4. Commands — [ ]

- [ ] Sidecar scans `<worktree>/.cursor/commands/*.md` and
      `~/.cursor/commands/*.md` (name = stem, description = first line),
      returns them in `init`; they flow through `take_available_commands()`.

### 5. Models / config — [ ]

- [ ] `NeutralConfigOption` from `Cursor.models.list()`; persisted pins
      (`cursor-grok-4.6-high`, `model|k=v`) must still decode.

### 6. Flip + delete — [ ]

- [ ] `HarnessTransport::CursorSdk`; Cursor launch row off `agent acp`.
- [ ] Before the flip: confirm with Matt how the key reaches the ajax-web
      host (launchd plist / docker compose / zshrc).
- [ ] Delete Cursor-ACP-only code (`parameterizedModelPicker` handling,
      `read_cursor_acp_model_labels`, `CURSOR_DEFAULT_SPAWN_MODEL` spawn
      pinning, ACP HTTP/2 notes) and the tests pinning only that mechanism
      (Matt approved those deletions).
- [ ] Docs: `web-session-behavior.md` Launch/Native sections.

### 7. Live verification (authenticated) — [ ]

- [ ] Real turn, resume by agent id, cancel, `/cmd`, model switch, image.

### 8. Durability — [ ]

Stages 3, 5, 8, 12 of `durable-execution-tmux-removal.md` (9–11 parked).
First check its stage 6/7 boxes against source (both transports are live),
and whether `SessionClient` should carry `Capabilities` rather than adding a
second `HarnessRuntime` abstraction.

## Behavior changes accepted (stop-condition review)

| Lost / changed | Why |
| --- | --- |
| Cursor chat auth = `CURSOR_API_KEY` / SDK login, not the `agent` CLI login | SDK credential resolution |
| Cursor ACP form elicitation | SDK has no elicitation channel (Claude path already declines) |
| Stored Cursor ACP session ids | not SDK agent ids → fail-closed → "Retry / Start fresh" |
| Node ≥ 22.13 for Cursor chat | `@cursor/sdk` requirement; terminal mode unaffected |
| `@cursor/sdk` is public beta | event `args`/`result` schemas documented as unstable |

## Delegation

Per AGENTS.md: model-router → acpx `scripts/run-delegate`; Qwen in small
packets (one module per packet, inline signatures, "do not explore"); every
prompt bans `git stash/checkout/restore/reset/clean`. Frontier agent reviews
each delta and re-runs the gate itself.

## Validation

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run -p ajax-web -p ajax-core --no-fail-fast`
- rustdoc `-D warnings` per `ci.yml`; `npm run verify` (PR gate)
- Live: authenticated Cursor task → chat turn, `/command`, cancel,
  restart + resume.

### Validation log

- 2026-10-13, Packet 1 on `ajax/native-harness-durability`: `cargo fmt
  --check` clean; `cargo clippy --all-targets --all-features -- -D warnings`
  clean; `cargo nextest run -p ajax-web -p ajax-core --no-fail-fast`
  2094/2095. Sole failure:
  `web_session_acp::spawn_tests::live_cursor_initialize_advertises_load_session`
  (spawns the real `agent acp`, needs a logged-in Cursor; pre-existing, pins
  the ACP path Task 6 deletes). Run by the orchestrator, not a delegate.
  Not run: rustdoc, `npm run verify`.
- 2026-10-13, durability source check: `HarnessTransport` = `Acp | PiRpc |
  ClaudeSdk` and `SessionClient` = `Acp | Pi | Claude`, so durability stages
  6 and 7 are done in source (boxes stale). No `HarnessRuntime` or
  `Capabilities` type exists. `resolve_recovery` is called only from tests;
  the host still applies the fixed restart rule (stage 5 remainder).

## Risks

- Task 0 outcome B grows scope (Ajax-side command expansion).
- SDK beta churn; pin a version in the install hint.
- Model catalog/pin decoding regressions for existing Cursor tasks.
- Rust file limit: `client.rs` (913) and `config_options.rs` (816) are near
  1,000 — no new feature lands in them.

## Approval

- [x] Transport = `@cursor/sdk` sidecar (re-confirmed 2026-10-13)
- [x] Auth change (`CURSOR_API_KEY` / SDK login) accepted
- [x] Behavior-change table accepted
- [x] Test deletions scoped to removed ACP mechanism approved
- [x] Shared-layer interpretation (no directory rename) accepted
