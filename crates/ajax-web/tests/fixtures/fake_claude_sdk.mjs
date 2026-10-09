// Fake @anthropic-ai/claude-agent-sdk for the claude_sdk_sidecar tests.
//
// Exports `query({prompt, options})` returning an object that is BOTH an
// async iterable of SDK messages and a control surface: interrupt(),
// setModel(m), applyFlagSettings(s), supportedModels(), supportedCommands(),
// initializationResult(), getContextUsage(opts), close(). The `prompt`
// argument is the sidecar's open async iterable of user messages; each user
// message plays a scripted turn into ONE internal async queue that the
// iterator drains. Message shapes mirror real SDK output (see
// .planning/packets/claude-agent-sdk-pi-rpc/claude-sdk-prompt.jsonl and
// claude-sdk-toolcall.jsonl), trimmed of hook_* noise. Text scripts:
//   - contains "SLOW": stream text_delta every 40ms until interrupt(),
//     then a user "[Request interrupted by user]" message and a result with
//     subtype error_during_execution / terminal_reason aborted_streaming;
//   - contains "TOOL": calls options.canUseTool, then assistant tool_use /
//     user tool_result / result success;
//   - otherwise: system/init (first turn only), stream_event, assistant,
//     result success.
// Every control-method call is also pushed as {type:'fake_call', method,
// args} so tests can observe the sidecar's dispatch.

const SLOW_TICK_MS = 40;
const MODELS = [
  { value: "default", displayName: "Default" },
  { value: "haiku", displayName: "Haiku" },
];
const COMMANDS = [{ name: "compact", description: "Compact the conversation" }];

function makeQueue() {
  const queued = [];
  let waiting = null;
  let closed = false;
  return {
    push(message) {
      if (closed) return;
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: message, done: false });
      } else queued.push(message);
    },
    end() {
      closed = true;
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: undefined, done: true });
      }
    },
    next() {
      if (queued.length) return Promise.resolve({ value: queued.shift(), done: false });
      if (closed) return Promise.resolve({ value: undefined, done: true });
      return new Promise((resolve) => (waiting = resolve));
    },
  };
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function query({ prompt, options = {} }) {
  const queue = makeQueue();
  const sessionId = options.resume || options.sessionId || "fake-session";
  const model = options.model || "haiku";
  let systemEmitted = false;
  let interrupted = false;
  let closed = false;

  function fakeCall(method, args) {
    queue.push({ type: "fake_call", method, args });
  }

  function usage() {
    return { input_tokens: 1, output_tokens: 1, cache_read_input_tokens: 0, cache_creation_input_tokens: 0 };
  }

  function modelUsage() {
    return { [model]: { name: model, contextWindow: 200000, inputTokens: 10, outputTokens: 10 } };
  }

  function systemInit() {
    if (systemEmitted) return;
    systemEmitted = true;
    queue.push({
      type: "system",
      subtype: "init",
      session_id: sessionId,
      model,
      cwd: options.cwd ?? null,
      tools: ["Write"],
      mcp_servers: [],
    });
  }

  function streamEvent(text) {
    queue.push({
      type: "stream_event",
      session_id: sessionId,
      parent_tool_use_id: null,
      event: { type: "content_block_delta", index: 0, delta: { type: "text_delta", text } },
    });
  }

  function assistant(content, stopReason) {
    queue.push({
      type: "assistant",
      session_id: sessionId,
      parent_tool_use_id: null,
      message: {
        id: "msg_fake_1",
        type: "message",
        role: "assistant",
        model,
        content,
        stop_reason: stopReason,
        stop_sequence: null,
        usage: usage(),
      },
    });
  }

  function result(subtype, extra) {
    queue.push({
      type: "result",
      subtype,
      is_error: subtype !== "success",
      duration_ms: 10,
      num_turns: 1,
      result: extra?.result ?? "",
      session_id: sessionId,
      total_cost_usd: 0,
      usage: usage(),
      modelUsage: modelUsage(),
      ...extra,
    });
  }

  async function playTurn(message) {
    interrupted = false;
    const block = message?.message?.content?.[0];
    const text = block?.type === "text" ? block.text : "";
    if (text.includes("SLOW")) {
      systemInit();
      let ticks = 0;
      while (!interrupted && !closed) {
        ticks += 1;
        streamEvent(`tick ${ticks}`);
        await sleep(SLOW_TICK_MS);
      }
      queue.push({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: { role: "user", content: [{ type: "text", text: "[Request interrupted by user]" }] },
      });
      result("error_during_execution", { terminal_reason: "aborted_streaming", is_error: true });
      return;
    }
    if (text.includes("TOOL")) {
      systemInit();
      const decision = await options.canUseTool?.("Write", { file_path: "x" }, {});
      queue.push({ type: "fake_call", method: "canUseTool", result: decision });
      assistant(
        [{ type: "tool_use", id: "toolu_fake_1", name: "Write", input: { file_path: "x", content: "x" } }],
        "tool_use",
      );
      queue.push({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_fake_1", content: "wrote x" }],
        },
      });
      result("success", { result: "wrote x" });
      return;
    }
    systemInit();
    streamEvent("ok");
    assistant([{ type: "text", text: "ok" }], "end_turn");
    result("success", { result: "ok" });
  }

  (async () => {
    try {
      for await (const message of prompt) {
        if (closed) break;
        await playTurn(message);
      }
    } catch (error) {
      stderr(`fake sdk prompt loop failed: ${error?.stack ?? error}`);
    }
    queue.end();
  })();

  function stderr(text) {
    process.stderr.write(`fake-claude-sdk: ${text}\n`);
  }

  return {
    [Symbol.asyncIterator]() {
      return { next: () => queue.next() };
    },
    async interrupt() {
      fakeCall("interrupt", []);
      interrupted = true;
      return null;
    },
    async setModel(nextModel) {
      fakeCall("setModel", [nextModel]);
    },
    async applyFlagSettings(settings) {
      fakeCall("applyFlagSettings", [settings]);
    },
    async supportedModels() {
      fakeCall("supportedModels", []);
      return MODELS;
    },
    async supportedCommands() {
      fakeCall("supportedCommands", []);
      return COMMANDS;
    },
    async initializationResult() {
      fakeCall("initializationResult", []);
      return { models: MODELS, commands: COMMANDS };
    },
    async getContextUsage(opts) {
      fakeCall("getContextUsage", [opts]);
      return { totalTokens: 25730, maxTokens: 200000, percentage: 13 };
    },
    close() {
      closed = true;
      queue.end();
    },
  };
}

// Session lookup used by the sidecar's fail-closed resume check: `undefined`
// when the session is missing, throws for boom- ids (lookup failure),
// otherwise a small metadata object echoing the project directory.
export async function getSessionInfo(sessionId, options) {
  if (sessionId?.startsWith("missing-")) return undefined;
  if (sessionId?.startsWith("boom-")) throw new Error("lookup exploded");
  return { sessionId, summary: "fake", cwd: options?.dir };
}
