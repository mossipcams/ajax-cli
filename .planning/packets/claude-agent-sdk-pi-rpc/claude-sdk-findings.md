# Claude Agent SDK protocol spike: findings

Driver: `claude-sdk-driver.mjs` (modes `prompt`, `toolcall`, `interrupt`, `resume`). SDK: `@anthropic-ai/claude-agent-sdk` 0.3.284 at the global claude-agent-acp `node_modules`, imported by absolute path. Model alias `haiku` resolved to `claude-haiku-4-5-20251001`. `includePartialMessages: true` in every turn. Auth worked (`apiKeySource: "none"`, claude.ai login). Every run exits 0; the interrupt run logs the SDK's exit-code-1 error as a `driver.error` line after its result (§5).

Artifacts:
- `claude-sdk-prompt.jsonl` (34 lines): every SDKMessage of one turn, `Reply with the single word ok`.
- `claude-sdk-toolcall.jsonl` (66 lines): `Write` tool turn. `canUseTool` lines are interleaved in arrival order (`kind: "canUseTool"`), followed by `driver.summary`.
- `claude-sdk-toolcall-bash-echo.jsonl`: first attempt with `Run the bash command: echo hi`. Kept as evidence that `echo` does not reach `canUseTool` (§6).
- `claude-sdk-interrupt.jsonl` (rerun): turn ended by `query.interrupt()`. Contains `driver.interrupt_returned` and `driver.error` lines.
- `claude-sdk-resume.jsonl`: second process with `options.resume` set to the prompt session id, then `kind: "getSessionMessages"` lines.
- `claude-sdk-introspection.json`: `supportedModels()`, `supportedCommands()`, `getContextUsage({detail:"summary"})` from the prompt turn. `accountInfo()` was dropped from the driver and its email was redacted from this file.

## 1. Init message (`type: "system"`, `subtype: "init"`)

Sits at index 11 of the prompt turn, after hook messages. Keys: `type, subtype, cwd, session_id, tools, mcp_servers, model, permissionMode, slash_commands, terminal_slash_commands, apiKeySource, claude_code_version, output_style, agents, skills, plugins, capabilities, analytics_disabled, product_feedback_disabled, uuid, memory_paths, messaging_socket_path, fast_mode_state, fast_mode_disabled_reason, per_turn_effort_active, view_mode`.

Observed values:
- `session_id`: a UUID, the same id later used for `resume`.
- `model`: `claude-haiku-4-5-20251001`. `permissionMode`: `default`. `apiKeySource`: `none`.
- `claude_code_version`: `2.1.284`. This is the SDK's own CLI, not the installed `claude` 2.1.293.
- `capabilities`: `["interrupt_receipt_v1","interrupt_cancel_queued_v1","msg_lifecycle_v1","mcp_read_resource_v1","mcp_tool_ui_meta_v1"]`.
- Counts: `tools` 322, `slash_commands` 104, `skills` 66, `mcp_servers` 13, `plugins` 10. Global user settings load by default.

## 2. `supportedModels()` and `supportedCommands()`

`supportedModels()` returned 12 entries. Each has `value, resolvedModel, displayName, description`, plus effort/thinking/fast/auto flags on most rows:

| value | resolvedModel |
|---|---|
| default | claude-opus-5-5 (Default, recommended) |
| opus | claude-opus-5-5 |
| sonnet | claude-sonnet-5-5 |
| claude-fable-5-1 | claude-fable-5-1 |
| haiku | claude-haiku-4-5-20251001 |
| claude-sonnet-5 | claude-sonnet-5 |
| claude-opus-5 | claude-opus-5 |
| claude-fable-5 | claude-fable-5 |
| claude-opus-4-8 | claude-opus-4-8 |
| claude-opus-4-7 | claude-opus-4-7 |
| claude-opus-4-6 | claude-opus-4-6 |
| claude-sonnet-4-6 | claude-sonnet-4-6 |

`supportedCommands()` returned 104 entries with keys `name, description, argumentHint`. First names: `claude-delegate, codex-delegate, coordinator, cursor-delegate, gitnexus-cli, …`. Evidence: `claude-sdk-introspection.json`.

## 3. Usage and context-window numbers

- **Per assistant message:** `message.usage` with `input_tokens, cache_creation_input_tokens, cache_read_input_tokens, output_tokens, cache_creation, service_tier`. Output tokens are reported per message.
- **Result message (per turn):** `usage` (same shape, plus `output_tokens_details.thinking_tokens`). `modelUsage[<model>]` has `inputTokens, outputTokens, cacheReadInputTokens, cacheCreationInputTokens, costUSD, contextWindow, maxOutputTokens, thinkingTokens`. For the prompt turn: `contextWindow` 200000, `maxOutputTokens` 32000. `total_cost_usd` is cumulative across turns per the SDK type doc.
- **`getContextUsage({detail:"summary"})`:** `totalTokens` 25730, `maxTokens` 200000, `rawMaxTokens` 200000, `percentage` 13, `model`, `apiUsage`, plus a `categories` breakdown.
- **Bridge:** reads the window from `modelUsage[...].contextWindow` (`claude-agent-acp/dist/acp-agent.js:3933-3936`) and from `getContextUsage().rawMaxTokens` (`acp-agent.js:6047-6054`).

## 4. Which message ends a turn

The `result` message. In the prompt turn it is `result/success` at index 31 of 34, with `stop_reason: "end_turn"`, `is_error: false`, `result: "ok"`. Two informational messages follow it: `system/session_state_changed` and `system/hook_response`. The SDK type comment at `sdk.d.ts` for `SDKResultMessage` says the same. Tool turns show the same pattern: `assistant` → `user` (tool_result) → `assistant` → `result`.

## 5. Interrupted turn

- Driver called `query.interrupt()` after 5 `stream_event` messages. It resolved to `{"still_queued": []}`, consistent with the `interrupt_receipt_v1` capability.
- Stream shape: partial `assistant` text ("1"), then a `user` message with content `[{"type":"text","text":"[Request interrupted by user]"}]`, then `result` with `subtype: "error_during_execution"`, `is_error: true`, `stop_reason: null`, `terminal_reason: "aborted_streaming"`, `errors: ["[ede_diagnostic] result_type=user last_content_type=n/a stop_reason=null"]`.
- After the result, the driver's `for await` threw `Error: Claude Code process exited with code 1` (`driver.error` line). It did so in both runs (the first attempt and the rerun). The driver ends input after `result`, and the success turns exit 0 under the same handling, so the cause is not pinned down. I did not test whether leaving input open avoids the exit.
- Implication: the consumer must treat an `error_during_execution` result plus a possible exit-code-1 throw as a normal interrupt outcome.

## 6. `canUseTool` and tools

- `Write` tool turn: `canUseTool` was called once, `toolName: "Write"`. Options carry `suggestions` (includes `setMode acceptEdits` for the session), `displayName, description, toolUseID, requestId`. The driver returned `allow` with `updatedInput`, the file was created, and the turn finished with `done`.
- `echo hi` (`claude-sdk-toolcall-bash-echo.jsonl`): zero `canUseTool` calls. Bash `echo` is auto-approved, so the bridge's permission path is not exercised by it.
- Tool call and tool result appear as `assistant` (`tool_use` block) and `user` (`tool_result` block) messages, with `tool_use_result` set on the user message.

## 7. Resume

- Second process, `options.resume` = prompt session `d8655aff-ea7e-4748-977a-5494518ac3e7`. Its `system/init` carries the same `session_id`. The resumed turn answered `ok` to "what single word did you reply earlier?", so it reads the prior history.
- Transcript location: `~/.claude/projects/-Users-matt-Desktop-Projects-ajax-cli--worktrees-ajax-claude-agent-sdk--planning-packets-claude-agent-sdk-pi-rpc/<session_id>.jsonl`. The resumed turn was appended to that file (43 lines in total).
- `getSessionMessages(id, {dir, includeSystemMessages: true})` returned 7 messages: user, assistant, assistant, system, user, assistant, assistant. Written to `claude-sdk-resume.jsonl` as `kind: "getSessionMessages"` lines.

## 8. Is an ACP-created session id usable with `options.resume`?

Yes, by source plus the same mechanism. Evidence:
- The bridge generates new-session ids with `randomUUID()` (`acp-agent.js:6402`, `6414`) and passes them to the SDK as `options.sessionId` for fresh sessions (`acp-agent.js:6751`).
- On resume the bridge passes `resume: sessionId`, and its comment says the resume "names the Claude session, which shares the ACP session id" (`acp-agent.js:1914-1916`).
- Transcript files are `<uuid>.jsonl` under `~/.claude/projects/<encoded cwd>/`. Across all 214 transcripts on disk, every filename is a UUID and equals the `sessionId` inside the file.
- The SDK's `resume` took a UUID from a `query()` session in this spike and resumed it correctly, so the id kind is the one `resume` expects.

Not directly tested: I did not resume an ACP-created transcript, because resuming appends to the user's real session file. There is also no ACP-specific marker. ACP-created sessions show `entrypoint: "sdk-ts"` like any `query()` (83 transcripts carry it), so ACP-created transcripts cannot be picked out from disk. The conclusion rests on the bridge source and the SDK behaviour above.

## Concerns

- Spike transcripts (5 sessions, including the echo and interrupt reruns) were written to `~/.claude/projects/<packet path>/`. They are outside the packet and can be deleted.
- `claude-sdk-driver.mjs` was edited after `claude-sdk-introspection.json` was generated: `accountInfo()` removed, and the email in the generated file was redacted by hand. A rerun will not reproduce that file byte-for-byte.
- The `.planning` packet is untracked. It contains tool-call inputs and cwd paths, and no secrets. Do not commit it without review.


## 9. Interrupt with input left open

Driver: `claude-sdk-interrupt-open.mjs`; transcript: `claude-sdk-interrupt-open.jsonl` (line numbers below are that file). One single long-lived `query()` in streaming-input mode, model `haiku`, `includePartialMessages: true`; the async-iterable prompt queue stayed OPEN across the whole run and was ended only at the very end.

(a) **Does `interrupt()` resolve and what does it return?** Yes — `await q.interrupt()` resolves cleanly and returns `{"still_queued": []}` (line 25, `driver.interrupt_returned`; called at line 24 after 5 `stream_event` messages). No throw.

(b) **Does the interrupted turn end with a result and which one?** Yes — `{type:'result', subtype:'error_during_execution', terminal_reason:'aborted_streaming', is_error:true, result:null}` (line 32; recorded at line 33). It arrived normally through the for-await loop.

(c) **Does the second turn succeed in the same process, no respawn, no `resume`?** Yes. With input still open (`driver.second_prompt_sent` line 34, `input_still_open:true`), the next pushed user message was answered: `{type:'result', subtype:'success', terminal_reason:'completed', is_error:false, result:'ok'}` (line 54). The SDK emitted a second `system/init` message with the **same session_id** `3a953126-6893-4ff9-af6f-1facd740e4b4` (lines 38-39, matching the first init at lines 12-13) and the second result carried that same session_id (line 55, `same_session_as_init:true`). Since no `resume` option was passed and no PID is exposed on any SDK message, session_id identity is the available process-identity evidence — a respawn without `resume` could not preserve it.

(d) **Is 'process exited with code 1' thrown while input is open, only after input ends, or never?** Never. Zero errors were thrown anywhere in the run. After `input.end()` (line 56) and one final `session_state_changed` message, the generator finished normally (`driver.generator_completed_normally`, line 58; `driver.summary` line 59, `loop_threw_after_input_ended:null`). So the earlier driver's exit-code-1 failure was caused by **closing the input stream right after the result**, not by `interrupt()` itself.

(e) **Recommended cancel strategy for a long-lived sidecar:** keep the process and call `interrupt()`. Supported by (a)-(d): `interrupt()` resolves immediately, the aborted turn terminates with a well-formed result, and the very next user message is answered successfully in the same session/process with no respawn and no `resume` option — i.e. interrupt is a safe per-turn cancel inside one long-lived streaming-input query. Respawning with `resume` should be reserved for actual process death, not for cancellation.

## 10. Live smoke of the production sidecar against the real SDK

Driver: `claude-sidecar-live.mjs` (ESM, node 22, no deps). Each scenario spawned a **fresh** child exactly the production way — `spawn('node', ['--input-type=module', '-e', <contents of crates/ajax-web/sidecar/claude_sdk_sidecar.mjs>], { cwd: packet dir })`, no `--sdk-module` or other args — with a 60s deadline and a trailing `shutdown`. Full transcript: `claude-sidecar-live.jsonl` (146 lines, all valid JSON). All four scenarios completed in ~17s total.

**(a) SDK resolution without `--sdk-module`: worked.** stderr tails for every scenario show the first two candidates failing and the third loading (only two failure lines ever appear, then init succeeds):
- `claude-sdk-sidecar: sdk candidate failed (@anthropic-ai/claude-agent-sdk (package import)): Cannot find package '@anthropic-ai/claude-agent-sdk' imported from .../.planning/packets/claude-agent-sdk-pi-rpc/[eval1]`
- `claude-sdk-sidecar: sdk candidate failed (/Users/matt/.nvm/versions/node/v22.23.1/lib/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs (npm root -g)): Cannot find module '...sdk.mjs' imported from .../[eval1]`

So neither the bare package import nor `$(npm root -g)` location resolves in this environment; the sidecar's fallback chain loaded the SDK from its remaining (acp-root) candidate. Resolution is therefore **silent except for two stderr noise lines** — callers should not treat those stderr lines as fatal.

**(b) Init response shape.** Scenario A init response (`driver.init_recorded`, line 5): `success:true`, keys exactly `[commands, initialization, models, sessionId]`. `sessionId` = `951a5071-0a01-4020-bd63-1a17bf246015`. **models: 5**, values `['default','opus','claude-fable-5-1[1m]','sonnet','haiku']` (note the mangled name — see (g)). **commands: 106**. `initialization` is present.

**(c) Prompt flow completed.** Scenario A prompt "Reply with the single word ok" streamed through to a `result`: `subtype:'success'`, `is_error:false`, result text `ok`. Usage from the result line: input_tokens **10**, output_tokens **362**; `modelUsage` key `claude-haiku-4-5-20251001` with `contextWindow: 200000` (maxOutputTokens 32000). Init latency A = **1583 ms**, B = **1032 ms**.

**(d) CRITICAL — fail-closed resume check against a genuinely resumable session: PASSED.** Scenario B, a brand-new child process, sent `init {cwd, model:'haiku', resume:'951a5071-0a01-4020-bd63-1a17bf246015'}` with the identical cwd string `/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-claude-agent-sdk/.planning/packets/claude-agent-sdk-pi-rpc`. The sidecar's `getSessionInfo` pre-check accepted it: `driver.init_recorded {success:true, sessionId:'951a5071-0a01-4020-bd63-1a17bf246015', error:null}` (line 28). The follow-up prompt "What single word did you reply earlier?" returned result `ok` (line 32) — **the model remembered the earlier word**, confirming true session continuity, not just a resumed id. No false rejection.

**(e) Random UUID correctly rejected.** Scenario C: `init {resume:'123f311a-61bc-4f91-b157-315a1b36b8f6'}` (fresh random UUID, identical cwd) → `success:false`, `error:"session not found: 123f311a-61bc-4f91-b157-315a1b36b8f6"` (line 40). The fail-closed check rejects nonexistent sessions with the expected message.

**(f) Interrupt then reuse: WORKS.** Scenario D, fresh child: prompt "Count slowly from 1 to 100...", abort sent after the 3rd `stream_event`. Aborted turn ended with `{type:'result', subtype:'error_during_execution', terminal_reason:'aborted_streaming', is_error:true}` (jsonl line 116; `driver.interrupted_result_recorded`). Second prompt "Reply with the single word ok" in the same process then completed: assistant text `ok` streamed (line 137) and final `{type:'result', subtype:'success', terminal_reason:'completed', is_error:false, stop_reason:'end_turn'}` (line 142). **Caveat:** a bug in this throwaway driver matched the *first* result line for both the first- and second-result branches in one loop iteration, so it sent `shutdown` while turn 2 was still streaming; the raw transcript (lines 120–143) shows the sidecar ignored/completed through that mid-turn shutdown and turn 2 finished cleanly. Driver-level defect, not a sidecar defect — the driver's own `second_result_recorded` marker is therefore unreliable, and the evidence above is taken from the raw transcript lines.

**(g) Surprises.**
- **User hooks fire in the sidecar.** Every init emitted 5 `SessionStart:startup` hook events (e.g. line 92+), including a user-local hook injecting a "PONYTAIL MODE ACTIVE" system prompt block (line 98) and an async-hook bootstrap (`{"async": true, "asyncTimeout": 180000}` + metrics JSON, line 107). Production sidecar sessions inherit the operator's `~/.claude` hooks/settings — expected for a real CLI, but it adds latency and prompt injection surface to every Ajax session.
- **Rate-limit telemetry streams as top-level messages:** a `rate_limit_event` with `seven_day utilization 0.91, status allowed_warning` appeared mid-turn (line 108). Consumers that assume only the documented message types will see an unknown type.
- **Mangled model name in init:** one of the 5 model values is literally `claude-fable-5-1[1m]` — an ANSI escape remnant in the SDK-provided catalog, so downstream UI must not render `value`s as trusted display text without sanitizing.
- **Aborted-turn accounting is odd:** the interrupted result reports `usage {input:0, output:0}` and `stop_reason:null` while its `modelUsage` entry shows inputTokens 906 / outputTokens 16 and costUSD 0.000986 — per-model accounting survives the abort even though top-level usage is zeroed.
- **Slow-ish init:** A=1583 ms (first, includes SDK module load + hooks), B=1032 ms. Sub-2s, acceptable; no API key needed (existing claude.ai login worked).

**Bottom line:** the production sidecar resolves the real SDK without `--sdk-module` (via its fallback chain), returns the expected init shape, completes prompt turns with sane usage/context-window numbers, and — most importantly — the new fail-closed resume check accepts a genuinely resumable session across processes (B) while rejecting a nonexistent UUID with `session not found` (C). Interrupt-then-reuse works in one process (D), matching the SDK-level spike from section 9.
