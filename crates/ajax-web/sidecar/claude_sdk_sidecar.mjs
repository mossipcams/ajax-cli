// Claude Agent SDK sidecar speaking Pi-style JSONL framing.
//
// Protocol (same framing as the Pi RPC child, so the existing Rust
// `JsonlProcess` can drive this process unchanged):
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
// Permissions run in dangerous mode (permissionMode "bypassPermissions" +
// allowDangerouslySkipPermissions; canUseTool stays as an always-allow
// fallback) and MCP form elicitations are forwarded to the host as
// `elicitation_request` events answered by `elicitation_response` commands;
// url-mode and secret-looking elicitations are declined, and any elicitation
// still pending on abort/shutdown resolves as cancelled.
// Claude runs with the `claude_code` system prompt preset (systemPrompt
// {type:'preset', preset:'claude_code'}).

import { randomUUID } from "node:crypto";
import { argv, exit, stderr, stdin, stdout } from "node:process";
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
// SDK module loading. The sidecar never resolves a module itself: the host
// resolves the Claude Agent SDK from a trusted location and passes its absolute
// path as an explicit `--sdk-module`, which is exclusive: if it cannot load,
// init fails instead of silently loading anything else.

function flagValue(name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
}

async function resolveSdk() {
  const tried = [];
  const specs = [];
  const fromArg = flagValue("--sdk-module");
  if (fromArg) {
    // Explicit override: try only this module, never fall back to the SDK.
    specs.push({ spec: fromArg, via: "argv --sdk-module", file: true });
  } else {
    // The sidecar never resolves a module itself; the host resolves the SDK and passes it.
    return {
      mod: null,
      error:
        "no --sdk-module given; the host resolves the Claude Agent SDK and passes its absolute path",
    };
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
  inflightPrompts: 0,
  pendingElicitations: new Map(),
  planTasks: [],
  pendingTaskCreates: new Map(),
};

async function canUseTool(_toolName, input) {
  return { behavior: "allow", updatedInput: input };
}

// Property-name markers (compared against the name lowercased with `-` and
// `_` removed) that mark an elicitation schema as secret-sensitive.
const SECRET_MARKERS = ["password", "secret", "token", "apikey", "authcode", "otp"];

function normalisePropertyName(name) {
  return String(name).toLowerCase().replace(/[-_]/g, "");
}

function hasSecretProperty(schema) {
  const properties = schema?.properties;
  if (properties === null || typeof properties !== "object") return false;
  return Object.keys(properties).some((key) =>
    SECRET_MARKERS.some((marker) => normalisePropertyName(key).includes(marker)),
  );
}

async function forwardElicitation(message, schema) {
  const id = randomUUID();
  writeLine({ type: "elicitation_request", id, message, schema });
  return new Promise((resolve) => state.pendingElicitations.set(id, resolve));
}

async function onElicitation(request) {
  const serverName = request?.serverName ?? "unknown server";
  const schema = request?.requestedSchema;
  if (
    request?.mode === "url" ||
    schema === null ||
    typeof schema !== "object" ||
    hasSecretProperty(schema)
  ) {
    log(`declining elicitation from ${serverName}`);
    return { action: "decline" };
  }
  return forwardElicitation(request.message ?? "", schema);
}

// Issue #1358: under permissionMode "bypassPermissions" canUseTool is never
// consulted, so the SDK auto-answers AskUserQuestion ("The user did not
// answer the questions"). A PreToolUse hook intercepts it instead: it builds
// an elicitation form from the questions, forwards it through the SAME
// pendingElicitations path as MCP elicitations (so `elicitation_response`
// commands and cancelPendingElicitations on abort/shutdown already work),
// then injects the answers back into the tool input (deny when declined).

async function askUserQuestionHook(input) {
  const questions = Array.isArray(input?.tool_input?.questions) ? input.tool_input.questions : [];
  const properties = {};
  questions.forEach((question, index) => {
    if (!question || typeof question.question !== "string") return;
    const title =
      typeof question.header === "string" && question.header.length > 0 ? question.header : question.question;
    const options = Array.isArray(question.options) ? question.options : [];
    if (question.multiSelect) {
      const labels = options
        .map((option) => option?.label)
        .filter((label) => typeof label === "string");
      properties[`question_${index}`] = {
        type: "string",
        title,
        description: `${question.question} (comma-separated: ${labels.join(", ")})`,
      };
    } else {
      properties[`question_${index}`] = {
        type: "string",
        title,
        description: question.question,
        oneOf: options.map((option) => ({
          const: option?.label,
          title: option?.label,
          ...(typeof option?.description === "string" && option.description.length > 0
            ? { description: option.description }
            : {}),
        })),
      };
    }
    properties[`question_${index}_other`] = {
      type: "string",
      title: "Other",
      description: "Type your own answer (optional).",
    };
  });
  const single = questions.length === 1 ? questions[0] : null;
  const message =
    single && typeof single.question === "string"
      ? single.question
      : "Claude has questions for you";
  const answer = await forwardElicitation(message, { type: "object", properties, required: [] });
  if (answer?.action !== "accept") {
    return {
      hookSpecificOutput: {
        hookEventName: "PreToolUse",
        permissionDecision: "deny",
        permissionDecisionReason: "The user declined to answer.",
      },
    };
  }
  const content = answer.content ?? {};
  const answers = {};
  questions.forEach((question, index) => {
    if (!question || typeof question.question !== "string") return;
    const primary = String(content[`question_${index}`] ?? "").trim();
    if (primary.length > 0) {
      answers[question.question] = primary;
      return;
    }
    const other = String(content[`question_${index}_other`] ?? "").trim();
    if (other.length > 0) answers[question.question] = other;
  });
  return {
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "allow",
      updatedInput: { ...input.tool_input, answers },
    },
  };
}

function cancelPendingElicitations() {
  for (const resolve of state.pendingElicitations.values()) resolve({ action: "cancel" });
  state.pendingElicitations.clear();
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

// Issue #1274: a pump that throws — or an iterator that ends/exhausts — while
// a prompt is in flight must NOT leave the session busy and healthy-looking.
// Every lost turn is ended with a synthesized error `result` so the host
// surfaces it; the sidecar itself stays alive for subsequent prompts.
function reportLostTurn(message) {
  if (state.inflightPrompts === 0) return;
  state.inflightPrompts = 0;
  log(`turn lost: ${message}`);
  writeLine({
    type: "result",
    subtype: "error_during_execution",
    is_error: true,
    duration_ms: 0,
    num_turns: 1,
    result: message,
    session_id: state.sessionId,
  });
}

// Issue #1357 (slice A): the Claude Agent SDK hides its todo tools unless
// CLAUDE_CODE_ENABLE_TODO_TOOLS=1, and with them enabled Claude manages tasks
// via TaskCreate/TaskUpdate. The task ids only appear in the TaskCreate tool
// result, so track main-loop tasks here and mirror every change of the
// ordered list as an `ajax_plan` line AFTER forwarding the SDK message that
// caused it (slice B maps ajax_plan to the plan panel).
function emitPlan() {
  writeLine({
    type: "ajax_plan",
    entries: state.planTasks.map((task) => ({ content: task.subject, status: task.status })),
  });
}

function applyTaskUpdate(input) {
  const taskId = input?.taskId;
  if (typeof taskId !== "string") return;
  if (input.status === "deleted") {
    const next = state.planTasks.filter((task) => task.id !== taskId);
    if (next.length !== state.planTasks.length) {
      state.planTasks = next;
      emitPlan();
    }
    return;
  }
  const task = state.planTasks.find((candidate) => candidate.id === taskId);
  if (!task) return;
  let changed = false;
  if (typeof input.status === "string" && input.status !== task.status) {
    task.status = input.status;
    changed = true;
  }
  if (typeof input.subject === "string" && input.subject !== task.subject) {
    task.subject = input.subject;
    changed = true;
  }
  if (changed) emitPlan();
}

function trackPlanMessage(message) {
  // Main loop only: ignore sub-agent messages (parent_tool_use_id set).
  if (!message || message.parent_tool_use_id != null) return;
  const content = Array.isArray(message?.message?.content) ? message.message.content : [];
  if (message.type === "assistant") {
    for (const block of content) {
      if (block?.type !== "tool_use") continue;
      if (block.name === "TaskCreate" && typeof block.id === "string") {
        state.pendingTaskCreates.set(
          block.id,
          typeof block.input?.subject === "string" ? block.input.subject : "",
        );
      } else if (block.name === "TaskUpdate") {
        applyTaskUpdate(block.input);
      }
    }
  } else if (message.type === "user") {
    for (const block of content) {
      if (block?.type !== "tool_result" || typeof block.tool_use_id !== "string") continue;
      const subject = state.pendingTaskCreates.get(block.tool_use_id);
      if (subject === undefined) continue;
      state.pendingTaskCreates.delete(block.tool_use_id);
      const taskResult = message.tool_use_result?.task;
      const id = typeof taskResult?.id === "string" ? taskResult.id : null;
      if (id !== null && !state.planTasks.some((task) => task.id === id)) {
        state.planTasks.push({ id, subject, status: "pending" });
        emitPlan();
      }
    }
  }
}

function startPump(query) {
  return (async () => {
    try {
      for await (const message of query) {
        if (message?.type === "result") state.inflightPrompts = Math.max(0, state.inflightPrompts - 1);
        writeLine(message);
        trackPlanMessage(message);
      }
      if (!state.shuttingDown) reportLostTurn("Claude SDK message stream ended before the in-flight prompt completed");
    } catch (error) {
      log(`message pump failed: ${error?.stack ?? error}`);
      if (!state.shuttingDown) reportLostTurn(`Claude SDK message stream failed: ${error?.message ?? String(error)}`);
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
  // Resume fails closed: verify the session exists before resuming. A lookup
  // that throws or times out is NOT proof of absence; degrade and resume.
  if (typeof cmd.resume === "string" && cmd.resume && typeof mod.getSessionInfo === "function") {
    let notFound = false;
    try {
      let clearTimer = () => {};
      const timeout = new Promise((resolve) => {
        const handle = setTimeout(() => resolve("timeout"), OPTIONAL_CALL_TIMEOUT_MS);
        clearTimer = () => clearTimeout(handle);
      });
      const info = await Promise.race([Promise.resolve(mod.getSessionInfo(cmd.resume, { dir: cmd.cwd })), timeout]);
      clearTimer();
      notFound = info === undefined || info === null;
    } catch (error) {
      log(`resume session lookup failed for ${cmd.resume}: ${error?.message ?? error}`);
    }
    if (notFound) return fail("init", cmd.id, `session not found: ${cmd.resume}`);
  }
  const sessionId = cmd.resume || cmd.sessionId || randomUUID();
  const options = {
    cwd: cmd.cwd,
    systemPrompt: { type: "preset", preset: "claude_code" },
    includePartialMessages: true,
    permissionMode: "bypassPermissions",
    env: { ...process.env, CLAUDE_CODE_ENABLE_TODO_TOOLS: "1" },
    allowDangerouslySkipPermissions: true,
    canUseTool,
    onElicitation,
    hooks: { PreToolUse: [{ matcher: "AskUserQuestion", hooks: [askUserQuestionHook] }] },
  };
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
  if (cmd.images !== undefined) {
    if (
      !Array.isArray(cmd.images) ||
      cmd.images.some(
        (image) =>
          typeof image !== "object" ||
          image === null ||
          typeof image.data !== "string" ||
          image.data.length === 0 ||
          typeof image.mimeType !== "string" ||
          image.mimeType.length === 0,
      )
    ) {
      return fail("prompt", cmd.id, "prompt images must be {data, mimeType} objects");
    }
  }
  const content = [];
  if (cmd.message.length > 0 || !(cmd.images && cmd.images.length > 0)) {
    content.push({ type: "text", text: cmd.message });
  }
  for (const image of cmd.images ?? []) {
    content.push({
      type: "image",
      source: { type: "base64", media_type: image.mimeType, data: image.data },
    });
  }
  state.input.push({
    type: "user",
    message: { role: "user", content },
    parent_tool_use_id: null,
    session_id: state.sessionId,
  });
  state.inflightPrompts += 1;
  respond("prompt", cmd.id, true, { disposition: "started" });
}

async function handleAbort(cmd) {
  if (!state.initialized) return fail("abort", cmd.id, "not initialized");
  cancelPendingElicitations();
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
  cancelPendingElicitations();
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

async function handleElicitationResponse(cmd) {
  const action = cmd?.action;
  if (
    typeof cmd?.id !== "string" ||
    !state.pendingElicitations.has(cmd.id) ||
    (action !== "accept" && action !== "decline" && action !== "cancel")
  ) {
    return fail("elicitation_response", cmd.id, "unknown or invalid elicitation answer");
  }
  const resolve = state.pendingElicitations.get(cmd.id);
  state.pendingElicitations.delete(cmd.id);
  resolve(
    action === "accept" && cmd.content && typeof cmd.content === "object"
      ? { action, content: cmd.content }
      : { action },
  );
  respond("elicitation_response", cmd.id, true, null);
}

const handlers = {
  init: handleInit,
  prompt: handlePrompt,
  abort: handleAbort,
  set_model: handleSetModel,
  set_effort: handleSetEffort,
  get_context_usage: handleGetContextUsage,
  shutdown: handleShutdown,
  elicitation_response: handleElicitationResponse,
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
