# Cursor: ACP → native (Cursor SDK sidecar) + shared /commands passthrough

> **SUPERSEDED IN PART 2026-10-13:** Matt decided "Use the sdk". The "REVISION
> (user direction): native = the GLOBAL `agent` CLI" section and its Tasks 2–7
> below are void; Packets 2–3 built for it were discarded and never merged
> (parked outside the repo). Task 1 (shared commands passthrough) shipped in
> its own PR; the SDK findings (Task 0b) stand. Remaining Cursor-SDK work
> continues in task `ajax-cli/native-harness-durability`, tracked in
> `.planning/agent-plans/native-harness-handoff.md` on that branch.

## Status

APPROVED 2026-10-13 by user ("Approved"). Implementing via model-router, packet by packet.
Branch: `ajax/durability-v2`.

## Request

"Swap cursor from ACP to native. Native should be in a shared layer and native
should allow pass through of /commands to Ajax web."

## Findings (evidence, not assumptions)

- "Native" in this repo = a harness that emits `AgentEvent::Native(NativeEvent)`
  and never names ACP types (Claude Agent SDK sidecar, Pi RPC). Cursor is the
  only remaining ACP-over-stdio harness besides Codex
  (`acp_launch_for_agent(Cursor)` → `HarnessTransport::Acp`, spawns `agent acp`).
- The shared native layer already exists and is guard-tested
  (`architecture_web_session.rs::native_harness_code_never_names_the_acp_protocol`):
  `agent_event.rs`, `rpc_client.rs`, `rpc_session.rs`, `rpc_handshake.rs`,
  `jsonl_process.rs`, `config_model.rs`, plus `SessionClient` as the single
  dispatch point. Cursor-native should plug into this, not add a second layer.
- **/commands gap is real and is in the shared layer, not Cursor-specific.**
  Claude's sidecar already calls `query.supportedCommands()`
  (`claude_sdk_sidecar.mjs:227`) and Pi runs `get_commands`; both parse into
  `RpcHandshake.commands: Vec<RpcCommand>` and **nothing reads that field**.
  The slice only fills `availableCommands` from
  `AcpClientEvent::AvailableCommandsUpdated` (`acp_drain.rs:113`). So today no
  native harness can surface slash commands to Ajax web.
- The wire/browser side needs no change: snapshot `availableCommands`
  (`name`, `description`, `inputHint`), composer completion, and "send text
  unchanged on prompt" already exist and are contract
  (`web-session-behavior.md` Launch section).
- Cursor has a first-party SDK, `@cursor/sdk` (TypeScript, public beta,
  Node ≥ 22.13): `Agent.create({ local: { cwd }, model: { id, params } })`,
  `agent.send()` → `run.stream()` with `thinking` / `tool_call` / `usage` /
  `assistant` events, `run.cancel()`, `run.steer()`, `Agent.resume(agentId)`,
  `Cursor.models.list()`. This is the direct analogue of the Claude Agent SDK
  sidecar and gives parity (thinking, usage, cancel) that
  `agent -p --output-format stream-json` does not (thinking suppressed, one
  process per turn, no command list).
- **Unknown, must be spiked:** the SDK docs do not document slash commands,
  `.cursor/commands`, or a command-listing API; headless CLI docs describe slash
  commands as interactive-only. The sandbox that produced this plan was not
  authenticated to Cursor, so no live stream was captured.
  Sources: cursor.com/docs/sdk/typescript, cursor.com/docs/cli/reference/output-format.

## Decision

Transport (REVISED, user direction): the **global native `agent` CLI**
(per-turn `-p --output-format stream-json`), not the `@cursor/sdk` sidecar. The "shared layer" is the existing neutral layer above;
the new work there is **commands passthrough for all native harnesses**
(Claude, Pi, Cursor get it together).

Skipped on purpose (YAGNI): renaming `web_session_acp/` → `web_session_native/`
(≈16k lines of churn, no behavior). Do as a separate mechanical PR if wanted.

## Scope

1. Shared: surface native `commands` to `availableCommands`.
2. Core: Cursor launch table → new `HarnessTransport::CursorSdk`.
3. Sidecar + Rust client for Cursor over the existing JSONL/RPC core.
4. Model/config options via neutral options + catalog.
5. Delete Cursor-ACP-only code and docs.

## Non-goals

- Codex stays on ACP; ACP stack, `acp_map`, elicitation/permission plumbing stay.
- No browser/UI change; no wire-protocol change.
- Terminal-mode Cursor (tmux, `agent_hooks.rs`, `__agent-event`) untouched.
- No new command semantics: Ajax advertises + sends text unchanged; it does not
  parse or expand commands itself unless the spike forces it (Task 0 outcome B).

## Tasks

### 0. Spike (needs an authenticated Cursor; gates 3–5) — [ ]

- [ ] `@cursor/sdk` local run from a Node script: event shapes, `agent_id`
      stable across turns, `Agent.resume` round-trip, `run.cancel()`.
- [ ] `agent.send("/<custom command>")` with `.cursor/commands/x.md` present
      and `local.settingSources` including `"project"`/`"user"`: does Cursor
      expand it?
  - Outcome A (expands): sidecar only needs to *list* commands (scan
    `.cursor/commands`, `~/.cursor/commands`; name = file stem, description =
    first line) and send text unchanged.
  - Outcome B (does not): sidecar expands `/name args` → file body + args
    before `send`, ack'd back to host; flagged as a contract extension.
- [ ] `Cursor.models.list()` shape incl. `params`; image input support;
      where run logs land (affects `ajax cost` log scan).
- [ ] Record results in this file before starting Task 3.
- STATUS 2026-10-13: BLOCKED on Cursor auth in this environment (`agent -p`
  → "Authentication required"; router probe: macOS login keychain locked;
  `agent acp` printed CLI help). Task 3 can be built against a fake SDK module
  (as `fake_claude_sdk.mjs` does for Claude); live behavior + outcome A/B
  still needs the spike.

### 0b. SDK type-definition review (no auth needed) — DONE 2026-10-13

Source: `npm pack @cursor/sdk@1.0.37` (Node >=22.13), read `dist/esm/*.d.ts`
and runtime strings; nothing executed.

- **Auth is not the CLI login.** Resolution is `apiKey` option →
  `CURSOR_API_KEY` env → SDK's own credential store (`Cursor.auth.login()`,
  `FileCredentialStore`, `getDefaultSdkAuthPath`). ACP rode `agent`'s own
  login/keychain. **New operational requirement + changed auth assumption**
  (stop-condition item) — awaiting user decision.
- **No local slash-command surface.** No `.cursor/commands` / list-commands
  API in the types; `slashCommand` strings in the runtime belong to a cloud
  onboarding flow, and `GetRepoSlashCommands` is a cloud RPC. So a sidecar
  must scan `.cursor/commands` + `~/.cursor/commands` itself to *list*
  (outcome A), and whether `send("/x")` expands locally cannot be known
  statically (local core is a platform binary). Expansion (outcome B) stays
  gated on the live spike + user sign-off.
- **Images supported:** `SDKUserMessage { text, images?: SDKImage[] }` — the
  "text-only" regression in the behavior table can be avoided later.
- Events: `SDKAssistantMessage`, `SDKThinkingMessage`, `SDKToolUseMessage`,
  `SDKUsageMessage`, `SDKStatusMessage`, `SDKTaskMessage`, `SDKSystemMessage`.
  `Run.cancel()`, `Run.steer?()`. `SettingSource` = project|user|team|mdm|
  plugins|all (load `project`+`user` so `.cursor/commands` rules apply).
- **Cursor ACP already passes `/commands` through** (ACP
  `available_commands_update`). Packet 1 gives Claude and Pi the same; the
  Cursor-specific gain from this swap is transport parity, not commands.

### 1. Shared commands passthrough (independent of Cursor; can ship first) — [x] DONE 2026-10-13

- [x] Test: fake Claude SDK sidecar with `supportedCommands` → client exposes
      commands (`spawn_exposes_the_advertised_command_list`).
- [x] Test: fake Pi `get_commands` → same; degraded handshake → empty.
- [x] Impl (DEVIATION from plan, see below): one-shot
      `take_available_commands()` on `ClaudeSdkClient`/`PiRpcClient`;
      `AcpEventSource::take_available_commands` (default empty);
      `SessionClient` converts `NativeCommand` → `AvailableCommandDescriptor`;
      `acp_drain` seeds `session_available_commands` before the poll loop and
      ACP `AvailableCommandsUpdated` replaces it.
- [x] Test: second drain of the same client yields `None` (no repeated snapshot).
- [x] Verify: fmt clean; clippy `--all-targets --all-features -D warnings`
      clean; `cargo nextest -p ajax-web -p ajax-core` 2094/2095 — sole failure
      `live_cursor_initialize_advertises_load_session` (spawns real `agent acp`,
      environmental, pre-existing, deleted by Task 5).

**Deviation:** the plan said `NativeEvent::AvailableCommands`. Round 1 (Qwen)
implemented that; it queued the list ahead of the turn stream and broke three
existing Pi tests that expect the first event to be the chunk
(`abort_flow_finishes_cancelled`, `pi_variant_*`). Handshake data already
travels via `session_new_result`/spawn-time state, so delivery became a typed
one-shot accessor. A first accessor version was *persistent* and would have
re-emitted `pending_commands_snapshot` on every `pump()`; caught in review,
fixed to one-shot with a regression test. `NativeEvent` has no new variant;
`rpc_client.rs` and `native_map*.rs` are unchanged.

**Known edge (not fixed):** `AcpSlot.session_available_commands` is not reset
when a client is replaced (same as config options). Swapping into a native
harness that advertises *no* commands keeps the previous harness's list until
the next update. Fixing it needs the reset ordered against the first drain
without wiping Codex's commands; do it with Task 3's spawn-report work.

### REVISION 2026-10-13 (user direction): native = the GLOBAL `agent` CLI

User: "it should use the native cursor global". Supersedes the `@cursor/sdk`
sidecar (old Tasks 2–5, removed). Reasons confirmed by inspection:

- Uses the CLI's own login — **auth assumption unchanged** (stop-condition
  cleared); no `CURSOR_API_KEY`, no Node ≥ 22.13 floor, no npm dependency.
- Global `agent` offers: `create-chat` (new chat id, works unauthenticated),
  `-p --output-format stream-json --stream-partial-output`, `--resume <id>`,
  `--model`, `--force`, `--trust`, `--workspace`.
- ACP ids are NOT chat ids (0/300 overlap between `~/.cursor/acp-sessions`
  and `~/.cursor/chats`) → stored Cursor ACP session ids fail closed to
  "Start fresh" (already in the behavior table).
- Cost of CLI-native vs SDK: one process **per turn** (not a long-lived host),
  no thinking events in print mode (docs), no usage event (ACP had none
  either), permissions via `--force` (ACP already auto-approved), no
  elicitation, `/cmd` expansion unverified (needs live run).

Architecture: a third native transport next to Claude SDK / Pi RPC, plugged
into the same `SessionClient` + `NativeEvent` layer. It is *not* a
`JsonlProcess`/`RpcSession` client (those assume a persistent child), so it
gets its own small per-turn process client, but reuses `NativeEvent`,
`NativeCommand`, `NeutralConfigOption`, the one-shot commands accessor and the
existing `agent models` parser in `slices/session_models.rs`.

### 2. Mapper `cursor_cli_map.rs` (pure) — [x] DONE 2026-10-13

- [x] Test (from the documented stream): `system/init` → session id;
      `assistant` with `timestamp_ms` and no `model_call_id` → `AgentText`
      (others are duplicates, skipped); `tool_call started/completed` with
      `call_id` → `ToolCall` / `ToolCallUpdate`; `result success` →
      `TurnFinished Ok`; `result is_error` → `TurnFinished Err`; unknown
      lines ignored.
- [x] Impl: no ACP types; reuse tool-kind naming from `claude_sdk_map` /
      `pi_rpc_map` rather than a new vocabulary.
- Verify: fmt/clippy clean; nextest -p ajax-web -p ajax-core 2105/2106 (only the
  known `live_cursor_initialize…`); 11/11 `cursor_cli_map` tests. Round 1 (Qwen)
  timed out mid-edit with a red gate; round 2 (GLM) fixed 4 mechanical
  findings. Module is `#![allow(dead_code)]` until Task 4 wires it.

### 3. Per-turn client `cursor_cli_client.rs` — [ ] (3a DONE 2026-10-13: core; 3b cancel + process-group kill pending)

- [ ] `turn_argv(...)` pure + tested:
      `agent -p --output-format stream-json --stream-partial-output --force
      --trust --workspace <wt> --resume <chat> [--model <m>] <prompt>`.
- [ ] Client: `begin_prompt` spawns the turn, reader thread parses NDJSON via
      Task 2 into a queue; `cancel` kills the turn's process group;
      `prompt_in_flight`, `shutdown`, `child_pid` (current turn or None);
      a turn that exits without `result` → `TurnFinished Err`.
- [ ] Test against `tests/fixtures/fake_cursor_cli.js` (like `fake_pi_rpc.js`).
- [ ] No `host_exited` false-positives between turns (no persistent host).

### 4. Spawn + `SessionClient::Cursor` + commands — [ ]

- [ ] `create-chat` for a fresh id; resume id passed through; fail closed
      (a failed `--resume` turn is an error, never a silent fresh chat).
- [ ] Report: restore advertised, image capability false (CLI has no image
      input), model from pin, neutral config options (model).
- [ ] `cursor_commands.rs`: scan `<wt>/.cursor/commands/*.md` and
      `~/.cursor/commands/*.md` → `NativeCommand` (stem, first line), exposed
      through the existing one-shot `take_available_commands()`. Text is sent
      unchanged (outcome A); expansion (outcome B) still needs live spike +
      sign-off.

### 5. Catalog / pins — [ ]

- [ ] Cursor catalog from `agent models` via the existing `session_models`
      parser (replaces `read_cursor_acp_model_labels`); persisted pins
      (`cursor-grok-4.6-high`, `model|k=v`) still decode; model applied via
      `--model` on the next turn.

### 6. Flip + delete — [ ]

- [ ] `HarnessTransport::CursorCli`; Cursor launch row → `agent`/`cursor-agent`
      (no `acp`), selector like `uses_pi_rpc` (test override keeps ACP tests).
- [ ] Delete Cursor-ACP-only code: `parameterizedModelPicker` handling in
      `config_options.rs`, `read_cursor_acp_model_labels`,
      `CURSOR_DEFAULT_SPAWN_MODEL` spawn pinning, ACP HTTP/2 notes, tests that
      pin only that mechanism (sign-off already given in Approval).
- [ ] Docs: `web-session-behavior.md` Launch/Native sections.

### 7. Live verification (needs an authenticated `agent`) — [ ]

- [ ] Real turn, resume by chat id, cancel, `/cmd`, `agent models` text,
      long tool output. Update outcome A/B and the behavior table.

## Behavior changes the user must accept (stop-condition review)

| Lost / changed | Why |
| --- | --- |
| Cursor ACP form elicitation | SDK has no elicitation channel (Claude path already declines) |
| Cursor image attachments | start text-only like Claude/Pi until Task 0 confirms SDK image input |
| Stored Cursor ACP session ids | not SDK agent ids → fail-closed → existing "Retry / Start fresh" |
| Node ≥ 22.13 for Cursor chat | `@cursor/sdk` requirement; terminal mode unaffected |
| `@cursor/sdk` is public beta | event `args`/`result` schemas documented as unstable |

## Delegation

Per AGENTS.md: model-router → acpx `scripts/run-delegate`; Qwen in small
packets (one module per packet, inline signatures, "do not explore"); every
prompt bans `git stash/checkout/reset`. Packets: 1, 2, (0 by hand — needs
Cursor auth), 3, 4, 5, 6. Frontier agent reviews each delta.

## Validation

- `cargo nextest run -p ajax-web -p ajax-core` (focused per packet first)
- `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`
- rustdoc `-D warnings` per `ci.yml`
- `npm run verify` (PR gate); web:smoke not required (no UI change)
- Live: authenticated Cursor task → chat turn, `/command` from composer,
  cancel, restart + resume.

## Risks

- Task 0 outcome B grows scope (Ajax-side command expansion).
- SDK beta churn; pin a version in install hint.
- Model catalog/pin decoding regressions for existing Cursor tasks.
- Rust file limit: `client.rs` (913) and `config_options.rs` (816) are near
  1,000 — deletions in Task 5 help; no new feature lands in them.

## Approval

- [x] Transport = `@cursor/sdk` sidecar
- [x] Behavior-change table accepted
- [x] Test deletions scoped to removed ACP mechanism approved
- [x] Shared-layer interpretation (no directory rename) accepted
