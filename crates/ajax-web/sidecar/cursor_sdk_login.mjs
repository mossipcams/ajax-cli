// Cursor SDK login sidecar: a standalone, single-shot Node process that runs
// @cursor/sdk's interactive sign-in and reports progress as JSON lines. The key
// NEVER leaves this process: it is logged in into an InMemoryCredentialStore,
// verified against Cursor.me, and only then persisted to the FileCredentialStore
// (with --store-path, or ~/.cursor/sdk/auth.json by default). Nothing but these
// protocol lines reaches stdout.
//
// Protocol:
//   stdin:  not read as commands; closing it ('end'/'close') cancels an in-flight
//           login. SIGTERM cancels too. An aborted login emits
//           {"type":"failed","error":"cancelled"} and exits 1.
//   stdout: one JSON object per line, in order:
//           --mode login:
//             (a) {"type":"url","url":<login url>} the moment onLoginUrl fires,
//                 before login resolves; (b) {"type":"verifying"} once login has
//                 resolved and creds loaded; (c) exactly ONE terminal line:
//                 {"type":"succeeded","email":<string|null>,
//                  "expiresAtMs":<number|null>} (exit 0), or
//                 {"type":"failed","error":"text"} for any failure — a missing
//                 pending credential, a Cursor.me verification failure, or a
//                 save failure (exit 1). store.save is NEVER called on a
//                 failure path.
//           --mode status: exactly ONE line:
//             {"type":"status","present":false} when the store has no creds, or
//             {"type":"status","present":true,"valid":true|false,
//              "email":<string|null>,"expiresAtMs":<number|null>} plus an
//             "error":"text" field only when valid is false (exit 0 either way).
//   stderr: diagnostics and all SDK console output. Before the SDK module is
//           imported, every console method (log/info/warn/debug/error/trace) is
//           repointed at stderr so nothing non-JSON can reach stdout, and
//           uncaught/unhandled exceptions are caught and reported redacted as
//           well. Every emitted string passes through redaction: any known
//           apiKey value (the in-memory pending creds or the loaded store
//           creds) is replaced with "<redacted>". No stdout or stderr line ever
//           contains an apiKey.

import { argv, exit, stderr, stdin, stdout } from "node:process";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

// Redaction. Every known apiKey value seen in this process is scrubbed from
// any emitted text, so no stdout/stderr line can carry a key. Defined before
// the console redirect below uses redact.
const knownKeys = new Set();

function trackKey(key) {
  if (typeof key === "string" && key.length > 0) knownKeys.add(key);
}

function redact(text) {
  let out = String(text);
  for (const key of knownKeys) out = out.split(key).join("<redacted>");
  return out;
}

// The SDK logs to console; redirect every console method BEFORE importing it
// (see header), redacting each line through the keys seen so far.
for (const level of ["log", "info", "warn", "debug", "error", "trace"]) {
  console[level] = (...args) => stderr.write(redact(args.map(String).join(" ")) + "\n");
}

const DRAIN_FALLBACK_MS = 2_000;

// ---------------------------------------------------------------------------
// Output plumbing: one JSON object per line, and exit only after draining.

let pendingWrites = 0;
let pendingExitCode = null;
let terminalEmitted = false;

function writeLine(value) {
  if (value?.type === "succeeded" || value?.type === "failed") terminalEmitted = true;
  pendingWrites += 1;
  stdout.write(JSON.stringify(value) + "\n", () => {
    pendingWrites -= 1;
    if (pendingExitCode !== null && pendingWrites === 0) exit(pendingExitCode);
  });
}

function finish(code) {
  if (pendingWrites === 0) exit(code);
  // A terminal line is mid-flight; exit with this code once stdout drains
  // (the unref'd fallback guards against a stuck pipe).
  pendingExitCode = code;
  const fallback = setTimeout(() => exit(code), DRAIN_FALLBACK_MS);
  fallback.unref();
}

function log(text) {
  stderr.write(`cursor-sdk-login: ${text}\n`);
}

// Uncaught and unhandled-rejection errors are reported redacted: one
// diagnostic line, at most one terminal failed line, then exit 1. The raw
// error never reaches stderr or stdout.
function handleFatal(error) {
  const message = redact(error?.message ?? String(error));
  log(`uncaught error: ${message}`);
  if (!terminalEmitted) writeLine({ type: "failed", error: message });
  finish(1);
}
process.on("uncaughtException", handleFatal);
process.on("unhandledRejection", handleFatal);

// ---------------------------------------------------------------------------
// SDK module loading. The sidecar never resolves a module itself: the host
// resolves @cursor/sdk from a trusted location and passes its absolute path as
// an explicit `--sdk-module`, exactly like cursor_sdk_sidecar.mjs: if it cannot
// load, the command fails instead of silently loading anything else.

function flagValue(name) {
  const index = argv.indexOf(name);
  return index >= 0 ? argv[index + 1] : undefined;
}

async function resolveSdk() {
  const fromArg = flagValue("--sdk-module");
  if (!fromArg) {
    writeLine({ type: "failed", error: redact("no --sdk-module given; the host resolves @cursor/sdk and passes its absolute path") });
    return null;
  }
  try {
    const mod = await import(pathToFileURL(resolve(fromArg)).href);
    if (mod?.Cursor) return mod;
  } catch (error) {
    log(`sdk candidate failed: ${redact(error?.message ?? String(error))}`);
  }
  writeLine({ type: "failed", error: redact("no loadable @cursor/sdk module at --sdk-module") });
  return null;
}

// ---------------------------------------------------------------------------
// Modes.

const controller = new AbortController();
let cancelled = false;

function cancel() {
  if (cancelled) return;
  cancelled = true;
  try {
    controller.abort();
  } catch {
    // already aborted
  }
}

// A host that closes the sidecar's stdin (or SIGTERMs it) cancels a pending
// login. resume() drains and discards stdin so 'end' fires reliably on EOF even
// though no host ever writes to it.
stdin.resume();
stdin.on("end", cancel);
stdin.on("close", cancel);
process.on("SIGTERM", cancel);

async function runLogin(mod, store) {
  const pending = new mod.InMemoryCredentialStore();
  // Track the key as early as possible: wrap the in-memory store so any save
  // is seen by redaction before anything else can emit it.
  const trackedStore = {
    load: () => pending.load(),
    save: (creds) => { trackKey(creds?.apiKey); return pending.save(creds); },
    clear: () => pending.clear(),
  };
  const result = await mod.Cursor.auth.login({
    openBrowser: false,
    store: trackedStore,
    apiKeyName: "Ajax",
    signal: controller.signal,
    onLoginUrl: (url) => writeLine({ type: "url", url }),
  });
  trackKey(result?.apiKey);
  writeLine({ type: "verifying" });
  const creds = await pending.load();
  if (!creds?.apiKey) {
    throw new Error("login resolved without credentials");
  }
  trackKey(creds.apiKey);
  await mod.Cursor.me({ apiKey: creds.apiKey }); // verify before persisting
  if (cancelled) throw new Error("cancelled"); // never save after cancellation
  await store.save(creds);
  writeLine({ type: "succeeded", email: creds.email ?? null, expiresAtMs: creds.apiKeyExpiresAtMs ?? null });
}

async function runStatus(mod, store) {
  const creds = await store.load();
  if (!creds?.apiKey) {
    writeLine({ type: "status", present: false });
    return;
  }
  trackKey(creds.apiKey);
  const base = { email: creds.email ?? null, expiresAtMs: creds.apiKeyExpiresAtMs ?? null };
  try {
    await mod.Cursor.me({ apiKey: creds.apiKey });
    writeLine({ type: "status", present: true, valid: true, ...base });
  } catch (error) {
    writeLine({ type: "status", present: true, valid: false, ...base, error: redact(error?.message ?? String(error)) });
  }
}

async function main() {
  const mode = flagValue("--mode");
  const storePath = flagValue("--store-path") ? resolve(flagValue("--store-path")) : undefined;
  if (mode !== "login" && mode !== "status") {
    writeLine({ type: "failed", error: redact(`--mode must be "login" or "status"; got: ${JSON.stringify(mode)}`) });
    finish(1);
    return;
  }
  const mod = await resolveSdk();
  if (!mod) {
    finish(1);
    return;
  }
  // --store-path is omitted in production, giving ~/.cursor/sdk/auth.json.
  const store = storePath ? new mod.FileCredentialStore(storePath) : new mod.FileCredentialStore();
  try {
    if (mode === "login") await runLogin(mod, store);
    else await runStatus(mod, store);
    finish(0);
  } catch (error) {
    log(`login flow failed: ${redact(error?.stack ?? String(error))}`);
    writeLine({ type: "failed", error: redact(cancelled ? "cancelled" : (error?.message ?? String(error))) });
    finish(1);
  }
}

void main();
