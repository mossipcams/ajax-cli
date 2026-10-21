import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { chmodSync, existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const SIDECAR = resolve(HERE, "cursor_sdk_sidecar.mjs");

function makeSandbox() {
  const cwd = mkdtempSync(join(tmpdir(), "cursor-sidecar-cwd-"));
  const home = mkdtempSync(join(tmpdir(), "cursor-sidecar-home-"));
  return { cwd, home };
}

function writeStore(home, store) {
  const dir = join(home, ".cursor", "sdk");
  mkdirSync(dir, { recursive: true });
  const path = join(dir, "auth.json");
  writeFileSync(path, typeof store === "string" ? store : JSON.stringify(store));
  return path;
}

async function until(check, label, timeoutMs = 5000) {
  const startedAt = Date.now();
  while (!check()) {
    if (Date.now() - startedAt > timeoutMs) throw new Error(`timed out waiting for ${label}`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 10));
  }
}

// ---------------------------------------------------------------------------
// Slice 3: host-side SDK sign-in (auth-signin / auth-renew / auth-status /
// auth-signout), driven against an INLINE fake @cursor/sdk written to a temp
// file from within this test. The fake's behavior is selected by
// FAKE_AUTH_MODE and its minted key comes from FAKE_AUTH_KEY; every test case
// asserts the raw key never reaches any JSONL stdout line or stderr.

const LOGIN_SECRET = "sk-fake-cursor-login-SECRET";
const PRIOR_STORE_KEY = "stored-prior-key-not-a-secret";

const AUTH_FAKE_SDK = [
  "import { appendFileSync } from 'node:fs';",
  "const mode = process.env.FAKE_AUTH_MODE ?? 'ok';",
  "const key = process.env.FAKE_AUTH_KEY ?? 'sk-fake-default';",
  "function rec(entry) { if (process.env.FAKE_AUTH_RECORD) appendFileSync(process.env.FAKE_AUTH_RECORD, JSON.stringify(entry) + '\\n'); }",
  "export const Agent = { create: () => Promise.resolve({ agentId: 'auth-1', close() {}, send: () => Promise.resolve({ stream: async function* () {}, wait: async () => ({ status: 'finished' }) }) }) };",
  "export const Cursor = {",
  "  auth: {",
  "    login(options) {",
  "      rec({ call: 'auth.login', openBrowser: options?.openBrowser });",
  "      if (typeof options?.onLoginUrl === 'function') options.onLoginUrl('https://cursor.com/sdk/login?code=fake-login-code-123');",
  "      if (mode === 'cancel') return Promise.reject(Object.assign(new Error('user cancelled'), { name: 'AbortError' }));",
  "      if (mode === 'timeout') return Promise.reject(new Error('login timed out'));",
  "      return Promise.resolve({ apiKey: key, expiresAt: '2099-01-01T00:00:00.000Z' });",
  "    },",
  "  },",
  "  me(options) {",
  "    rec({ call: 'me', hasKey: Boolean(options?.apiKey) });",
  "    if (mode === 'reject-me') return Promise.reject(new Error('Invalid User API Key'));",
  "    return Promise.resolve({ email: 'operator@example.com' });",
  "  },",
  "};",
].join("\n");

function writeAuthFakeSdk() {
  const dir = mkdtempSync(join(tmpdir(), "cursor-auth-sdk-"));
  const sdkPath = join(dir, "fake_cursor_sdk_auth.mjs");
  writeFileSync(sdkPath, AUTH_FAKE_SDK);
  return { sdkPath, recordPath: join(dir, "auth-calls.jsonl") };
}

function spawnAuthSidecar({ sdkPath, mode = "ok", recordPath }) {
  const { cwd, home } = makeSandbox();
  const state = { lines: [], stderrText: "", buffer: "" };
  const proc = spawn(process.execPath, [SIDECAR, "--sdk-module", sdkPath], {
    cwd,
    env: { ...process.env, HOME: home, FAKE_AUTH_MODE: mode, FAKE_AUTH_KEY: LOGIN_SECRET, ...(recordPath ? { FAKE_AUTH_RECORD: recordPath } : {}) },
    stdio: ["pipe", "pipe", "pipe"],
  });
  proc.stdout.on("data", (chunk) => {
    state.buffer += chunk.toString();
    let index = state.buffer.indexOf("\n");
    while (index >= 0) {
      const line = state.buffer.slice(0, index);
      state.buffer = state.buffer.slice(index + 1);
      if (line.length > 0) state.lines.push(line);
      index = state.buffer.indexOf("\n");
    }
  });
  proc.stderr.on("data", (chunk) => { state.stderrText += chunk.toString(); });
  return {
    proc,
    home,
    lines: () => state.lines,
    stderr: () => state.stderrText,
    send(command) {
      proc.stdin.write(JSON.stringify({ id: Math.random().toString(16).slice(2), type: command.type, ...command }) + "\n");
    },
    response(type) { return state.lines.map((line) => JSON.parse(line)).find((value) => value.type === "response" && value.command === type); },
    records() {
      if (!existsSync(recordPath)) return [];
      return readFileSync(recordPath, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line));
    },
  };
}

function storeFileFor(home) {
  return join(home, ".cursor", "sdk", "auth.json");
}

function assertNoRawKey(sidecar) {
  for (const line of sidecar.lines()) {
    assert.ok(!line.includes(LOGIN_SECRET), `raw key reached stdout: ${line}`);
  }
  assert.ok(!sidecar.stderr().includes(LOGIN_SECRET), `raw key reached stderr: ${sidecar.stderr()}`);
}

function assertNoTempLeftovers(home) {
  const dir = join(home, ".cursor", "sdk");
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir)) {
    assert.ok(!entry.includes(".tmp"), `partial temp file left behind: ${entry}`);
  }
}

function seedPriorStore(home) {
  const path = writeStore(home, { apiKey: PRIOR_STORE_KEY });
  return { path, content: readFileSync(path, "utf8") };
}

test("auth-signin surfaces the login URL, stores the key 0600, and replies sanitized status", async () => {
  const { sdkPath, recordPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath, recordPath });
  try {
    sidecar.send({ type: "auth-signin" });
    await until(() => sidecar.response("auth-signin") !== undefined, "auth-signin response");

    // The URL event arrived and carries no key material.
    const urlLines = sidecar.lines().filter((line) => line.includes('"auth_url"'));
    assert.equal(urlLines.length, 1, `expected exactly one auth_url event; got: ${urlLines.join(" | ")}`);
    assert.ok(!urlLines[0].includes(LOGIN_SECRET), `login URL leaked the key: ${urlLines[0]}`);

    const response = sidecar.response("auth-signin");
    assert.equal(response.success, true);
    assert.equal(response.data.status, "signed-in");
    assert.equal(response.data.accountLabel, "operator@example.com");
    assert.equal(response.data.expiresAt, "2099-01-01T00:00:00.000Z");

    // The protected store file was created with 0600 perms and holds the key.
    const storePath = storeFileFor(sidecar.home);
    assert.ok(existsSync(storePath), "protected store file was not created");
    assert.equal(statSync(storePath).mode & 0o777, 0o600, `store file mode is not 0600: ${(statSync(storePath).mode & 0o777).toString(8)}`);
    assert.equal(JSON.parse(readFileSync(storePath, "utf8")).apiKey, LOGIN_SECRET);

    // Browser opening was disabled and the minted key reached Cursor.me().
    const login = sidecar.records().find((entry) => entry.call === "auth.login");
    assert.ok(login, "fake SDK never recorded an auth.login call");
    assert.equal(login.openBrowser, false);
    const meCall = sidecar.records().find((entry) => entry.call === "me");
    assert.equal(meCall?.hasKey, true, "Cursor.me() must receive the minted key for verification");

    assertNoRawKey(sidecar);
  } finally {
    sidecar.proc.kill();
  }
});

test("auth-signin cancellation: redacted typed login-stage error, prior credential intact", async () => {
  const { sdkPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath, mode: "cancel" });
  const prior = seedPriorStore(sidecar.home);
  try {
    sidecar.send({ type: "auth-signin" });
    await until(() => sidecar.response("auth-signin") !== undefined, "cancelled auth-signin response");

    const response = sidecar.response("auth-signin");
    assert.equal(response.success, false);
    assert.match(response.error, /stage "login"/);
    assert.match(response.error, /cancelled/);

    // The prior credential is byte-for-byte intact; no partial file appears.
    assert.equal(readFileSync(prior.path, "utf8"), prior.content);
    assertNoTempLeftovers(sidecar.home);
    assertNoRawKey(sidecar);
  } finally {
    sidecar.proc.kill();
  }
});

test("auth-signin timeout: redacted typed login-stage error, prior credential intact", async () => {
  const { sdkPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath, mode: "timeout" });
  const prior = seedPriorStore(sidecar.home);
  try {
    sidecar.send({ type: "auth-signin" });
    await until(() => sidecar.response("auth-signin") !== undefined, "timed-out auth-signin response");

    const response = sidecar.response("auth-signin");
    assert.equal(response.success, false);
    assert.match(response.error, /stage "login"/);
    assert.match(response.error, /timed out/);

    assert.equal(readFileSync(prior.path, "utf8"), prior.content);
    assertNoTempLeftovers(sidecar.home);
    assertNoRawKey(sidecar);
  } finally {
    sidecar.proc.kill();
  }
});

test("auth-signin with a rejected Cursor.me() verification: store untouched, redacted verify-stage error", async () => {
  const { sdkPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath, mode: "reject-me" });
  const prior = seedPriorStore(sidecar.home);
  try {
    sidecar.send({ type: "auth-signin" });
    await until(() => sidecar.response("auth-signin") !== undefined, "rejected auth-signin response");

    const response = sidecar.response("auth-signin");
    assert.equal(response.success, false);
    assert.match(response.error, /stage "verify"/);
    // The SDK's raw rejection text ("Invalid User API Key") is never forwarded.
    assert.ok(!response.error.includes("Invalid User API Key"), `SDK error text leaked: ${response.error}`);

    assert.equal(readFileSync(prior.path, "utf8"), prior.content);
    assertNoTempLeftovers(sidecar.home);
    assertNoRawKey(sidecar);
  } finally {
    sidecar.proc.kill();
  }
});

test("auth-signin store-write permission failure: redacted typed error, no partial file", async () => {
  const { sdkPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath });
  const storeDir = join(sidecar.home, ".cursor", "sdk");
  mkdirSync(storeDir, { recursive: true });
  chmodSync(storeDir, 0o500); // read-only: the temp write must fail
  try {
    sidecar.send({ type: "auth-signin" });
    await until(() => sidecar.response("auth-signin") !== undefined, "permission-failed auth-signin response");

    const response = sidecar.response("auth-signin");
    assert.equal(response.success, false);
    assert.match(response.error, /stage "store"/);

    // No store file and no partial temp file may exist in the read-only dir.
    for (const entry of readdirSync(storeDir)) {
      assert.notEqual(entry, "auth.json", `store file must not be created on write failure: ${entry}`);
      assert.ok(!entry.includes(".tmp"), `partial temp file left behind: ${entry}`);
    }
    assertNoRawKey(sidecar);
  } finally {
    chmodSync(storeDir, 0o755);
    sidecar.proc.kill();
  }
});

test("auth-renew atomically replaces the existing store; auth-signout removes it", async () => {
  const { sdkPath } = writeAuthFakeSdk();
  const sidecar = spawnAuthSidecar({ sdkPath });
  const prior = seedPriorStore(sidecar.home);
  try {
    sidecar.send({ type: "auth-renew" });
    await until(() => sidecar.response("auth-renew") !== undefined, "auth-renew response");

    const response = sidecar.response("auth-renew");
    assert.equal(response.success, true);
    assert.equal(response.data.status, "signed-in");
    // The prior credential is atomically replaced by the minted key.
    assert.equal(readFileSync(prior.path, "utf8"), JSON.stringify({ apiKey: LOGIN_SECRET, expiresAt: "2099-01-01T00:00:00.000Z" }));
    assertNoTempLeftovers(sidecar.home);
    assertNoRawKey(sidecar);

    // auth-status now reports presence + sanitized fields, never the key.
    sidecar.send({ type: "auth-status" });
    await until(() => sidecar.response("auth-status") !== undefined, "auth-status response");
    const status = sidecar.response("auth-status");
    assert.equal(status.success, true);
    assert.deepEqual(status.data, { present: true, expiresAt: "2099-01-01T00:00:00.000Z" });

    // auth-signout removes the protected store file.
    sidecar.send({ type: "auth-signout" });
    await until(() => sidecar.response("auth-signout") !== undefined, "auth-signout response");
    const signout = sidecar.response("auth-signout");
    assert.equal(signout.success, true);
    assert.ok(!existsSync(prior.path), "store file must be removed on sign-out");

    // After sign-out the status is absent.
    sidecar.send({ type: "auth-status" });
    await until(() => sidecar.lines().filter((line) => line.includes('"auth-status"')).length > 1, "second auth-status response");
    const statuses = sidecar.lines().map((line) => JSON.parse(line)).filter((value) => value.type === "response" && value.command === "auth-status");
    assert.deepEqual(statuses[1].data, { present: false });

    assertNoRawKey(sidecar);
  } finally {
    sidecar.proc.kill();
  }
});
