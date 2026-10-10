// Throwaway protocol spike: does interrupt() leave a long-lived streaming-input query usable?
// Single query(), input queue stays OPEN across an interrupt, a second turn, and only ends at the very end.
// Output: claude-sdk-interrupt-open.jsonl (one JSON per line; SDK messages + driver.* markers).
import { appendFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SDK_PATH =
  "/Users/matt/.nvm/versions/node/v22.23.1/lib/node_modules/@agentclientprotocol/claude-agent-acp/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs";
const { query } = await import(SDK_PATH);

const DIR = path.dirname(fileURLToPath(import.meta.url));
const outFile = path.join(DIR, "claude-sdk-interrupt-open.jsonl");
writeFileSync(outFile, "");

const line = (obj) => appendFileSync(outFile, JSON.stringify(obj) + "\n");

// Async-iterable prompt queue: stays OPEN until .end() is called at the very end.
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
    get isOpen() {
      return !closed;
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
  message: { role: "user", content: [{ type: "text", text }] },
  parent_tool_use_id: null,
  session_id: "",
});

const input = makeInput();
const q = query({
  prompt: input,
  options: {
    cwd: DIR,
    model: "haiku",
    includePartialMessages: true,
  },
});

let stage = "counting"; // counting -> second -> finishing
let streamEvents = 0;
let interrupted = false;
let initSessionId = null;
let firstResult = null;
let secondResult = null;
let inputEnded = false;
let loopThrew = null;

line({ kind: "driver.first_prompt_sent", prompt: "Count slowly from 1 to 200, one number per line with no other text." });
input.push(
  userMessage("Count slowly from 1 to 200, one number per line with no other text.")
);

try {
  for await (const m of q) {
    line(m);

    if (m.type === "system" && m.subtype === "init") {
      initSessionId = m.session_id ?? null;
      line({ kind: "driver.init_session_id", session_id: initSessionId });
    }

    if (stage === "counting") {
      if (m.type === "stream_event") streamEvents += 1;
      if (!interrupted && streamEvents >= 5) {
        interrupted = true;
        line({ kind: "driver.interrupt_called", at_stream_events: streamEvents });
        try {
          const response = await q.interrupt();
          line({ kind: "driver.interrupt_returned", response: response ?? null });
        } catch (error) {
          line({
            kind: "driver.interrupt_threw",
            message: String(error?.message ?? error),
          });
        }
      }
      if (interrupted && m.type === "result") {
        firstResult = m;
        line({
          kind: "driver.first_result_recorded",
          subtype: m.subtype ?? null,
          terminal_reason: m.terminal_reason ?? null,
          is_error: m.is_error ?? null,
          session_id: m.session_id ?? null,
        });
        stage = "second";
        line({ kind: "driver.second_prompt_sent", input_still_open: input.isOpen });
        input.push(userMessage("Reply with the single word ok"));
      }
    } else if (stage === "second") {
      if (m.type === "result") {
        secondResult = m;
        line({
          kind: "driver.second_result_recorded",
          subtype: m.subtype ?? null,
          terminal_reason: m.terminal_reason ?? null,
          is_error: m.is_error ?? null,
          session_id: m.session_id ?? null,
          same_session_as_init: (m.session_id ?? null) === initSessionId,
        });
        stage = "finishing";
        inputEnded = true;
        line({ kind: "driver.input_ended", stage });
        input.end();
      }
    }
  }
  line({ kind: "driver.generator_completed_normally" });
} catch (error) {
  loopThrew = String(error?.message ?? error);
  line({
    kind: "driver.error",
    stage,
    input_ended: inputEnded,
    input_was_open_when_thrown: input.isOpen,
    message: loopThrew,
  });
}

line({
  kind: "driver.summary",
  interrupt_called: interrupted,
  init_session_id: initSessionId,
  first_result: firstResult
    ? {
        subtype: firstResult.subtype ?? null,
        terminal_reason: firstResult.terminal_reason ?? null,
        is_error: firstResult.is_error ?? null,
      }
    : null,
  second_result: secondResult
    ? {
        subtype: secondResult.subtype ?? null,
        terminal_reason: secondResult.terminal_reason ?? null,
        is_error: secondResult.is_error ?? null,
      }
    : null,
  second_turn_same_session: secondResult
    ? (secondResult.session_id ?? null) === initSessionId
    : null,
  loop_threw_after_input_ended: loopThrew,
});
