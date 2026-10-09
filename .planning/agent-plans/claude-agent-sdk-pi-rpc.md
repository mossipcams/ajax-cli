# Claude via Agent SDK, Pi via RPC mode (Ajax Chat transports)

Mode: Architecture Change.
Status: **approved 2026-10-08** (Matt chose option (a), Node sidecar) — slice 0 done; slice 1+2 next.
Branch: `ajax/claude-agent-sdk`.

## Goal

Ajax Chat stops reaching Claude and Pi through their ACP bridge packages
(`claude-agent-acp`, `pi-acp@0.0.34`). Claude is driven through the Claude
Agent SDK; Pi through `pi --mode rpc`. Cursor and Codex stay on ACP.

## Evidence (what the tree looks like today)

- Launch table: `acp_launch_for_agent` in `crates/ajax-core/src/adapters/agent.rs:281`
  maps every harness to an ACP entry point; `ajax doctor` and
  `adapters.rs:570` report `acp:<harness>`.
- One transport: `AcpStdioClient` (`adapters/web_session_acp/client.rs:333`)
  spawns the child and hands stdio to `sdk_connection::run`, which speaks ACP
  with the `agent-client-protocol` crate.
- The seam is already narrow: the connection thread consumes `ClientCommand`
  (`sdk_connection.rs:66` — Prompt, Cancel, RespondPermission,
  RespondElicitation, ApplyModelPin, ApplyConfigOption, CloseSession, Shutdown)
  and produces `ConnectionReady` + `AcpClientEvent` (`client.rs:209`).
- The slice is not transport-neutral: 25 production files use ACP schema types
  (`SessionNotification`, `ContentBlock`, `SessionConfigOption`, …) as their
  internal vocabulary, e.g. `acp_map.rs`, `acp_drain.rs`, `prompt_content.rs`.
- Local tools: `pi` 1.0.0 (`--mode rpc` present, protocol in its
  `docs/rpc.md`, `rpc-commands.md`, `rpc-extension-ui.md`), `claude` 2.1.293,
  `@anthropic-ai/claude-agent-sdk` 0.3.284 (only as a dependency of the bridge).
- The Agent SDK ships for TypeScript and Python only. Ajax web is Rust.

## Design

**Keep ACP schema types as the slice's internal event vocabulary.** New
transports translate into `AcpClientEvent` / `ConnectionReady` inside the
adapter. Rewriting 25 files and ~23k lines of slice + tests to a neutral model
is not needed to change the wire protocol, and is a non-goal here.

1. **Transport selection in core.** The launch table gains a transport kind per
   harness: `Acp` (Cursor, Codex), `ClaudeAgentSdk`, `PiRpc`. Doctor and install
   hints follow it (`sdk:claude`, `rpc:pi` instead of `acp:claude`, `acp:pi`).
2. **Pi: Rust speaks RPC directly.** New `adapters/web_session_acp/pi_rpc.rs`
   connection loop: strict LF-split JSONL, command/response correlation by id.
   - Prompt → `prompt`; finish on `agent_settled` (not `agent_end`).
   - Cancel → `abort`.
   - Model / reasoning → `set_model`, `set_thinking_level`; catalog from
     `get_available_models` / `get_available_thinking_levels`, presented as the
     same two config options the UI already renders.
   - Slash commands → `get_commands`. Usage → `get_session_stats`.
   - Restore → launch with the stored session; replay via `get_messages`.
   - Extension UI `select` / `confirm` / `input` → existing form elicitation;
     `notify` / `setStatus` / `setWidget` / `setTitle` ignored or mapped to title.
3. **Claude: a small Node sidecar owned by Ajax.** `query()` from
   `@anthropic-ai/claude-agent-sdk` in streaming-input mode. The sidecar is a
   thin pipe: it forwards each `SDKMessage` as one JSONL line and accepts JSONL
   commands. All mapping lives in Rust (`claude_sdk.rs`), where the tests are.
   - Prompt → streamed `SDKUserMessage`; finish on the `result` message.
   - Cancel → `interrupt()`.
   - Model / effort / mode → `setModel`, `applyFlagSettings`,
     `setPermissionMode`; catalog from `supportedModels()`.
   - Slash commands → `supportedCommands()`. Usage from `result.usage`.
   - Permissions → `canUseTool`, auto-allowed on the host as today.
   - Elicitation → `onElicitation` → existing form elicitation.
   - Restore → `resume: <session id>`; history via `getSessionMessages`.
   - Streaming text/thinking → `includePartialMessages`.
4. **Sidecar packaging.** The script is embedded in the binary
   (`include_str!`), written to the Ajax state dir, and run with `node`. The SDK
   resolves from a global install, with the existing `npx -y <package>` fallback.
   No bundler, no new build step.

## Decision needed before slice 2

How Rust reaches the Agent SDK. Recommendation is (a).

- **(a) Node sidecar using the SDK** — what was asked for; documented API;
  costs a Node runtime + one npm package at runtime (same as the bridge today).
- **(b) Rust drives `claude -p --input-format stream-json --output-format
  stream-json` directly** — no Node, no npm package; but it is not the SDK, and
  the control channel (interrupt, set model, tool permission) is the SDK's
  private wire format, so it can break on any `claude` release.

## Non-goals

- Renaming `Acp*` types/modules or replacing ACP schema types in the slice.
- Changing Cursor or Codex.
- Browser protocol (`protocol v2`) or UI changes.
- New Claude/Pi features the bridges did not expose (subagent views, steering,
  session fork). Add after parity.
- Changing task truth, registry, lifecycle, or the tmux terminal path.

## Open risks (resolve in slice 0)

- **Stored session ids.** Existing rows hold bridge-issued ACP session ids.
  If `claude-agent-acp` ids are Claude session ids they resume unchanged; pi-acp
  ids likely do not map to Pi session files. Fallback is the existing typed
  restore failure (`Retry` / `Start fresh`) — no silent `session/new`.
- **Permission mode.** Today the host sets a full-access ACP mode. The SDK
  equivalent is `bypassPermissions` + `allowDangerouslySkipPermissions`; confirm
  that matches current behavior rather than widening it.
- **Pi usage/context numbers.** `web-session-behavior.md:603` documents a
  pi-acp-specific usage limitation; RPC `get_session_stats` should lift it —
  the doc invariant changes.
- **Test doubles.** `tests/fixtures/fake_acp.js` only speaks ACP. Each new
  transport needs its own fake child (`fake_pi_rpc.js`, `fake_claude_sdk.js`).
  Existing tests are not edited or weakened; harness-specific ones that pin
  bridge behavior need Matt's sign-off before they change.

## Slice 0 findings

Evidence: `.planning/packets/claude-agent-sdk-pi-rpc/` (transcripts + findings).

Pi RPC (done, reviewed 2026-10-08 — all four transcripts parse, 0 bad lines):

- **Stored Pi session ids are resumable.** pi-acp's ACP session id *is* Pi's
  native session id (verified: a pi-acp id names a file under
  `~/.pi/agent/sessions/<project>/<ts>_<id>.jsonl`). Restore =
  `pi --mode rpc --session <id>`; no migration, risk closed for Pi.
- Run completion is `agent_settled`, also after `abort`
  (`message_end.stopReason: "aborted"`); the child stays usable after abort.
- Tool lifecycle arrives outside the message stream:
  `tool_execution_start` / `_update` / `_end` keyed by `toolCallId`.
- `get_session_stats.contextUsage {tokens, contextWindow, percent}` gives real
  context numbers — lifts the pi-acp usage limitation.
- This install emits ~10 `extension_ui_request` records per run, all
  `setStatus` / `setWidget`. They must be dropped, not surfaced as elicitation.
- Launch needs `-na` (`--no-approve`) so the child never blocks on a trust prompt.

Claude Agent SDK 0.3.284 (done, reviewed 2026-10-08 — five transcripts, 0 bad lines):

- **Stored Claude session ids are resumable.** The bridge creates ids with
  `randomUUID()`, passes them as `options.sessionId`, and resumes with
  `resume: sessionId`; all 214 on-disk transcripts have filename == session id.
  Not tested against a real bridge-created chat (would append to a user
  transcript) — confirm on a throwaway chat in slice 5.
- A turn ends at the `result` message; two informational `system` messages can
  follow it and must not reopen the turn.
- **Interrupt:** `interrupt()` resolves, the turn ends with
  `result/error_during_execution` (`terminal_reason: "aborted_streaming"`), then
  the driver saw `Claude Code process exited with code 1`. The driver closed
  input after `result`; whether the process survives with input left open (the
  sidecar's real mode) is **unverified — first thing slice 4 must test**. If it
  dies, cancel means respawn + `resume`.
- Usage: `result.modelUsage[model].contextWindow` and
  `getContextUsage()` give window + percentage.
- `canUseTool` is not called for auto-approved tools (`echo`); it fired for `Write`.
- The stream is noisy: `system/hook_*`, `session_state_changed`,
  `rate_limit_event`, `thinking_tokens` must be dropped by the mapper.
- The SDK runs its own bundled CLI (2.1.284), not the installed `claude`
  2.1.293, and loads global user settings by default (322 tools, 13 MCP
  servers, 104 commands).
- `supportedModels()` → 12 rows with effort flags; `supportedCommands()` → 104.

Spike left 5 throwaway transcripts under
`~/.claude/projects/<packet path>/`; safe to delete.

## Slice 1+2 progress (2026-10-08)

Round A — pure Pi RPC record mapper (`pi_rpc_map.rs`, `pi_rpc_map_tests.rs`,
fixtures under `crates/ajax-web/tests/fixtures/pi_rpc/`, 3 lines in `mod.rs`):

- Qwen round: timed out at 900s, left an uncompilable, undeclared draft.
- GLM revision round: timed out at 900s but left a correct module. Reviewed by
  parent: compiles, 8/8 `pi_rpc_map` tests pass, scope held (only the allowed
  paths), fixtures trimmed (no line > 2KB, no machine paths).
- Gate NOT yet green: `cargo fmt --check` (1 hunk in `pi_rpc_map.rs`) and
  `clippy -D warnings` (2 `redundant_closure` in `pi_rpc_map_tests.rs:114,207`).
- Router stop rule hit (two failed rounds); Matt approved a third, narrowly
  scoped round (fmt + 2 closures) on Qwen. It landed. Parent re-ran the gate:
  `cargo fmt --check` clean, `clippy -p ajax-web --all-targets --all-features
  -D warnings` clean, `nextest pi_rpc_map` 8/8. Scope held (2 files). Round A
  complete; not committed.
- Round B — standalone Pi process module (`pi_rpc_process.rs`,
  `pi_rpc_process_tests.rs`, `tests/fixtures/fake_pi_rpc.js`, +3 lines in
  `mod.rs`): Qwen round FAILED (never declared in `mod.rs`, did not compile,
  placeholder tests that called `abort()`); rejected. GLM revision landed it.
  Parent gate: fmt clean, clippy `-D warnings` clean, `nextest pi_rpc` 18/18
  (8 mapper + 10 process), `node --check` ok, scope held. Not committed.
  Qwen record on Rust modules so far: 0 for 3 (timeout, junk, timeout) —
  it only landed the mechanical lint packet.
- Round C — Pi handshake (`pi_rpc_handshake.rs`, `pi_rpc_handshake_tests.rs`,
  fake child switches): first Qwen round timed out leaving a correct module but
  a non-compiling test file; second, narrowed Qwen round (tests file only)
  landed. Parent gate: fmt clean, clippy `-D warnings` clean, `nextest pi_rpc`
  24/24 with all 6 handshake tests listed (32 assertions in the file, none
  ignored), `node --check` ok, scope held. Not committed. Qwen is reliable on
  narrow, single-file packets with exact compile errors in hand; it timed out
  on every packet that asked for a new module plus tests.
- Round D — Pi session driver core (`pi_rpc_session.rs`,
  `pi_rpc_session_tests.rs`, new fake switches `--reject-prompt`,
  `--hold-run`, `--interleave-text`): first Qwen round timed out leaving a
  correct driver and 2 failing tests whose fixtures were wrong (fake emitted
  the whole run before abort could fire; fake's abort `message_end` put
  stopReason at top level, real Pi nests it under `message`; one test expected
  an event the mapper never produces). Second narrow Qwen round fixed fixtures
  only. Parent gate: fmt clean, clippy `-D warnings` clean, `nextest pi_rpc`
  31/31, both named tests pass, `node --check` ok, no scope violations; the one
  replaced assertion swapped an unsatisfiable `UnknownSessionUpdate` match for
  an exact `AgentMessageChunk "pending-text"` match. Not committed.
- Round E/F — `PiRpcClient` (`pi_rpc_client.rs`, 210 lines) + its 10 tests +
  fake child made executable and honoring `--session <id>`. First Qwen attempt
  timed out with ZERO files written after 45 reads/greps (packet told it to
  read ~8 files); re-sent with every signature inlined and "do not explore" and
  it landed in one round. Parent gate: fmt/clippy clean, `nextest pi_rpc`
  41/41, no scope violations. Known ceiling: the client holds its mutex while
  waiting for an event, so `cancel` from another thread waits up to the event
  timeout; images are not forwarded (`ponytail:` comment).
- Design decision for wiring (parent, 2026-10-08): do NOT refactor
  `AcpStdioClient` (client.rs is 899 lines). Add `SessionClient` enum
  `{Acp(AcpStdioClient), Pi(PiSession)}` with the identical method set, then
  switch the slot (`acp_slot.rs`) and ~8 slice call sites to it. Existing tests
  construct `AcpStdioClient` directly and never put one in a slot, so no test
  edits are needed. Selection stays "always ACP" until the core transport flip.
  Packets: 3a SessionClient + tests; 3b slot/call-site switch + selection
  predicate (always false); 3c Pi model/thinking options + usage
  (`get_session_stats`); 4 core transport kind + `ajax doctor` + flip (BLOCKED on
  Matt: breaks core tests `commands/tests/suite_1.rs:658-681` that pin
  `pi-acp`); 5 docs.
- Round G — `SessionClient` enum + `AcpEventSource` trait (Qwen, landed after a
  narrow trait follow-up); slot/call-site switch (Qwen, 11+/10- lines in 4
  files, behavior unchanged, workspace check + 823 tests green); `PiRpcSession::request`
  (Qwen, 7 tests); Pi model/thinking options + `set_model`/`set_thinking_level`
  on `PiRpcClient` (GLM after two Qwen timeouts; 837/837 green).
  INCIDENT 2026-10-08: a Qwen round ran `git stash push/pop` on the shared stash
  stack and popped another session's `distcheck` stash, leaving
  `crates/ajax-web/web/dist/app.js` unmerged. Entry survived (git keeps a stash
  whose pop conflicts); discarded with `scripts/delegate-delta restore`, file
  verified byte-equal to HEAD, stages cleared, stash stack untouched (50). All
  later delegate prompts ban stash/checkout/reset. See memory
  `delegate_git_stash_incident`.
- Round H — Pi spawn path in `SessionClient` (GLM, after a 177-read exploration
  timeout; re-cut with all facts inlined): `spawn_with_operator_pin`, hard-off
  `uses_pi_rpc` (compile-time false outside tests), fail-closed restore
  (`RestoreFailure::Rejected` if Pi resumes a different id), SpawnReport, `spawn_acp`
  routed through it; then pin application + real `apply_model_pin` /
  `apply_config_option` (GLM; BLOCKED on one stale test input, fixed by Qwen).
  Parent gate: 849/849, fmt/clippy/workspace check clean, no scope violations.
  TEST EDITS TO DISCLOSE (both session-created test files, no assertion
  weakened): `session_client_tests.rs::pi_variant_rejects_model_and_option_changes`
  rewritten to `pi_variant_applies_model_and_option_changes` (it pinned a stub);
  `session_client_spawn_tests.rs` placeholder pin `"operator-pin"` -> `"default"`
  (3 args). Nothing committed. State = Pi RPC transport fully built and tested but
  UNREACHABLE in production until the flip.
- Round I (Matt answered the three held questions: edit event-order tests for
  usage = yes; flip core + update pi-acp test expectations = yes; start Claude
  with the interrupt spike = yes):
  usage (`get_session_stats` before each finish; 2 existing tests in
  `session_client_tests.rs` now skip one UsageUpdate), core flip
  (`HarnessTransport`, Pi launch = `pi`/PiRpc, `rpc_harness_programs`, doctor
  `rpc:pi`; the 4 approved pi-acp test expectations edited; Pi `native_program`
  kept `Some("pi")` so a 5th test needed no edit), web flip (`uses_pi_rpc`
  follows the launch table in production; Pi model catalog from the RPC
  handshake), docs (web-cockpit.md, web-session-behavior.md, README.md).
  Parent gate after each: workspace nextest 2555/2555, fmt/clippy clean, no scope
  violations, 0 unmerged, 50 stashes. PI SIDE COMPLETE. Not live-tested against
  a real chat session and nothing committed.
- (resolved) Held for Matt: (1) usage updates before each finished prompt change the event
  order that existing `pi_rpc_client_tests` prompt-flow test pins (edit that one
  test, or add an opt-in switch); (2) core transport flip breaks
  `commands/tests/suite_1.rs:658-681` which pin `pi-acp`.
- Remaining for slice 1+2: Pi connection loop (spawn `pi --mode rpc -na
  --session <id>`, JSONL framing, command correlation, prompt/abort, restore),
  transport kind on `AcpLaunch`, doctor/install hint, `fake_pi_rpc.js`, docs.

Router notes: the registry id `claude-haiku-5-5` is rejected by the Claude
bridge (`session/set_config_option` invalid value), so the HAIKU route fails
in ~2s without touching the tree. Qwen needs the `127.0.0.1:18000` SSH tunnel
and an API key to probe; unauthenticated probes return 401.

## Task checklist

Each slice is one PR under the 600 lines/file, 1500 lines/PR gate, stacked.

- [x] Slice 0 — spike: capture real `pi --mode rpc` and SDK transcripts for
      prompt / tool call / cancel / resume; settle the session-id risk.
- [ ] Slice 1+2 (merged — a transport field with one value is dead config) —
      core transport kind on the launch table, Pi RPC transport for
      spawn / prompt / tool events / cancel, `fake_pi_rpc.js`, mapping tests,
      doctor + install hint for Pi, `web-cockpit.md` harness table.
- [ ] Slice 3 — Pi restore, model/thinking options, commands, usage, extension UI.
- [ ] Slice 4 — Claude sidecar + `claude_sdk.rs` mapping + `fake_claude_sdk.js`.
- [ ] Slice 5 — Claude restore, model/effort/mode, commands, usage, elicitation.
- [ ] Slice 6 — remove `claude-agent-acp` / `pi-acp` from launch table, README
      install block, smoke-test fakes; update `web-session-behavior.md`.
- [ ] Slice 7 — live check: one real Claude and one real Pi chat session on the
      dev web server (prompt, tool call, cancel, model switch, restart restore).

## Validation

Per slice, the hook-equivalent gate:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-core -p ajax-web
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
```

No UI/layout change is planned, so `web:smoke` is only required if a slice
touches `crates/ajax-web/web`.

Results: none yet.

## Deviations

None yet.


## Claude Agent SDK transport — packet plan (2026-10-08, approved to start by Matt)

Spike result (`claude-sdk-findings.md` §9): one long-lived streaming-input
`query()` survives `interrupt()`; the next prompt works in the same process.
Cancel = keep the sidecar and call `interrupt()`; respawn+`resume` only for
process death. The earlier "exit code 1" came from closing input after `result`.

Design (reuse Pi's stack, do not duplicate it):
- **Sidecar** `claude_sdk_sidecar.mjs` (embedded with `include_str!`, written to the
  Ajax state dir at spawn, run with `node`). It speaks the SAME JSONL framing as Pi
  RPC so `PiRpcProcess` is reused unchanged: stdin commands `{id,type,...}`; stdout
  responses `{id,type:"response",command,success,data|error}` plus raw SDK messages
  as events (their own `type`: system/stream_event/assistant/user/result/...).
  Commands: `init {cwd,model?,resume?,sessionId?,settingSources?}` -> response
  `{sessionId, models, commands}` (from `initializationResult()`/`supportedModels()`/
  `supportedCommands()`); `prompt {message}`; `abort` (-> `interrupt()`);
  `set_model {model}`; `set_effort {level}`; `get_context_usage`; `shutdown`.
  Permissions: `canUseTool` auto-allows (`{behavior:'allow', updatedInput:input}`),
  matching today's trusted-local auto-approve; no `bypassPermissions`.
  `onElicitation` declines (known gap). The SDK module is injectable via
  `AJAX_CLAUDE_SDK_MODULE` (tests use a fake SDK module, zero tokens); otherwise it
  resolves bare `@anthropic-ai/claude-agent-sdk`, then `npm root -g`, then the copy
  nested in `@agentclientprotocol/claude-agent-acp`.
- **Rust**: `claude_sdk_map.rs` (pure SDK message -> `AcpClientEvent`, fixtures from
  the real spike transcripts), session/client mirroring `PiRpcSession`/`PiRpcClient`
  (generalize the session over its mapper rather than copy it), a third
  `SessionClient` variant, fail-closed restore (`resume` must return the same id),
  model/effort options from `supportedModels()`, usage from the `result` message
  (`modelUsage.contextWindow`) / `get_context_usage`.
- **Core**: `HarnessTransport::ClaudeSdk`, Claude launch entry keeps `Some(..)`,
  doctor `sdk:claude` (node + the SDK package + `claude`), web selector follows the
  launch table, Claude model catalog from `init`.

Packets (each: inlined facts, gate = fmt + clippy + whole-workspace nextest, parent
re-runs the gate and reads the diff):
- [x] C1 sidecar + fake SDK module + Rust-driven tests (spawned through `PiRpcProcess`) — GLM, 10 tests, 2565/2565 (before Matt's no-GLM instruction); sidecar 358 lines, no bypassPermissions
- [x] C1b security review finding on commit a70105d4 (unbounded stdout line in `read_stdout_lines`): `read_stdout_lines_bounded` with a 16 MiB `take(max+1)` cap on the READ, one Error then discard-to-LF; 8 tests in `pi_rpc_process_bound_tests.rs`; Qwen needed 3 rounds (reader correct first time; its tests deadlocked on a live sender in `drain`, then one wrong expected value `"\u{2028}"` -> `"a\u{2028}b"` in a brand-new test); parent gate 2573/2573. NOTE: this host has no `timeout` binary, use `perl -e 'alarm N; exec @ARGV' --` for bounded runs (an unwrapped `timeout cargo ...` silently ran nothing)
- [ ] C2 `claude_sdk_map.rs` pure mapper + trimmed real-transcript fixtures + tests
- [ ] C3 generalize `PiRpcSession` over a mapper fn (no behavior change for Pi)
- [ ] C4 `ClaudeSdkClient` (spawn via embedded sidecar, init handshake, prompt/cancel/
      events, usage, options, resume fail-closed) + tests with the fake SDK
- [ ] C5 `SessionClient::Claude` variant + spawn path + catalog (test-gated like Pi)
- [ ] C6 core `HarnessTransport::ClaudeSdk` + doctor `sdk:claude` + selector flip.
      NEEDS MATT'S APPROVAL FIRST: tests pin `claude-agent-acp`
      (`adapters.rs` acp_launch test, `suite_1.rs` doctor tests, CLI smoke fake,
      `every_bridge_harness_names_the_cli...`). The earlier approval covered only the
      four `pi-acp` expectations.
- [ ] C7 docs (web-cockpit, web-session-behavior, README)
- [ ] C8 live smoke against the REAL SDK (not part of CI): confirms `initializationResult()`
      shape, sidecar SDK resolution on this machine, one prompt + one interrupt

Open questions to settle inside the packets, not by guessing: which `settingSources`
the sidecar should default to (the spike loaded ALL global user settings: 322 tools,
13 MCP servers); whether `initializationResult()` works before the first prompt.
