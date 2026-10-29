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
  fakeCall("queryOptions", [options]);
  let interrupted = false;
  let wedged = false;
  let throwing = false;
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
    if (text.includes("ELICIT")) {
      systemInit();
      const secret = text.includes("ELICIT_SECRET");
      const answer = await options.onElicitation?.({
        serverName: "fake",
        message: "Pick a name",
        mode: "form",
        requestedSchema: {
          type: "object",
          properties: secret ? { apiKey: { type: "string" } } : { name: { type: "string" } },
          required: [secret ? "apiKey" : "name"],
        },
      });
      const reply = `elicit:${answer?.action ?? "none"}:${JSON.stringify(answer?.content ?? null)}`;
      streamEvent(reply);
      assistant([{ type: "text", text: reply }], "end_turn");
      result("success", { result: reply });
      return;
    }

    if (text.includes("PUMP_FAIL")) {
      systemInit();
      streamEvent("pre-crash");
      throwing = true;
      return;
    }
    if (text.includes("EXHAUST")) {
      systemInit();
      streamEvent("dying");
      queue.end();
      return;
    }
    if (text.includes("WEDGE")) {
      systemInit();
      streamEvent("wedged");
      wedged = true;
      await new Promise(() => {});
      return;
    }
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
    if (text.includes("TASKS")) {
      systemInit();
      assistant(
        [{ type: "tool_use", id: "toolu_fake_task_a", name: "TaskCreate", input: { subject: "alpha", description: "Task alpha" } }],
        "tool_use",
      );
      queue.push({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_fake_task_a", content: "Task #1 created successfully: alpha" }],
        },
        tool_use_result: { task: { id: "1", subject: "alpha" } },
      });
      assistant(
        [{ type: "tool_use", id: "toolu_fake_task_b", name: "TaskCreate", input: { subject: "beta", description: "Task beta" } }],
        "tool_use",
      );
      queue.push({
        type: "user",
        session_id: sessionId,
        parent_tool_use_id: null,
        message: {
          role: "user",
          content: [{ type: "tool_result", tool_use_id: "toolu_fake_task_b", content: "Task #2 created successfully: beta" }],
        },
        tool_use_result: { task: { id: "2", subject: "beta" } },
      });
      assistant(
        [{ type: "tool_use", id: "toolu_fake_upd_1", name: "TaskUpdate", input: { taskId: "1", status: "in_progress" } }],
        "tool_use",
      );
      assistant(
        [{ type: "tool_use", id: "toolu_fake_upd_2", name: "TaskUpdate", input: { taskId: "1", status: "completed" } }],
        "tool_use",
      );
      result("success", { result: "tasks done" });
      return;
    }
    if (text.includes("ASK")) {
      systemInit();
      const hook = options.hooks?.PreToolUse?.[0]?.hooks?.[0];
      let decision;
      try {
        decision = await hook?.({
          hook_event_name: "PreToolUse",
          tool_name: "AskUserQuestion",
          tool_input: {
            questions: [
              {
                question: "Red or blue?",
                header: "Color",
                options: [{ label: "Red" }, { label: "Blue" }],
                multiSelect: false,
              },
            ],
          },
        });
      } catch (error) {
        decision = { error: String(error?.message ?? error) };
      }
      const reply = `ask:${JSON.stringify(decision ?? null)}`;
      streamEvent(reply);
      assistant([{ type: "text", text: reply }], "end_turn");
      result("success", { result: reply });
      return;
    }
      systemInit();
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
    const imageBlocks = (message?.message?.content ?? []).filter((block) => block?.type === "image");
    const reply =
      imageBlocks.length > 0
        ? `ok images=${imageBlocks.length}:${imageBlocks[0].source.media_type}`
        : "ok";
    streamEvent(reply);
    assistant([{ type: "text", text: reply }], "end_turn");
    result("success", { result: reply });
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
      return { next: () => (throwing ? Promise.reject(new Error("fake SDK stream exploded")) : queue.next()) };
    },
    async interrupt() {
      fakeCall("interrupt", []);
      if (wedged) return new Promise(() => {});
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

export async function getSessionInfo(sessionId, options) {
  if (sessionId?.startsWith("missing-")) return undefined;
  if (sessionId?.startsWith("boom-")) throw new Error("lookup exploded");
  return { sessionId, summary: "fake", cwd: options?.dir };
}
