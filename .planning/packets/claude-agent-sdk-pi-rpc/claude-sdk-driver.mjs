// Throwaway protocol spike: drives @anthropic-ai/claude-agent-sdk query() in streaming-input mode.
// Usage: node claude-sdk-driver.mjs <prompt|toolcall|interrupt|resume>
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SDK_PATH =
  "/Users/matt/.nvm/versions/node/v22.23.1/lib/node_modules/@agentclientprotocol/claude-agent-acp/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs";
const { query, getSessionMessages } = await import(SDK_PATH);

const DIR = path.dirname(fileURLToPath(import.meta.url));
const FILES = {
  prompt: "claude-sdk-prompt.jsonl",
  toolcall: "claude-sdk-toolcall.jsonl",
  interrupt: "claude-sdk-interrupt.jsonl",
  resume: "claude-sdk-resume.jsonl",
};
const INTROSPECTION = "claude-sdk-introspection.json";
const mode = process.argv[2];
if (!FILES[mode]) throw new Error(`unknown mode: ${mode}`);
const outFile = path.join(DIR, FILES[mode]);
writeFileSync(outFile, "");

const line = (obj) => appendFileSync(outFile, JSON.stringify(obj) + "\n");

function makeInput() {
  const queued = [];
  let waiting = null;
  let closed = false;
  return {
    push(msg) {
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: msg, done: false });
      } else queued.push(msg);
    },
    end() {
      closed = true;
      if (waiting) {
        const resolve = waiting;
        waiting = null;
        resolve({ value: undefined, done: true });
      }
    },
    [Symbol.asyncIterator]() {
      return this;
    },
    next() {
      if (queued.length) return Promise.resolve({ value: queued.shift(), done: false });
      if (closed) return Promise.resolve({ value: undefined, done: true });
      return new Promise((resolve) => (waiting = resolve));
    },
    return() {
      closed = true;
      return Promise.resolve({ value: undefined, done: true });
    },
  };
}

const userMessage = (text) => ({
  type: "user",
  message: { role: "user", content: text },
  parent_tool_use_id: null,
  session_id: "",
  origin: { kind: "human" },
});

function baseOptions(extra) {
  return {
    cwd: DIR,
    model: "haiku",
    includePartialMessages: true,
    ...extra,
  };
}

async function runTurn({ prompt, options, onMessage }) {
  const input = makeInput();
  const q = query({ prompt: input, options: baseOptions(options) });
  input.push(userMessage(prompt));
  for await (const m of q) {
    line(m);
    await onMessage?.(m, q);
    if (m.type === "result") input.end();
  }
  return q;
}

function canUseToolRecorder(decisionLog) {
  return async (toolName, input, opts) => {
    const { signal, ...rest } = opts;
    line({ kind: "canUseTool", toolName, input, options: rest });
    decisionLog.push(toolName);
    return { behavior: "allow", updatedInput: input, toolUseID: opts.toolUseID };
  };
}

async function modePrompt() {
  const introspection = {};
  await runTurn({
    prompt: "Reply with the single word ok",
    options: {},
    onMessage: async (m, q) => {
      if (m.type !== "result") return;
      introspection.sessionId = m.session_id;
      introspection.supportedModels = await q.supportedModels();
      introspection.supportedCommands = await q.supportedCommands();
      introspection.contextUsageSummary = await q.getContextUsage({ detail: "summary" });
    },
  });
  writeFileSync(path.join(DIR, INTROSPECTION), JSON.stringify(introspection, null, 2));
}

async function modeToolcall() {
  const decisions = [];
  await runTurn({
    prompt:
      "Use the Write tool to create a file named canuse-probe.txt in the current directory containing exactly the text hi. Then reply with the single word done.",
    options: { canUseTool: canUseToolRecorder(decisions) },
  });
  line({ kind: "driver.summary", canUseToolCalls: decisions });
}

async function modeInterrupt() {
  let interrupted = false;
  let seenEvents = 0;
  await runTurn({
    prompt:
      "Count from 1 to 500, writing each number on its own line with no other text.",
    options: {},
    onMessage: async (m, q) => {
      if (m.type === "stream_event") seenEvents += 1;
      if (!interrupted && seenEvents >= 5) {
        interrupted = true;
        const response = await q.interrupt();
        line({ kind: "driver.interrupt_returned", response: response ?? null });
      }
    },
  });
}

async function modeResume() {
  const prior = readFileSync(path.join(DIR, FILES.prompt), "utf8")
    .split("\n")
    .filter(Boolean)
    .map((l) => JSON.parse(l));
  const sessionId = prior.find((m) => m.type === "result")?.session_id;
  if (!sessionId) throw new Error("no session_id in claude-sdk-prompt.jsonl");

  await runTurn({
    prompt:
      "What single word did you reply with in the earlier turn? Reply with only that word.",
    options: { resume: sessionId },
  });

  const messages = await getSessionMessages(sessionId, {
    dir: DIR,
    includeSystemMessages: true,
  });
  messages.forEach((message, index) =>
    line({ kind: "getSessionMessages", sessionId, index, message }),
  );
}

const modes = {
  prompt: modePrompt,
  toolcall: modeToolcall,
  interrupt: modeInterrupt,
  resume: modeResume,
};
try {
  await modes[mode]();
} catch (error) {
  line({ kind: "driver.error", message: String(error?.message ?? error) });
}
