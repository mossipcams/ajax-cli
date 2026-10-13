# Native Claude SDK and Pi RPC events (drop the ACP translation)

## Approval

APPROVED 2026-10-12 ("Just do it"), defaults taken: base #1264, AgentEvent in adapter layer. Architecture change (adapter/slice boundary, event vocabulary); AGENTS.md
requires approval before implementation. Direction requested by the user
2026-10-12: "rewrite Ajax web to handle them instead of converting them to ACP".

## Current state (verified in source)

- `claude_sdk_map.rs` (308 lines) and `pi_rpc_map.rs` (185) build
  `agent_client_protocol` `SessionNotification` / `ToolCall` / `ContentChunk`
  values and wrap them in `AcpClientEvent::SessionUpdate`.
- `rpc_session.rs` runs the mapper (`RecordMapper = fn(&Value,&str) -> RpcMapping`,
  `RpcMapping.events: Vec<AcpClientEvent>`).
- `SessionClient { Acp, Pi, Claude }` (`session_client.rs`) already hides the
  transport, but yields `AcpClientEvent`.
- `slices/web_session/acp_drain.rs` matches `AcpClientEvent`, and `acp_map.rs`
  (568 lines) converts ACP JSON into the web's own `SessionServerEvent`
  (`slices/web_session/mod.rs:111`). So Claude/Pi data currently goes
  native JSON -> ACP schema types -> serialized -> `acp_map` -> `SessionServerEvent`.
- ACP-typed leakage beyond that path: `config_options*`, `available_command_descriptors`,
  `prompt_capability_descriptors`, `apply_model`, `sdk_elicitation`,
  `task_session_{answers,directory,replacement,spawn}`, `output_content`,
  `prompt_content`, `architecture_web_session.rs`.

## Deviation (2026-10-12)

Stage 1 does NOT move `acp_map` out of the slice. `AgentEvent` wraps `AcpClientEvent`
(unchanged ACP path) next to a new `NativeEvent`; only Claude/Pi mappers move to `Native`.
Approval: user said "Just do it" on the plan with base #1264 / adapter-layer type.

## Proposed design

1. Add a harness-neutral `AgentEvent` in the adapter layer, shaped like
   `SessionServerEvent` (text/thought chunks, tool call + update, plan, usage,
   turn finished/aborted, elicitation, error, exited). It carries no
   `agent_client_protocol` types.
2. `claude_sdk_map` and `pi_rpc_map` emit `AgentEvent` directly. Delete their ACP
   schema imports and the `#![allow(dead_code)]`.
3. `SessionClient::next_event` returns `AgentEvent`. The ACP client keeps
   `AcpClientEvent` internally and converts once, at its own edge, by calling
   the existing `acp_map` logic (moved, not rewritten). ACP stays the
   protocol for Codex/Cursor/etc.
4. `acp_drain.rs` consumes `AgentEvent`; ACP-only requests (`ClientRequest`,
   config/model options) move behind capability methods on `SessionClient`
   instead of being matched on in the slice.
5. Config options, model apply, commands and prompt capabilities for Claude/Pi
   get native descriptors rather than ACP `SessionConfigOption`s. This is the
   largest part and is deliberately a later stage.

## Stages (each lands green on its own)

- [x] S1a additive: `NativeEvent`/`AgentEvent` (adapters/web_session_acp/agent_event.rs) and
      `map_native_event` (slices/web_session/native_map.rs) with equivalence tests vs `acp_map`.
      Verified: fmt, clippy -D warnings, nextest native_map 10/10. Only edit to existing code:
      `derive_tool_locations`/`target_from_raw_input` made `pub(super)`.
- [x] S1b (verified: fmt, workspace clippy -D warnings, ajax-web nextest 949/950; the one failure is
      `live_cursor_initialize_advertises_load_session`, which needs the real Cursor CLI/network and is
      unrelated) `SessionClient::poll_event`/`wait_event` return `AgentEvent`; `acp_drain` handles
      `Native` via `map_native_event`; ACP behavior identical (existing tests are the oracle).
- [x] S2 Claude (S2a plumbing + S2b native mapper; verified fmt, clippy, nextest 950/951, doc, verify:arch) mapper emits `AgentEvent` natively; drop ACP imports from `claude_sdk_map`.
- [x] S3 Pi mapper emits native events (verified fmt, clippy, nextest 950/951, doc, verify:arch).
      Note: Pi/RPC tests no longer assert `notification.session_id`, since `NativeEvent` carries no session id.
- [x] S4 Native config/model/catalog/lifecycle/prompt for Claude and Pi. Stack:
      #1285 `refactor/web-session/neutral-config-options` (S4a-S4c, S4e-1) and
      #1286 `refactor/web-session/native-lifecycle-events` (the rest). Every
      packet went to Qwen; the orchestrator reran fmt, workspace clippy
      `-D warnings`, `npm run verify:arch` and `cargo nextest run -p ajax-web
      --no-fail-fast` on each. The only failing test throughout is
      `live_cursor_initialize_advertises_load_session` (needs the real Cursor
      CLI).
  - [x] S4a (a02005ef) neutral `NeutralConfigOption` in `config_model.rs` with
        ACP conversions.
  - [x] S4b (47e289ed, 44e1bf40, 339269e3) the slice receives neutral options
        from `SessionClient` (`SessionSpawnReport`, `SessionConfigOutcome`) and
        passes the existing wire value `SessionConfigValue`. NARROWED (user
        "Yes", 2026-10-13): `config_options.rs` and `apply_model` keep ACP
        types internally as the Cursor/Codex pin logic, so
        `config_options_tests.rs` was not rewritten; `persisted_model_id` and
        `triggers_model_persist` convert neutral options back to ACP to reuse
        it. `NeutralConfigValue` from S4a was deleted as a duplicate.
  - [x] S4c (49de9693, 15e679e2) Claude and Pi build neutral options directly;
        the Claude and Pi model catalog is built natively (user, 2026-10-13:
        "The model catalog should not be ACP").
  - [x] S4d (in the S4d/S4f commit) the slice passes the wire elicitation
        action and content; `sdk_elicitation::action_from_wire` builds the ACP
        action.
  - [x] S4e (a7b684fb, 5988864f, 499f85b1, 7c67f276) native turn-finished,
        error, exited and usage events; prompts reach the Claude and Pi clients
        as plain text (`session_client::prompt_text` flattens content blocks).
        Claude and Pi emit no `AgentEvent::Acp(..)`.
  - [x] S4f guard `native_harness_code_never_names_the_acp_protocol` over the
        ten Claude, Pi and shared RPC files.
  - Existing tests changed, shape only, approved by the user ("Yes"): helpers
    and matches in `session_client_{pin,claude,spawn,}_tests.rs`,
    `claude_sdk_{client,hardening,spawn,map}_tests.rs`,
    `pi_rpc_client_{options,usage,}_tests.rs`, `session_close_tests.rs`.
    Assertions removed because the field no longer exists: five
    `method == "session/prompt"` checks (native turn-finished has no method).
  - Not done, never in scope: the slice still builds prompts as ACP
    `ContentBlock`s (`prompt_content.rs`) and hands them to `SessionClient`.
  - Defect found on the way: #1283 (Pi resume of a never-persisted session),
    fixed in 74660397 and shipped separately as #1284.
- [x] S5 Docs: `docs/architecture/web-session-behavior.md`, `architecture.md`,
      `architecture_web_session.rs` guard rules.

## Non-goals

- No change to ACP-driven harnesses (Codex, Cursor, others).
- No change to the durable-execution kernel, registry schema, or the sidecar protocol.
- No web UI or `SessionServerEvent` wire-format change.

## Open decisions

1. Base branch: do this on #1264 (owns Claude SDK + Pi RPC), not #1280. Doing it
   on #1280 would put the rewrite on top of the durability work.
2. Where `AgentEvent` lives: adapter layer (preferred; keeps the slice from
   depending on adapter types) vs reusing `SessionServerEvent` directly.
3. Sub-agent traffic (`parent_tool_use_id`) is dropped today; native events could
   carry it. Out of scope unless wanted.

## Verification

`cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -D warnings`,
`cargo nextest run --workspace`, `npm run verify:arch`,
`RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`. No test edits.
Not yet run.

## Delegation

Per AGENTS.md, implementation goes through the model-router; small packets per
stage (one module per packet), no git stash/checkout/reset in delegate prompts.
