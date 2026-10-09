// Claude Agent SDK sidecar speaking Pi-style JSONL framing.
//
// Protocol (same framing as the Pi RPC child, so the existing Rust
// `PiRpcProcess` can drive this process unchanged):
//   stdin:  one JSON command per LF-terminated line: {"id", "type", ...}.
//           Lines are split on "\n" ONLY (U+2028/U+2029 are NOT separators);
//           one optional preceding "\r" is stripped. A line that is not valid
//           JSON is answered with {"type":"response","command":"parse",
//           "success":false,"error":...} and no id.
//   stdout: (a) responses {"id","type":"response","command","success":true,
//               "data":{...}} or {"id",...,"success":false,"error":"text"};
//           (b) events: every SDK message the query yields, forwarded
//               verbatim as one JSON line each (their own `type`: system,
//               stream_event, assistant, user, result, ...).
//   stderr: diagnostics only. Never write non-protocol text to stdout.
//
// Commands: init {cwd, model?, resume?, sessionId?, settingSources?} (must be
// first), prompt {message}, abort, set_model {model}, set_effort {level},
// get_context_usage, shutdown. stdin EOF behaves like shutdown (no response).
// Permissions auto-approve locally (canUseTool always allows; no
// bypassPermissions flag) and MCP elicitation is declined.

import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { argv, env, exit, stderr, stdin, stdout } from "node:process";
import { pathToFileURL } from "node:url";

const OPTIONAL_CALL_TIMEOUT_MS = 15_000;
const SHUTDOWN_PUMP_TIMEOUT_MS = 5_000;
const DRAIN_FALLBACK_MS = 2_000;

// ---------------------------------------------------------------------------
// Output plumbing: one JSON object per line, and exit only after draining.

let pendingWrites = 0;
let wantExit = false;

function writeLine(value) {
  pendingWrites += 1;
  stdout.write(JSON.stringify(value) + "\n", () => {
    pendingWrites -= 1;
    if (wantExit && pendingWrites === 0) exit(0);
  });
}

function log(text) {
  stderr.write(`claude-sdk-sidecar: ${text}\n`);
}

function respond(command, id, success, payload) {
  const response = { type: "response", command, success };
  if (id !== undefined && id !== null) response.id = id;
  if (success) response.data = payload;
  else response.error = String(payload);
  writeLine(response);
}

function fail(command, id, message) {
  respond(command, id, false, message);
}

// ---------------------------------------------------------------------------
// Long-lived streaming input queue (stays OPEN until end()).

function makeInput() {
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

// ---------------------------------------------------------------------------
// SDK module resolution: 1) --sdk-module argv / env, 2) package import,
// 3) + 4) npm root -g locations.

function flagValue(name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
}

async function resolveSdk() {
  const tried = [];
  const specs = [];
  const fromArg = flagValue("--sdk-module");
  if (fromArg) specs.push({ spec: fromArg, via: "argv --sdk-module", file: true });
  if (env.AJAX_CLAUDE_SDK_MODULE) {
    specs.push({ spec: env.AJAX_CLAUDE_SDK_MODULE, via: "env AJAX_CLAUDE_SDK_MODULE", file: true });
  }
  specs.push({ spec: "@anthropic-ai/claude-agent-sdk", via: "package import", file: false });
  try {
    const root = execFileSync("npm", ["root", "-g"], { encoding: "utf8" }).trim();
    specs.push({ spec: `${root}/@anthropic-ai/claude-agent-sdk/sdk.mjs`, via: "npm root -g", file: true });
    specs.push({
      spec: `${root}/@agentclientprotocol/claude-agent-acp/node_modules/@anthropic-ai/claude-agent-sdk/sdk.mjs`,
      via: "npm root -g (acp)",
      file: true,
    });
  } catch (error) {
    log(`npm root -g failed: ${error?.message ?? error}`);
  }
  for (const { spec, via, file } of specs) {
    const label = `${spec} (${via})`;
    tried.push(label);
    try {
      const mod = await import(file ? pathToFileURL(spec).href : spec);
      if (typeof mod?.query === "function") return { mod, error: null };
      tried[tried.length - 1] = `${label} [no query export]`;
    } catch (error) {
      log(`sdk candidate failed (${label}): ${error?.message ?? error}`);
    }
  }
  return { mod: null, error: `no loadable claude-agent-sdk module; tried: ${tried.join("; ")}` };
}

// ---------------------------------------------------------------------------
// Session state, permissions, and the message pump.

const state = {
  initialized: false,
  sessionId: null,
  input: null,
  query: null,
  pump: null,
  shuttingDown: false,
};

async function canUseTool(toolName, input) {
  return { behavior: "allow", updatedInput: input };
}

async function onElicitation(request) {
  log(`declining elicitation from ${request?.serverName ?? "unknown server"}`);
  return { action: "decline" };
}

async function optional(fn, fallback) {
  const timer = new Promise((_, reject) => {
    const handle = setTimeout(() => reject(new Error("timed out")), OPTIONAL_CALL_TIMEOUT_MS);
    handle.unref();
  });
  try {
    return await Promise.race([Promise.resolve(fn()), timer]);
  } catch (error) {
    log(`optional query call failed: ${error?.message ?? error}`);
    return fallback;
  }
}

function startPump(query) {
  return (async () => {
    try {
      for await (const message of query) writeLine(message);
    } catch (error) {
      log(`message pump failed: ${error?.stack ?? error}`);
    }
  })();
}

// ---------------------------------------------------------------------------
// Command handlers.

async function handleInit(cmd) {
  if (state.initialized) return fail("init", cmd.id, "already initialized");
  if (typeof cmd.cwd !== "string" || !cmd.cwd) {
    return fail("init", cmd.id, "init requires a cwd string");
  }
  const { mod, error } = await resolveSdk();
  if (!mod) return fail("init", cmd.id, error);
  const sessionId = cmd.resume || cmd.sessionId || randomUUID();
  const options = { cwd: cmd.cwd, includePartialMessages: true, canUseTool, onElicitation };
  if (cmd.model) options.model = cmd.model;
  if (cmd.resume) options.resume = cmd.resume;
  else options.sessionId = sessionId;
  if (cmd.settingSources) options.settingSources = cmd.settingSources;
  state.input = makeInput();
  state.query = mod.query({ prompt: state.input, options });
  state.sessionId = sessionId;
  state.initialized = true;
  state.pump = startPump(state.query);
  const models = await optional(() => state.query.supportedModels(), []);
  const commands = await optional(() => state.query.supportedCommands(), []);
  const initialization = await optional(() => state.query.initializationResult(), null);
  respond("init", cmd.id, true, { sessionId, models, commands, initialization });
}

function handlePrompt(cmd) {
  if (!state.initialized) return fail("prompt", cmd.id, "not initialized");
  if (typeof cmd.message !== "string") {
    return fail("prompt", cmd.id, "prompt requires a message string");
  }
  state.input.push({
    type: "user",
    message: { role: "user", content: [{ type: "text", text: cmd.message }] },
    parent_tool_use_id: null,
    session_id: state.sessionId,
  });
  respond("prompt", cmd.id, true, { disposition: "started" });
}

async function handleAbort(cmd) {
  if (!state.initialized) return fail("abort", cmd.id, "not initialized");
  const result = await state.query.interrupt();
  respond("abort", cmd.id, true, result ?? null);
}

async function handleSetModel(cmd) {
  if (!state.initialized) return fail("set_model", cmd.id, "not initialized");
  if (typeof cmd.model !== "string") {
    return fail("set_model", cmd.id, "set_model requires a model string");
  }
  await state.query.setModel(cmd.model);
  respond("set_model", cmd.id, true, null);
}

async function handleSetEffort(cmd) {
  if (!state.initialized) return fail("set_effort", cmd.id, "not initialized");
  if (typeof cmd.level !== "string") {
    return fail("set_effort", cmd.id, "set_effort requires a level string");
  }
  await state.query.applyFlagSettings({ effortLevel: cmd.level });
  respond("set_effort", cmd.id, true, null);
}

async function handleGetContextUsage(cmd) {
  if (!state.initialized) return fail("get_context_usage", cmd.id, "not initialized");
  const usage = await state.query.getContextUsage({ detail: "summary" });
  respond("get_context_usage", cmd.id, true, usage);
}

async function handleShutdown(cmd) {
  respond("shutdown", cmd.id, true, null);
  await shutdown();
}

async function shutdown() {
  if (state.shuttingDown) return;
  state.shuttingDown = true;
  try {
    state.input?.end();
  } catch (error) {
    log(`input end failed: ${error?.message ?? error}`);
  }
  if (state.pump) {
    await Promise.race([
      state.pump,
      new Promise((resolve) => {
        const handle = setTimeout(resolve, SHUTDOWN_PUMP_TIMEOUT_MS);
        handle.unref();
      }),
    ]);
  }
  try {
    state.query?.close?.();
  } catch (error) {
    log(`query close failed: ${error?.message ?? error}`);
  }
  wantExit = true;
  if (pendingWrites === 0) exit(0);
  const fallback = setTimeout(() => exit(0), DRAIN_FALLBACK_MS);
  fallback.unref();
}

const handlers = {
  init: handleInit,
  prompt: handlePrompt,
  abort: handleAbort,
  set_model: handleSetModel,
  set_effort: handleSetEffort,
  get_context_usage: handleGetContextUsage,
  shutdown: handleShutdown,
};

function dispatch(cmd) {
  const type = cmd?.type;
  const handler = typeof type === "string" ? handlers[type] : undefined;
  if (!handler) {
    respond(
      typeof type === "string" ? type : "unknown",
      cmd?.id ?? null,
      false,
      `unknown command type: ${JSON.stringify(type)}`,
    );
    return;
  }
  Promise.resolve(handler(cmd)).catch((error) => {
    log(`command ${type} failed: ${error?.stack ?? error}`);
    fail(type, cmd?.id ?? null, error?.message ?? String(error));
  });
}

// ---------------------------------------------------------------------------
// stdin: LF-only line splitting (never U+2028/U+2029).

function handleLine(rawLine) {
  let line = rawLine;
  if (line.endsWith("\r")) line = line.slice(0, -1);
  if (line.length === 0) return;
  let cmd;
  try {
    cmd = JSON.parse(line);
  } catch (error) {
    respond("parse", null, false, `invalid JSON command line: ${error?.message ?? error}`);
    return;
  }
  dispatch(cmd);
}

let buffer = "";
stdin.setEncoding("utf8");
stdin.on("data", (chunk) => {
  buffer += chunk;
  let newline = buffer.indexOf("\n");
  while (newline >= 0) {
    const line = buffer.slice(0, newline);
    buffer = buffer.slice(newline + 1);
    handleLine(line);
    newline = buffer.indexOf("\n");
  }
});
stdin.on("end", () => {
  void shutdown();
});
stdin.on("error", (error) => {
  log(`stdin error: ${error?.message ?? error}`);
  void shutdown();
});
