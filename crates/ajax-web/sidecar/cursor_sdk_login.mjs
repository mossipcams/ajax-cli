import { argv, exit, stderr, stdin, stdout } from "node:process";
import { pathToFileURL } from "node:url";
import { resolve } from "node:path";

const knownKeys = new Set();

function trackKey(key) {
  if (typeof key === "string" && key.length > 0) knownKeys.add(key);
}

function redact(text) {
  let out = String(text);
  for (const key of knownKeys) out = out.split(key).join("<redacted>");
  return out;
}

for (const level of ["log", "info", "warn", "debug", "error", "trace"]) {
  console[level] = (...args) => stderr.write(redact(args.map(String).join(" ")) + "\n");
}

const DRAIN_FALLBACK_MS = 2_000;

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
  pendingExitCode = code;
  const fallback = setTimeout(() => exit(code), DRAIN_FALLBACK_MS);
  fallback.unref();
}

function log(text) {
  stderr.write(`cursor-sdk-login: ${text}\n`);
}

function handleFatal(error) {
  const message = redact(error?.message ?? String(error));
  log(`uncaught error: ${message}`);
  if (!terminalEmitted) writeLine({ type: "failed", error: message });
  finish(1);
}
process.on("uncaughtException", handleFatal);
process.on("unhandledRejection", handleFatal);

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

const controller = new AbortController();
let cancelled = false;

function cancel() {
  if (cancelled) return;
  cancelled = true;
  try {
    controller.abort();
  } catch {
  }
}

stdin.resume();
stdin.on("end", cancel);
stdin.on("close", cancel);
process.on("SIGTERM", cancel);

async function runLogin(mod, store) {
  const pending = new mod.InMemoryCredentialStore();
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
  await mod.Cursor.me({ apiKey: creds.apiKey });
  if (cancelled) throw new Error("cancelled");
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
