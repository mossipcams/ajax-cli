// Fake @cursor/sdk for the cursor_sdk_sidecar tests.
//
// Exports `Agent { create, resume }` and `Cursor { models: { list } }`, shaped
// exactly like the real SDK surface the sidecar codes against:
//   Agent.create({ local: { cwd, settingSources }, model }) -> Promise<agent>
//   Agent.resume(agentId, { local, model })  (throws for unknown ids)
//   agent.agentId / agent.close() / agent.send(text | {text,images},
//   {model,mode}) -> Promise<run>
//   run.stream(): AsyncGenerator<message>; run.wait(); run.cancel()
// Deterministic and network-free. Text scripts:
//   - contains "FAIL": status RUNNING, then status ERROR "out of usage";
//     wait() -> { status: "error", error: { message: "out of usage" } };
//   - contains "SLOW": status RUNNING, then waits (up to 10s) until cancel();
//     then status CANCELLED; wait() -> { status: "cancelled" };
//   - contains "LATE": send() resolves only after 100ms (abort-race test);
//     echo text gains ` images=<n>` / ` mode=<m>` when those are passed.
//   - otherwise the full happy path with a tool_call whose call_id
//     deliberately embeds a newline. `send` writes one console.log line so the
//     tests prove the sidecar keeps stdout free of non-protocol text.

const SLOW_MAX_MS = 10_000;

const MODELS = [
  {
    id: "default",
    displayName: "Auto",
    variants: [{ params: [], isDefault: true }],
  },
  {
    id: "grok-4.6",
    displayName: "Grok 4.6",
    parameters: [{ id: "effort", values: [{ value: "low" }, { value: "high" }] }],
    variants: [{ params: [{ id: "effort", value: "high" }], isDefault: true }],
  },
];

function makeAgent(agentId) {
  const agent = {
    agentId,
    closed: false,
    close() {
      agent.closed = true;
    },
    async send(input, { model, mode } = {}) {
      // SDK noise on the console: the sidecar must keep this off stdout.
      console.log("sdk noise");
      const text = typeof input === "string" ? input : input.text;
      const images = typeof input === "string" ? [] : (input.images ?? []);
      let extraEcho = "";
      if (images.length > 0) extraEcho += ` images=${images.length}`;
      if (mode !== undefined) extraEcho += ` mode=${mode}`;
      const run = makeRun(agentId, text, model, extraEcho);
      // "LATE" resolves only after 100ms so an abort can land before state.run.
      if (text.includes("LATE")) await new Promise((resolve) => setTimeout(resolve, 100));
      return run;
    },
  };
  return agent;
}

function makeRun(agentId, text, model, extraEcho = "") {
  const runId = `run-${agentId}`;
  const stamped = (message) => ({ agent_id: agentId, run_id: runId, ...message });
  let cancelled = false;

  function slowWait() {
    return new Promise((resolve) => {
      // Poll for cancel(); never sleeps longer than the 10s cap.
      const startedAt = Date.now();
      const timer = setInterval(() => {
        if (cancelled || Date.now() - startedAt >= SLOW_MAX_MS) {
          clearInterval(timer);
          resolve();
        }
      }, 25);
    });
  }

  function* happyStream() {
    yield stamped({ type: "status", status: "RUNNING" });
    yield stamped({ type: "thinking", text: "hm" });
    yield stamped({ type: "assistant", message: { content: [{ type: "text", text: `echo[${model?.id ?? "?"}]: ${text}${extraEcho}` }] } });
    yield stamped({ type: "tool_call", status: "running", call_id: "call-1\nfc_1", name: "read" });
    yield stamped({ type: "tool_call", status: "completed", call_id: "call-1\nfc_1", name: "read" });
    yield stamped({ type: "usage", usage: { inputTokens: 1, outputTokens: 2, totalTokens: 3 } });
    yield stamped({ type: "status", status: "FINISHED" });
  }

  const run = {
    async *stream() {
      if (text.includes("FAIL")) {
        yield stamped({ type: "status", status: "RUNNING" });
        yield stamped({ type: "status", status: "ERROR", message: "out of usage" });
        return;
      }
      if (text.includes("SLOW")) {
        yield stamped({ type: "status", status: "RUNNING" });
        await slowWait();
        yield stamped({ type: "status", status: cancelled ? "CANCELLED" : "FINISHED" });
        return;
      }
      for (const message of happyStream()) yield message;
    },
    async wait() {
      if (text.includes("FAIL")) {
        return { status: "error", error: { message: "out of usage" } };
      }
      if (text.includes("SLOW")) {
        await slowWait();
        return { status: cancelled ? "cancelled" : "finished" };
      }
      return { status: "finished", usage: { inputTokens: 1, outputTokens: 2, totalTokens: 3 } };
    },
    async cancel() {
      cancelled = true;
    },
  };
  return run;
}

export const Agent = {
  create(_options) {
    return Promise.resolve(makeAgent("agent-fake-1"));
  },
  resume(agentId, _options) {
    if (agentId === "agent-missing") return Promise.reject(new Error("Agent agent-missing not found"));
    return Promise.resolve(makeAgent(agentId));
  },
};

export const Cursor = {
  models: {
    async list() {
      return MODELS;
    },
  },
};
