// Cursor SDK sidecar speaking the same JSONL framing as the Claude Agent SDK
// sidecar, so the existing Rust `JsonlProcess` can drive this process.
//
// Protocol:
//   stdin:  one JSON command per LF-terminated line: {"id", "type", ...}. Lines
//           are split on "\n" ONLY (U+2028/U+2029 are NOT separators); one
//           optional preceding "\r" is stripped. Invalid JSON is answered with
//           {"type":"response","command":"parse","success":false,"error":...}
//           and no id.
//   stdout: (a) responses {"id","type":"response","command","success":true,
//               "data":{...}} or {...,"success":false,"error":"text"};
//           (b) events: every run stream message forwarded verbatim as one
//               JSON line each (their own `type`: status, thinking, assistant,
//               tool_call, usage, task, system, user, request); and (c)
//               exactly ONE terminal line per prompt:
//               {"type":"result","status":"finished"|"error"|"cancelled",
//                "error":<string, only when present>,
//                "usage":<object, only when present>}.
//   stderr: diagnostics only. The SDK logs to console; before the SDK module
//           is imported, console.log/info/warn/debug are repointed at stderr so
//           nothing non-JSON can reach stdout. stdout carries protocol lines
//           only. The sidecar resolves the SDK credential itself (CURSOR_API_KEY,
//           else ~/.cursor/sdk/auth.json) and keeps it in memory solely to pass
//           to SDK calls; key material is never logged, printed, or included in
//           responses.
//
// Commands: init {cwd, model:{id,params?}, mode?, resume?, settingSources?} (must be
// first), prompt {message, images?: [{url}] or [{data,mimeType}] (base64)},
// abort, set_model {model:{id,params?}}, set_mode {mode:"agent"|"plan"},
// shutdown (cancels an active run before waiting on the pump); stdin EOF behaves
// like shutdown (no response). Any command before init other
// than init is answered success:false. With `resume`, Agent.resume(id, ...)
// is called with the model ALWAYS passed (a resumed agent has no model); a
// throwing resume answers success:false and NEVER falls back to Agent.create.
// Without it, Agent.create({local:{cwd,settingSources},model}) is used;
// settingSources defaults to ["project","user"]. init data: {sessionId,
// models (Cursor.models.list(), or [] if it throws/exceeds 15s), commands
// (scanned .cursor/commands/*.md slash commands), cursorCredential (a
// non-secret status of where the credential came from: "env" for the --api-key
// argument, "store" for the protected store; never key material)}. prompt is busy while a run
// is active; slash commands expand Ajax-side (leading "/name" replaced by the
// scanned file's text read at prompt time, remainder appended after "\n\n",
// unknown names unchanged, no templating or nesting). send/stream failures
// yield {"type":"result","status":"error","error":<text>}; abort calls
// run.cancel() (or cancels as soon as a late send resolves) and the terminal
// line always comes from the prompt pump.

import { argv, exit, stderr, stdin, stdout } from "node:process";
import { pathToFileURL } from "node:url";
import { readFileSync, readdirSync, mkdirSync, writeFileSync, renameSync, unlinkSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

// True when run as a script (`node cursor_sdk_sidecar.mjs`), false when the
// module is imported by tests for the exported credential reader.
const IS_MAIN = typeof process.argv[1] === "string" && process.argv[1].length > 0 &&
  import.meta.url === pathToFileURL(process.argv[1]).href;

if (IS_MAIN) {
  // The SDK logs to console; redirect BEFORE importing it (see header).
  for (const level of ["log", "info", "warn", "debug"]) {
    console[level] = (...args) => stderr.write(args.map(String).join(" ") + "\n");
  }
}

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
  stderr.write(`cursor-sdk-sidecar: ${text}\n`);
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
// SDK module loading. The sidecar never resolves a module itself: the host
// resolves @cursor/sdk from a trusted location and passes its absolute path as
// an explicit `--sdk-module`, exactly like the Claude sidecar: if it cannot
// load, init fails instead of silently loading anything else.

function flagValue(name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
}

/** Parse `--api-key` from an argv vector (exported for tests). */
export function parseApiKeyArg(argvList) {
  const index = argvList.indexOf("--api-key");
  const value = index >= 0 ? argvList[index + 1] : undefined;
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function argApiKey() {
  return parseApiKeyArg(argv);
}

async function resolveSdk() {
  const tried = [];
  const fromArg = flagValue("--sdk-module");
  if (!fromArg) {
    return {
      mod: null,
      error: "no --sdk-module given; the host resolves @cursor/sdk and passes its absolute path",
    };
  }
  const label = `${fromArg} (argv --sdk-module)`;
  tried.push(label);
  try {
    const mod = await import(pathToFileURL(fromArg).href);
    if (mod?.Agent && mod?.Cursor) return { mod, error: null };
    tried[tried.length - 1] = `${label} [no Agent/Cursor exports]`;
  } catch (error) {
    log(`sdk candidate failed (${label}): ${error?.message ?? error}`);
  }
  return { mod: null, error: `no loadable @cursor/sdk module; tried: ${tried.join("; ")}` };
}

// ---------------------------------------------------------------------------
// Cursor SDK credential resolution. Prefers the CURSOR_API_KEY environment
// variable, otherwise reads the protected credential store written by
// `Cursor.auth.login()` at ~/.cursor/sdk/auth.json (renewed every 90 days;
// see docs/architecture/web-cockpit.md). The resolved key is returned ONLY so
// it can be passed to an SDK call; reader failures are typed
// CursorCredentialErrors whose messages are static text and therefore can
// never contain key material. The store file's shape is read only as far as
// the fields we need: an apiKey, plus (optionally) an expiry field.

export class CursorCredentialError extends Error {
  constructor(code, message) {
    super(message);
    this.name = "CursorCredentialError";
    this.code = code; // "missing" | "expired"
  }
}

// Tolerates only the expiry field spellings we must read; no expiry field at
// all is tolerated (the credential stands until renewed externally).
function storeExpiryMs(store) {
  for (const field of ["expiresAt", "expires_at", "expiredAt", "expiry"]) {
    const value = store?.[field];
    if (value === undefined || value === null) continue;
    let ms;
    if (typeof value === "number" && Number.isFinite(value)) {
      ms = value < 1e12 ? value * 1000 : value; // epoch seconds or milliseconds
    } else if (typeof value === "string") {
      const parsed = Date.parse(value);
      if (!Number.isNaN(parsed)) ms = parsed;
    }
    if (ms !== undefined) return ms;
  }
  return null;
}

export function resolveCursorCredential({ env = process.env, storePath, now = Date.now } = {}) {
  const fromEnv = env?.CURSOR_API_KEY;
  if (typeof fromEnv === "string" && fromEnv.length > 0) return fromEnv; // env wins
  const path = storePath ?? join(homedir(), ".cursor", "sdk", "auth.json");
  let raw;
  try {
    raw = readFileSync(path, "utf8");
  } catch {
    throw new CursorCredentialError("missing",
      `no Cursor SDK credential found: set CURSOR_API_KEY or run Cursor.auth.login() to write ${path}`);
  }
  let store;
  try {
    store = JSON.parse(raw);
  } catch {
    throw new CursorCredentialError("missing", `Cursor SDK credential store at ${path} is unreadable`);
  }
  const key = typeof store?.apiKey === "string" ? store.apiKey : "";
  if (key.length === 0) {
    throw new CursorCredentialError("missing", `Cursor SDK credential store at ${path} has no API key`);
  }
  const expiresAtMs = storeExpiryMs(store);
  if (expiresAtMs !== null && now() >= expiresAtMs) {
    throw new CursorCredentialError("expired",
      "the stored Cursor SDK credential is expired; re-run Cursor.auth.login()");
  }
  return key;
}

/** Host argv key first; otherwise the protected credential store (not child env). */
export function resolveSidecarCredential() {
  const fromArgv = IS_MAIN ? argApiKey() : undefined;
  if (typeof fromArgv === "string" && fromArgv.length > 0) return fromArgv;
  return resolveCursorCredential({ env: {} });
}

// ---------------------------------------------------------------------------
// Cursor SDK host-side sign-in (slice 3). auth-signin / auth-renew run
// `Cursor.auth.login({ openBrowser: false })`, surface the login URL as an
// `auth_url` event line, hold the returned credential in a module local ONLY
// (never on state, never in responses or logs), verify it with `Cursor.me()`,
// and atomically write it to the protected store (temp file + rename, 0600).
// Cancellation, timeout, verification failure, or a store-write failure NEVER
// touches the existing store file; failures are typed and redacted: static
// text naming which stage failed, never an SDK error string. auth-status and
// auth-signout expose presence/label/expiry only — no key material.

class AuthFlowError extends Error {
  constructor(stage, message) {
    super(message);
    this.name = "AuthFlowError";
    this.stage = stage; // "login" | "verify" | "store"
  }
}

export function protectedStorePath() {
  return join(homedir(), ".cursor", "sdk", "auth.json");
}

// The in-memory credential minted by login, before verification and the store
// write. Module local on purpose: it must not sit on `state` (which is close
// to the wire) or reach any respond/fail/log payload.
let pendingAuthKey = null;

function authCommandName(cmd) {
  // `type` is the wire command name; the response must carry the same one.
  return cmd.type === "auth-renew" ? "auth-renew" : "auth-signin";
}

// Map a throwing login() to a redacted, typed stage message. The SDK's own
// error text is deliberately never forwarded: it could carry URL-embedded key
// material.
function authLoginFailureText(loginError) {
  const detail = String(loginError?.name ?? "") + " " + String(loginError?.message ?? loginError);
  if (/abort/i.test(detail)) return 'auth-signin failed at stage "login": the browser sign-in was cancelled';
  if (/tim(e|ed)[\s-]*out/i.test(detail)) return 'auth-signin failed at stage "login": the browser sign-in timed out';
  return 'auth-signin failed at stage "login": the SDK login call failed';
}

function sanitizedAccountStatus(credential, account) {
  // Only non-secret label/expiry fields; key material is filtered by name AND
  // by value (the live key can never appear verbatim).
  const data = {};
  for (const [source] of [[account], [credential]]) {
    if (data.accountLabel === undefined) {
      for (const field of ["email", "login", "username", "name"]) {
        const value = source?.[field];
        if (typeof value === "string" && value.length > 0) {
          data.accountLabel = value;
          break;
        }
      }
    }
    if (data.expiresAt === undefined) {
      for (const field of ["expiresAt", "expires_at", "expiredAt", "expiry"]) {
        const value = source?.[field];
        if (value !== undefined && value !== null && String(value) !== pendingAuthKey) {
          data.expiresAt = value;
          break;
        }
      }
    }
  }
  return data;
}

async function atomicWriteCredentialStore(store, commandName) {
  const path = protectedStorePath();
  try {
    mkdirSync(dirname(path), { recursive: true });
    const temp = `${path}.${process.pid}.tmp`;
    try {
      writeFileSync(temp, JSON.stringify(store), { mode: 0o600 });
      renameSync(temp, path); // atomic replace; the prior file is never truncated
    } catch {
      try { unlinkSync(temp); } catch { /* no partial file may be left behind */ }
      throw new AuthFlowError("store", `${commandName} failed at stage "store": writing the credential store failed`);
    }
  } catch (error) {
    if (error instanceof AuthFlowError) throw error;
    throw new AuthFlowError("store", `${commandName} failed at stage "store": writing the credential store failed`);
  }
}

async function handleAuthSignin(cmd) {
  const commandName = authCommandName(cmd);
  const { mod, error } = await resolveSdk();
  if (!mod) return fail(commandName, cmd.id, `${commandName} failed at stage "sdk": ${error}`);
  if (typeof mod.Cursor?.auth?.login !== "function") {
    return fail(commandName, cmd.id, `${commandName} failed at stage "login": the loaded SDK exposes no Cursor.auth.login`);
  }
  let loginResult;
  try {
    // The URL is surfaced to the operator as an event line the moment it
    // appears; it carries no credential. Browser opening stays disabled: the
    // host is headless and the operator opens the URL themselves.
    loginResult = await mod.Cursor.auth.login({
      openBrowser: false,
      onLoginUrl: (url) => {
        if (typeof url === "string" && url.length > 0) writeLine({ type: "auth_url", url });
      },
    });
  } catch (loginError) {
    // Cancellation / timeout / rejection: the prior credential is untouched.
    return fail(commandName, cmd.id, authLoginFailureText(loginError));
  }
  const key = typeof loginResult?.apiKey === "string" ? loginResult.apiKey : "";
  if (key.length === 0) {
    return fail(commandName, cmd.id, `${commandName} failed at stage "login": the SDK login returned no API key`);
  }
  pendingAuthKey = key; // module local only; cleared on every failure below
  let account;
  try {
    if (typeof mod.Cursor?.me !== "function") {
      throw new AuthFlowError("verify", "no verifier");
    }
    account = await mod.Cursor.me({ apiKey: key }); // verify the minted credential
  } catch {
    pendingAuthKey = null;
    return fail(commandName, cmd.id, `${commandName} failed at stage "verify": the returned credential was rejected by Cursor`);
  }
  const store = { apiKey: key };
  for (const field of ["expiresAt", "expires_at", "expiredAt", "expiry"]) {
    if (loginResult[field] !== undefined && loginResult[field] !== null) store[field] = loginResult[field];
  }
  try {
    await atomicWriteCredentialStore(store, commandName);
  } catch (storeError) {
    pendingAuthKey = null;
    return fail(commandName, cmd.id, String(storeError?.message ?? "writing the credential store failed"));
  }
  pendingAuthKey = null; // never held past the successful write
  respond(commandName, cmd.id, true, { status: "signed-in", ...sanitizedAccountStatus(loginResult, account) });
}

async function handleAuthStatus(cmd) {
  // Reuse the exact credential-reader path (CURSOR_API_KEY first, then the
  // protected store); reply presence + non-secret fields only. The key is
  // consumed and dropped: it is never echoed anywhere.
  try {
    resolveCursorCredential({});
  } catch (credentialError) {
    if (!(credentialError instanceof CursorCredentialError)) throw credentialError;
    return respond("auth-status", cmd.id, true, { present: false });
  }
  let data = { present: true };
  const storePath = protectedStorePath();
  try {
    const store = JSON.parse(readFileSync(storePath, "utf8"));
    const key = typeof store?.apiKey === "string" ? store.apiKey : null;
    for (const field of ["email", "login", "username", "name"]) {
      const value = store?.[field];
      if (typeof value === "string" && value.length > 0 && key !== null && value !== key) {
        data.accountLabel = value;
        break;
      }
    }
    for (const field of ["expiresAt", "expires_at", "expiredAt", "expiry"]) {
      const value = store?.[field];
      if (value !== undefined && value !== null && String(value) !== key) {
        data.expiresAt = value;
        break;
      }
    }
  } catch { // env/argv credential, or an unreadable store: presence only
  }
  respond("auth-status", cmd.id, true, data);
}

async function handleAuthSignout(cmd) {
  const path = protectedStorePath();
  try {
    unlinkSync(path); // only the protected store path is ever removed
  } catch { // no store file (or an env credential): sign-out is a success no-op
  }
  respond("auth-signout", cmd.id, true, { signedOut: true });
}


// ---------------------------------------------------------------------------
// Session state and the per-prompt run pump.

const state = {
  initialized: false,
  apiKey: undefined, // slice 3: resolved at init, consumed ONLY by SDK calls; never in respond/fail/log payloads
  agent: null,
  model: null,
  activeRun: false,
  abortRequested: false,
  mode: undefined, // "agent" | "plan"; omitted from send options when undefined
  run: null,
  pump: null,
  commands: {}, // name -> { file, description } from .cursor/commands scanning
  shuttingDown: false,
};

function startPump(run) {
  return (async () => {
    let terminal;
    try {
      for await (const message of run.stream()) writeLine(message);
      const done = await run.wait();
      terminal = { type: "result", status: done?.status ?? "error" };
      if (done?.error?.message) terminal.error = done.error.message;
      if (done?.usage !== undefined) terminal.usage = done.usage;
    } catch (error) {
      log(`run pump failed: ${error?.stack ?? error}`);
      terminal = { type: "result", status: "error", error: String(error?.message ?? error) };
    }
    writeLine(terminal);
  })();
}

// ---------------------------------------------------------------------------
// Slash-command scanning and expansion (Ajax-side, plain text only).

function scanCommands(cwd) {
  const merged = {};
  // Project commands win over user commands with the same name.
  for (const dir of [join(cwd, ".cursor", "commands"), join(homedir(), ".cursor", "commands")]) {
    let entries;
    try {
      entries = readdirSync(dir);
    } catch {
      continue; // missing directories are not an error
    }
    for (const entry of entries) {
      if (!entry.endsWith(".md")) continue;
      const name = entry.slice(0, -3);
      const file = join(dir, entry);
      let description = "";
      try {
        for (const line of readFileSync(file, "utf8").split("\n")) {
          if (line.trim().length > 0) {
            description = line.replace(/^#+\s*/, "").trim().slice(0, 120);
            break;
          }
        }
      } catch {
        // unreadable file: keep the entry with an empty description
      }
      if (!Object.prototype.hasOwnProperty.call(merged, name)) merged[name] = { file, description };
    }
  }
  return merged;
}

function expandSlash(message) {
  const match = /^\/([A-Za-z0-9_-]+)(\s|$)/.exec(message);
  if (!match) return message;
  const entry = state.commands[match[1]];
  if (!entry) return message;
  let text;
  try {
    text = readFileSync(entry.file, "utf8").trim(); // read at prompt time
  } catch (error) {
    log(`command file read failed for /${match[1]}: ${error?.message ?? error}`);
    return message;
  }
  const remainder = message.slice(match[0].length).trim();
  return remainder ? `${text}\n\n${remainder}` : text;
}

// ---------------------------------------------------------------------------
// Command handlers.

async function handleInit(cmd) {
  if (state.initialized) return fail("init", cmd.id, "already initialized");
  if (typeof cmd.cwd !== "string" || !cmd.cwd) {
    return fail("init", cmd.id, "init requires a cwd string");
  }
  if (cmd.mode !== undefined && cmd.mode !== "agent" && cmd.mode !== "plan") {
    return fail("init", cmd.id, 'mode must be "agent" or "plan"');
  }
  state.mode = cmd.mode; // default stays undefined
  const { mod, error } = await resolveSdk();
  if (!mod) return fail("init", cmd.id, error);
  let apiKey;
  try {
    apiKey = resolveSidecarCredential();
  } catch (credentialError) {
    if (credentialError instanceof CursorCredentialError) {
      return fail("init", cmd.id, credentialError.message);
    }
    throw credentialError;
  }
  state.apiKey = apiKey;
  // Non-secret credential-source status for the web picker; never key material.
  const cursorCredential = argApiKey() ? "env" : "store";
  state.commands = scanCommands(cmd.cwd);
  const settingSources = Array.isArray(cmd.settingSources) && cmd.settingSources.length > 0 ? cmd.settingSources : ["project", "user"];
  const model = cmd.model;
  let agent;
  if (cmd.resume) {
    // Fails closed: a throwing resume is reported, never retried as create.
    try {
      // A resumed agent has no model of its own; the model is ALWAYS passed.
      agent = await mod.Agent.resume(cmd.resume, { local: { cwd: cmd.cwd, settingSources }, model, apiKey });
    } catch (resumeError) {
      return fail("init", cmd.id, String(resumeError?.message ?? resumeError));
    }
  } else {
    try {
      agent = await mod.Agent.create({ local: { cwd: cmd.cwd, settingSources }, model, apiKey });
    } catch (createError) {
      return fail("init", cmd.id, String(createError?.message ?? createError));
    }
  }
  state.agent = agent;
  state.model = model;
  state.initialized = true;
  let models = [];
  try {
    models = await Promise.race([
      mod.Cursor.models.list({ apiKey }), // CursorRequestOptions.apiKey per @cursor/sdk types
      new Promise((resolve) => {
        const handle = setTimeout(() => resolve(null), OPTIONAL_CALL_TIMEOUT_MS);
        handle.unref();
      }),
    ]);
    if (!Array.isArray(models)) models = [];
  } catch (listError) {
    log(`models list failed: ${listError?.message ?? listError}`);
    models = [];
  }
  const commands = Object.entries(state.commands).map(([name, entry]) => ({ name, description: entry.description }));
  respond("init", cmd.id, true, { sessionId: agent.agentId, models, commands, cursorCredential });
}

function handlePrompt(cmd) {
  if (!state.initialized) return fail("prompt", cmd.id, "not initialized");
  if (typeof cmd.message !== "string") {
    return fail("prompt", cmd.id, "prompt requires a message string");
  }
  if (state.activeRun) return fail("prompt", cmd.id, "busy");
  let images;
  if (cmd.images !== undefined) {
    images = cmd.images;
    const valid = Array.isArray(images) &&
      images.every((image) =>
        typeof image === "object" && image !== null &&
        ((typeof image.url === "string" && Object.keys(image).length === 1) ||
          (typeof image.data === "string" && typeof image.mimeType === "string" && Object.keys(image).length === 2)),
      );
    if (!valid) {
      return fail("prompt", cmd.id, "prompt images must be {url} or {data,mimeType}");
    }
  }
  const options = { model: state.model };
  if (state.mode !== undefined) options.mode = state.mode; // omitted when undefined
  // No apiKey here on purpose: @cursor/sdk SendOptions carries no credential; the
  // key is bound to the agent at Agent.create/resume time, so every send inherits it.
  const text = expandSlash(cmd.message);
  state.abortRequested = false;
  let run;
  try {
    // send() does not throw on quota/provider failures; those arrive as
    // stream status messages and a "error" wait() status. Only transport- or
    // setup-level failures reach the catch below.
    run = Promise.resolve(images && images.length > 0
      ? state.agent.send({ text, images }, options)
      : state.agent.send(text, options));
  } catch (sendError) {
    writeLine({ type: "result", status: "error", error: String(sendError?.message ?? sendError) });
    return fail("prompt", cmd.id, String(sendError?.message ?? sendError));
  }
  state.activeRun = true;
  respond("prompt", cmd.id, true, { disposition: "started" });
  run
    .then((r) => {
      if (!state.activeRun) return; // shut down mid-flight; nothing to pump
      if (state.abortRequested) {
        // Abort raced ahead of send() resolving; cancel the fresh run now.
        r.cancel().catch((cancelError) => log(`late abort cancel failed: ${cancelError?.message ?? cancelError}`));
      }
      state.run = r;
      state.pump = startPump(r);
      return state.pump;
    })
    .catch((sendError) => {
      log(`agent.send failed: ${sendError?.stack ?? sendError}`);
      writeLine({ type: "result", status: "error", error: String(sendError?.message ?? sendError) });
    })
    .finally(() => {
      state.activeRun = false;
      state.run = null;
    });
}

async function handleAbort(cmd) {
  if (!state.initialized) return fail("abort", cmd.id, "not initialized");
  if (state.activeRun) {
    // Cancel as soon as the run exists; a run that resolves late is cancelled
    // in the send .then() via state.abortRequested. No active run is a no-op
    // that still succeeds.
    state.abortRequested = true;
    if (state.run) {
      try {
        await state.run.cancel();
      } catch (cancelError) {
        log(`run cancel failed: ${cancelError?.message ?? cancelError}`);
      }
    }
  }
  respond("abort", cmd.id, true, null);
}

async function handleSetModel(cmd) {
  if (!state.initialized) return fail("set_model", cmd.id, "not initialized");
  if (typeof cmd.model !== "object" || cmd.model === null || typeof cmd.model.id !== "string") {
    return fail("set_model", cmd.id, "set_model requires a model object with an id string");
  }
  state.model = cmd.model; // used by the next agent.send
  respond("set_model", cmd.id, true, null);
}

function handleSetMode(cmd) {
  if (!state.initialized) return fail("set_mode", cmd.id, "not initialized");
  if (cmd.mode !== "agent" && cmd.mode !== "plan") {
    return fail("set_mode", cmd.id, 'mode must be "agent" or "plan"');
  }
  state.mode = cmd.mode; // used by the next agent.send
  respond("set_mode", cmd.id, true, null);
}

async function handleShutdown(cmd) {
  respond("shutdown", cmd.id, true, null);
  await shutdown();
}

async function shutdown() {
  if (state.shuttingDown) return;
  state.shuttingDown = true;
  if (state.run) {
    try {
      // Cancel the active run so the pump reaches its terminal line fast.
      await state.run.cancel();
    } catch (cancelError) {
      log(`shutdown cancel failed: ${cancelError?.message ?? cancelError}`);
    }
  }
  if (state.pump && state.activeRun) {
    try {
      await Promise.race([
        state.pump,
        new Promise((resolve) => {
          const handle = setTimeout(resolve, SHUTDOWN_PUMP_TIMEOUT_MS);
          handle.unref();
        }),
      ]);
    } catch {
      // pump errors are already reported on the wire
    }
  }
  try {
    state.agent?.close?.();
  } catch (error) {
    log(`agent close failed: ${error?.message ?? error}`);
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
  set_mode: handleSetMode,
  "auth-signin": handleAuthSignin,
  "auth-renew": handleAuthSignin,
  "auth-status": handleAuthStatus,
  "auth-signout": handleAuthSignout,
  shutdown: handleShutdown,
};

function dispatch(cmd) {
  const type = cmd?.type;
  if (typeof type !== "string") {
    respond("unknown", cmd?.id ?? null, false, `unknown command type: ${JSON.stringify(type)}`);
    return;
  }
  // Auth commands are host-level and run before init (that is the point of a
  // sign-in flow); every other command still requires an initialized session.
  if (!state.initialized && type !== "init" && !type.startsWith("auth-")) {
    return fail(type, cmd?.id ?? null, "not initialized");
  }
  const handler = handlers[type];
  if (!handler) {
    return fail(type, cmd?.id ?? null, `unknown command type: ${type}`);
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

if (IS_MAIN) {
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
}
