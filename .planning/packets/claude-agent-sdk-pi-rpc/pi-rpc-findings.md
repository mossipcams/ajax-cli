# Pi RPC Mode — Protocol Spike Findings

Read-only spike, 2026-10-08. Nothing in this packet touches repository source; all
files live under `.planning/packets/claude-agent-sdk-pi-rpc/`.

Pi binary: `/Users/matt/.nvm/versions/node/v22.23.1/bin/pi` (global
`@earendil-works/pi-coding-agent`), driven as a child process in RPC mode.
Docs used: `docs/rpc.md`, `docs/rpc-commands.md`, `docs/json.md`, `docs/cli.md`
under the global npm root.

## Exact commands used

Driver (throwaway, not product code):

```bash
cd /Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-claude-agent-sdk/.planning/packets/claude-agent-sdk-pi-rpc
node --check driver.mjs
node driver.mjs
```

`driver.mjs` spawns four child processes (identical args except the last):

```bash
pi --mode rpc --session-dir <spike>/sessions -na                      # runs 1–3
pi --mode rpc --session-dir <spike>/sessions -na --session <sessionId> # run 4 (resume)
```

- `-na` = `--no-approve` (documented in `docs/cli.md`); ignores trust-gated
  project-local files so the child never blocks on an interactive trust prompt.
- `--session-dir <dir>` overrides storage/lookup for session files (takes
  precedence over `PI_CODING_AGENT_SESSION_DIR` and the `sessionDir` setting).
- Each run writes JSONL commands to the child's stdin and records every stdout
  line verbatim into the evidence `.jsonl` files (driver parses each line as
  JSON; unparsable lines would be collected separately — none occurred).
- Closing stdin (`stdin.end()`) lets pi exit cleanly: all four children exited
  with code 0. Stderr was empty on all four runs (`stderr-*.log` are 0 bytes).
- Model used: `glm-5.2` (already selected as default in this environment; no
  `set_model` was needed).

Evidence files (each line is one stdout record, in order):

- `pi-rpc-prompt.jsonl` — one prompt ("Reply with the single word ok") through
  `agent_settled`, plus `get_state` before and after. Assistant content was
  exactly `[{"type":"text","text":"ok"}]`.
- `pi-rpc-toolcall.jsonl` — prompt "Run the bash command: echo hi"; Pi executed
  the `bash` tool (`tool_execution_start` → 2× `tool_execution_update` →
  `tool_execution_end`, `isError:false`, result text `hi\n`), then a second
  turn replying "done", then `agent_settled`.
- `pi-rpc-abort.jsonl` — long prompt, `abort` sent mid-stream after the first
  `text_delta`; run terminated via abort (details below).
- `pi-rpc-resume.jsonl` — second process resuming run 1's session via
  `--session <sessionId>`; includes `get_state`, `get_messages`,
  `get_available_models`, `get_available_thinking_levels`, `get_commands`,
  `get_session_stats` responses plus a follow-up prompt proving context
  continuity (the assistant correctly recalled the earlier request).

## How a session is resumed from the CLI

Session identity (from `get_state`):

- `sessionId`: `"01a11bd7-d94a-7275-883a-469e65d71842"` (a UUID).
- `sessionFile`: absolute path of the persisted session, e.g.
  `<session-dir>/2026-10-08T14-08-13-131Z_01a11bd7-d94a-7275-883a-469e65d71842.jsonl`
  (file name = `<ISO-timestamp>_<sessionId>.jsonl`).

Resume options (from `docs/cli.md` and `pi --help`):

- `--session <path|id>` — open by file path, exact ID, or **partial ID**. Pi
  searches the current project first and offers to fork a cross-project match.
  **Confirmed by the spike**: `pi --mode rpc --session-dir <dir> -na --session
  01a11bd7-d94a-7275-883a-469e65d71842` resumed run 1's session in a new
  process — `get_state` in the child returned the same `sessionId`/
  `sessionFile`, `messageCount: 3`, and the follow-up prompt answered with
  knowledge of the earlier exchange.
- `--session-id <id>` — use exact project session ID, creating it if missing.
- `--continue, -c` — continue the most recent session.
- `--resume, -r` — interactive session picker.
- `--fork <path|id>` — fork a session into a new session (cannot be combined
  with `--session`/`--continue`/`--resume`/`--no-session`).
- Important: the child must pass the **same `--session-dir`** used to create
  the session, or it will look in the default location and not find it.

## Command response shapes (all verbatim from `pi-rpc-resume.jsonl`)

Every response is `{"id","type":"response","command","success","data"|error}``
echoing the request `id`. Failed commands return `success:false` with an
`error` string instead of `data`.

### `get_state` → `data`

```json
{
  "model": {
    "id": "glm-5.2", "name": "GLM-5.2",
    "api": "openai-completions", "provider": "opencode-go",
    "baseUrl": "https://opencode.ai/zen/go/v1",
    "reasoning": true, "input": ["text"],
    "cost": {"input":1.4,"output":4.4,"cacheRead":0.26,"cacheWrite":0},
    "contextWindow": 1000000, "maxTokens": 131072
  },
  "thinkingLevel": "medium",
  "isStreaming": false,
  "isCompacting": false,
  "steeringMode": "all",
  "followUpMode": "one-at-a-time",
  "sessionFile": "<abs path>/sessions/<ts>_<uuid>.jsonl",
  "sessionId": "<uuid>",
  "autoCompactionEnabled": true,
  "messageCount": 3,
  "pendingMessageCount": 0
}
```

(`isStreaming` is the live check for whether a prompt/steer would be queued vs
rejected; `messageCount`/`pendingMessageCount` track queue depth.)

### `get_available_models` → `data.models` (array of ModelInfo)

```json
{ "models": [ {
  "id": "minimax-m3", "name": "MiniMax-M3",
  "api": "anthropic-messages", "provider": "opencode-go",
  "baseUrl": "https://opencode.ai/zen/go",
  "reasoning": true, "input": ["text"],
  "cost": {"input":0.3,"output":1.2,"cacheRead":0.06,"cacheWrite":0},
  "contextWindow": 1000000, "maxTokens": 131072
}, … ] }
```

Each entry is the same shape as `get_state.data.model`. Some entries carry
optional extras: `compat` (e.g. `{"allowEmptySignature":true}`), `inputLimits`
(image resize caps), and `type: "chat"`.

### `get_available_thinking_levels` → `data.levels`

```json
{ "levels": ["off","minimal","low","medium","high"] }
```

(Actual list is model-dependent; this is what `glm-5.2` reports.)

### `get_commands` → `data.commands` (array of CommandInfo)

```json
{ "commands": [ {
  "name": "extensions",
  "description": "Manage local extensions and browse/install community packages",
  "source": "extension",
  "sourceInfo": {
    "path": "/Users/matt/.pi/agent/npm/node_modules/pi-extmgr/src/index.ts",
    "source": "npm:pi-extmgr", "scope": "user", "origin": "package",
    "baseDir": "/Users/matt/.pi/agent/npm/node_modules/pi-extmgr"
  }
}, … ] }
```

Entries come from built-ins plus loaded extensions; `source` can be `"builtin"`
or `"extension"` (with `sourceInfo` describing the extension origin).

### `get_session_stats` → `data`

```json
{
  "sessionFile": "<abs path>/sessions/<ts>_<uuid>.jsonl",
  "sessionId": "<uuid>",
  "userMessages": 1, "assistantMessages": 1,
  "toolCalls": 0, "toolResults": 0, "totalMessages": 3,
  "tokens": {"input":704,"output":3,"cacheRead":9655,"cacheWrite":0,"total":10362},
  "cost": 0.0035091,
  "contextUsage": {"tokens":10362,"contextWindow":1000000,"percent":1.0362}
}
```

### `get_messages` → `data.messages` (array of Message)

Each message has `role` (`system` | `user` | `assistant` | `toolResult`),
`content` (typed blocks: `text`, `thinking`, `tool_call`, `tool_result`),
and `stopReason` on assistant messages (`"stop"` | `"toolUse"` | `"aborted"`).
In the resumed session it returned the full history: system, user, and the
assistant reply from run 1.

## Event stream (what the run actually emits, in order)

For a simple prompt (`pi-rpc-prompt.jsonl`):

```text
response{command:"get_state"}                 # my discovery call
response{command:"prompt", data:{disposition:"started"}}
agent_start
  turn_start
    message_start{role:"system"} → message_end{role:"system"}
    message_start{role:"user"}  → message_end{role:"user"}
    message_start{role:"assistant"} (stopReason:"pending", zeroed usage)
      message_update{assistantMessageEvent.type:"text_start"}
      message_update{assistantMessageEvent.type:"text_delta", delta:"…"}
      message_update{assistantMessageEvent.type:"text_end"}
    message_end{role:"assistant", stopReason:"stop", usage:{…}}
  turn_end
agent_end{messages:[…], willRetry:false}
agent_settled
response{command:"get_state"}                 # idle again
```

For a tool call (`pi-rpc-toolcall.jsonl`), the assistant stream also contains
`thinking_start` / `thinking_delta` / `thinking_end` and
`toolcall_start`/`toolcall_delta`/`toolcall_end` updates (event fields:
`contentIndex`, `id`/`toolName`), `message_end` arrives with
`stopReason:"toolUse"`, and then the tool lifecycle events fire **outside the
message stream**:

```json
{"type":"tool_execution_start","toolCallId":"chatcmpl-tool-…","toolName":"bash","args":{"command":"echo hi"}}
{"type":"tool_execution_update","toolCallId":"…","toolName":"bash","args":{…},"partialResult":"…"}
{"type":"tool_execution_end","toolCallId":"…","toolName":"bash","result":{…},"isError":false}
```

`tool_execution_end.result` carries `content` (typed blocks, e.g. text
`"hi\n"`) plus `details` (compression info, context-hygiene classification).
A `toolResult` role message then enters the history, and a new `turn_start`
follows for the tool-result turn.

### Abort (`pi-rpc-abort.jsonl`)

While streaming (`text_delta` seen), sending `{"id":"a2","type":"abort"}`:

- `response{command:"abort", success:true}` immediately.
- The in-flight assistant `message_update` stream wraps up
  (`thinking_end`, `text_end`), then `message_end{role:"assistant",
  stopReason:"aborted"}`.
- Then `agent_end{…, willRetry:false}` and `agent_settled`.
- The child remains fully usable afterwards (no exit, no error events).

### Run completion marker

**`agent_settled` is the event that marks the end of an agent run.** Sequence
per run: `agent_start` → (turns) → `agent_end{messages, willRetry}` →
`agent_settled`. A driver should wait for `agent_settled` (issued *after* the
final `agent_end`) before treating the prompt as fully processed. Per-turn
completion is `turn_end`; per-assistant-message completion is `message_end`
(`stopReason:"stop"` normal, `"toolUse"` wants tool results, `"aborted"` after
abort). `agent_settled` also fires after an aborted run, so it is the single
wait-point for "run over" in both cases. Extra housekeeping events
(`extension_ui_request` in this install) appear interleaved but are not part
of the agent loop; filter on `type` prefixes you care about.

## Prompt command shape (for reference)

Request: `{"id":"p1","type":"prompt","message":"Reply with the single word ok"}`.
Response: `{"id":"p1","type":"response","command":"prompt","success":true,
"data":{"disposition":"started"}}` — `disposition:"started"` means the prompt
was accepted and streaming began. (Other documented dispositions:
`queued`/`handled` for steering while already streaming; `success:false`
means rejected before acceptance.)

## Other operational notes

- RPC framing is line-delimited JSON both ways (JSONL on stdin and stdout);
  no length-prefixing.
- Session persistence: run 1 + its resume share one file in `sessions/`
  (45,731 bytes); the toolcall and abort runs each created their own session
  file. `get_session_stats` reads the same numbers a resumed process sees.
- `set_model` exists (`{"type":"set_model","modelId":"…"}`) but was not needed;
  the environment's default model was usable from the start.

## Driver artifacts (throwaway)

- `driver.mjs` — the driver itself.
- `driver-summary.json` — per-run metadata (session ids, exits, line counts).
- `sessions/` — the pi session files created by the four child processes.
- `stderr-*.log` — per-run stderr captures (all empty).
