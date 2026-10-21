// node:test coverage for cursor_sdk_login.mjs, driven against a fake @cursor/sdk
// written to a throwaway mkdtemp dir. The fake login blocks until a `release`
// file appears next to the fake module (polled every 20ms) and rejects on
// signal abort. Each test spawns a fresh sidecar with HOME set to an mkdtemp
// dir so the real home is never read.

import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

const HERE = dirname(fileURLToPath(import.meta.url));
const SIDECAR = resolve(HERE, "cursor_sdk_login.mjs");
const FAKE_KEY = "sk-FAKE-SECRET";

const fakeSdkSource = `
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const releaseFile = join(here, "release");
const meReleaseFile = join(here, "meRelease");

class FileCredentialStore {
  constructor(path) {
    this.path = path ?? join(homedir(), ".cursor", "sdk", "auth.json");
  }
  async load() {
    try {
      return JSON.parse(readFileSync(this.path, "utf8"));
    } catch {
      return undefined; // absent file means no credentials
    }
  }
  async save(creds) {
    writeFileSync(this.path, JSON.stringify(creds));
  }
}

class InMemoryCredentialStore {
  constructor() {
    this.creds = undefined;
  }
  async load() {
    return this.creds;
  }
  async save(creds) {
    this.creds = creds;
  }
}

async function waitForRelease(signal) {
  for (;;) {
    if (signal?.aborted) throw new Error("aborted");
    if (existsSync(releaseFile)) return;
    await new Promise((resolveTick) => setTimeout(resolveTick, 20));
  }
}

async function waitForMeRelease() {
  for (;;) {
    if (existsSync(meReleaseFile)) return;
    await new Promise((resolveTick) => setTimeout(resolveTick, 20));
  }
}

const Cursor = {
  auth: {
    async login(options) {
      options.onLoginUrl("https://cursor.example/fake-login");
      await waitForRelease(options.signal);
      const creds = { version: 1, backendUrl: "x", apiKey: "sk-FAKE-SECRET", apiKeyExpiresAtMs: 4102444800000, email: "op@example.com", createdAtMs: 1 };
      await options.store.save(creds);
      if (process.env.FAKE_LEAKY === "1") {
        console.log("log leak", creds.apiKey);
        console.warn("warn leak", creds.apiKey);
        console.error("error leak", creds.apiKey);
        console.trace("trace leak", creds.apiKey);
      }
      if (process.env.FAKE_THROW_UNCAUGHT === "1") {
        setTimeout(() => { throw new Error('boom sk-FAKE-SECRET'); }, 0);
        return new Promise(() => {}); // never resolves on purpose
      }
      return creds;
    },
  },
  async me({ apiKey }) {
    if (process.env.FAKE_ME_BLOCK === "1") await waitForMeRelease();
    if (process.env.FAKE_LEAKY === "1") console.log("me leak", apiKey);
    if (process.env.FAKE_ME_REJECT === "1") throw new Error("fake me rejected");
    if (apiKey !== "sk-FAKE-SECRET") throw new Error("fake me: bad key");
    return { email: "op@example.com" };
  },
};

export { Cursor, FileCredentialStore, InMemoryCredentialStore };
`;

class LoginRun {
  constructor({ args = [], env = {} } = {}) {
    this.home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
    this.fakeDir = mkdtempSync(join(tmpdir(), "cursor-login-fake-"));
    writeFileSync(join(this.fakeDir, "fake_cursor_sdk.mjs"), fakeSdkSource);
    this.proc = spawn(process.execPath, [SIDECAR, ...args, "--sdk-module", join(this.fakeDir, "fake_cursor_sdk.mjs")], {
      env: { ...process.env, HOME: this.home, ...env },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.stdoutLines = [];
    this.stderrText = "";
    let stdoutBuffer = "";
    this.proc.stdout.on("data", (chunk) => {
      stdoutBuffer += chunk.toString();
      let index = stdoutBuffer.indexOf("\n");
      while (index >= 0) {
        const line = stdoutBuffer.slice(0, index);
        stdoutBuffer = stdoutBuffer.slice(index + 1);
        if (line.length > 0) this.stdoutLines.push(line);
        index = stdoutBuffer.indexOf("\n");
      }
    });
    this.proc.stderr.on("data", (chunk) => {
      this.stderrText += chunk.toString();
    });
  }

  lines() {
    return this.stdoutLines.map((line) => JSON.parse(line)); // keep the JSON-only invariant hot
  }

  typeLines(type) {
    return this.lines().filter((value) => value.type === type);
  }

  release() {
    writeFileSync(join(this.fakeDir, "release"), "1");
  }

  waitFor(predicate, label) {
    if (predicate()) return Promise.resolve(true);
    return new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(() => {
        this.proc.stdout.removeListener("data", onChange);
        rejectPromise(new Error(`timed out waiting for ${label}; stdout: ${JSON.stringify(this.stdoutLines)}; stderr: ${this.stderrText}`));
      }, 5000);
      let settled = false;
      const onChange = () => {
        if (settled) return;
        try {
          if (predicate()) {
            settled = true;
            clearTimeout(timer);
            this.proc.stdout.removeListener("data", onChange);
            resolvePromise(true);
          }
        } catch {
          // partial line mid-parse is not the predicate yet
        }
      };
      const onExit = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.proc.stdout.removeListener("data", onChange);
        rejectPromise(new Error(`sidecar exited while waiting for ${label}; stdout: ${JSON.stringify(this.stdoutLines)}; stderr: ${this.stderrText}`));
      };
      this.proc.stdout.on("data", onChange);
      this.proc.once("exit", onExit);
    });
  }

  async exitCode() {
    return new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(() => rejectPromise(new Error(`sidecar did not exit; stderr: ${this.stderrText}`)), 5000);
      this.proc.once("exit", (code) => {
        clearTimeout(timer);
        resolvePromise(code);
      });
    });
  }

  close() {
    try {
      this.proc.stdin.end();
    } catch {
      // already dead
    }
    try {
      this.proc.kill();
    } catch {
      // already dead
    }
  }

  assertNoLeak() {
    const all = `${this.stdoutLines.join("\n")}\n${this.stderrText}`;
    assert.ok(!all.includes(FAKE_KEY), `apiKey leaked into output: ${all}`);
  }
}

test("login publishes the url while login is still blocked, before release", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", join(home, "auth.json")] });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    // The url arrived while the fake login is still blocked on the release file.
    assert.equal(run.typeLines("verifying").length, 0);
    assert.equal(run.typeLines("succeeded").length + run.typeLines("failed").length, 0);
    const urlLine = run.typeLines("url")[0];
    assert.equal(urlLine.url, "https://cursor.example/fake-login");

    run.release();
    await run.waitFor(() => run.typeLines("verifying").length > 0, "verifying line");
    const succeeded = await (async () => {
      await run.waitFor(() => run.typeLines("succeeded").length > 0, "succeeded line");
      return run.typeLines("succeeded")[0];
    })();
    assert.equal(succeeded.email, "op@example.com");
    assert.equal(succeeded.expiresAtMs, 4102444800000);
    assert.equal(await run.exitCode(), 0);
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("login success writes the store file and emits succeeded with email and expiresAtMs", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const fakeDir = mkdtempSync(join(tmpdir(), "cursor-login-fake-"));
  writeFileSync(join(fakeDir, "fake_cursor_sdk.mjs"), fakeSdkSource);
  const storePath = join(home, "auth.json");
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath] });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    run.release();
    await run.waitFor(() => run.typeLines("succeeded").length > 0, "succeeded line");
    const succeeded = run.typeLines("succeeded")[0];
    assert.equal(succeeded.email, "op@example.com");
    assert.equal(succeeded.expiresAtMs, 4102444800000);
    assert.equal(await run.exitCode(), 0);

    const saved = JSON.parse(readFileSync(storePath, "utf8"));
    assert.equal(saved.apiKey, FAKE_KEY);
    assert.equal(saved.email, "op@example.com");
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("login with a Cursor.me rejection emits failed, exits 1, and never touches the store", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  const seeded = JSON.stringify({ version: 1, apiKey: "sk-OLD-PRESEED" });
  writeFileSync(storePath, seeded);
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath], env: { FAKE_ME_REJECT: "1" } });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    run.release();
    await run.waitFor(() => run.typeLines("failed").length > 0, "failed line");
    const failed = run.typeLines("failed")[0];
    assert.ok(failed);
    assert.equal(run.typeLines("succeeded").length, 0);
    assert.equal(await run.exitCode(), 1);

    // The pre-seeded store must be byte-identical: save was never called.
    assert.equal(readFileSync(storePath, "utf8"), seeded);
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("cancelling a blocked login via stdin close emits failed 'cancelled' and leaves the store untouched", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  const seeded = JSON.stringify({ version: 1, apiKey: "sk-OLD-PRESEED" });
  writeFileSync(storePath, seeded);
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath] });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    // Close stdin while the fake login is still blocked: this must cancel it.
    run.proc.stdin.end();
    await run.waitFor(() => run.typeLines("failed").length > 0, "failed line");
    const failed = run.typeLines("failed")[0];
    assert.equal(failed.error, "cancelled");
    assert.equal(run.typeLines("succeeded").length, 0);
    assert.equal(await run.exitCode(), 1);

    assert.equal(readFileSync(storePath, "utf8"), seeded);
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("status: no store -> present false; valid store -> valid true; me rejection -> valid false", async () => {
  // (a) No store at all: HOME points at an empty temp dir and --store-path is omitted.
  const emptyHome = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const fakeDirA = mkdtempSync(join(tmpdir(), "cursor-login-fake-"));
  writeFileSync(join(fakeDirA, "fake_cursor_sdk.mjs"), fakeSdkSource);
  {
    const proc = spawn(process.execPath, [SIDECAR, "--mode", "status", "--sdk-module", join(fakeDirA, "fake_cursor_sdk.mjs")], {
      env: { ...process.env, HOME: emptyHome },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const [stdoutText, stderrText] = await collectOutput(proc);
    assert.equal(stdoutText.trim(), JSON.stringify({ type: "status", present: false }));
    assert.ok(!`${stdoutText}${stderrText}`.includes(FAKE_KEY));
  }

  // (b) Valid store: status must report present:true, valid:true.
  const homeB = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storeB = join(homeB, "auth.json");
  writeFileSync(storeB, JSON.stringify({ version: 1, apiKey: FAKE_KEY, email: "op@example.com", apiKeyExpiresAtMs: 4102444800000 }));
  const fakeDirB = mkdtempSync(join(tmpdir(), "cursor-login-fake-"));
  writeFileSync(join(fakeDirB, "fake_cursor_sdk.mjs"), fakeSdkSource);
  {
    const proc = spawn(process.execPath, [SIDECAR, "--mode", "status", "--store-path", storeB, "--sdk-module", join(fakeDirB, "fake_cursor_sdk.mjs")], {
      env: { ...process.env, HOME: homeB },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const [stdoutText, stderrText] = await collectOutput(proc);
    assert.deepEqual(JSON.parse(stdoutText.trim()), { type: "status", present: true, valid: true, email: "op@example.com", expiresAtMs: 4102444800000 });
    assert.ok(!`${stdoutText}${stderrText}`.includes(FAKE_KEY));
  }

  // (c) FAKE_ME_REJECT=1: same store, but me() rejects -> valid:false with an error.
  {
    const proc = spawn(process.execPath, [SIDECAR, "--mode", "status", "--store-path", storeB, "--sdk-module", join(fakeDirB, "fake_cursor_sdk.mjs")], {
      env: { ...process.env, HOME: homeB, FAKE_ME_REJECT: "1" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    const [stdoutText, stderrText] = await collectOutput(proc);
    const statusLine = JSON.parse(stdoutText.trim());
    assert.equal(statusLine.type, "status");
    assert.equal(statusLine.present, true);
    assert.equal(statusLine.valid, false);
    assert.equal(statusLine.email, "op@example.com");
    assert.equal(typeof statusLine.error, "string");
    assert.ok(!`${stdoutText}${stderrText}`.includes(FAKE_KEY));
  }
});

test("cancelling during Cursor.me verification never saves and emits a single failed 'cancelled' line", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  const seeded = JSON.stringify({ version: 1, apiKey: "sk-OLD-PRESEED" });
  writeFileSync(storePath, seeded);
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath], env: { FAKE_ME_BLOCK: "1" } });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    run.release();
    await run.waitFor(() => run.typeLines("verifying").length > 0, "verifying line");
    // Close stdin while Cursor.me is blocked; then release me so the flow reaches the pre-save check.
    run.proc.stdin.end();
    writeFileSync(join(run.fakeDir, "meRelease"), "1");
    await run.waitFor(() => run.typeLines("failed").length > 0, "failed line");
    const failed = run.typeLines("failed")[0];
    assert.equal(failed.error, "cancelled");
    // Exactly one terminal line in total, and it is the cancellation.
    assert.equal(run.typeLines("succeeded").length + run.typeLines("failed").length, 1);
    assert.equal(await run.exitCode(), 1);

    // The pre-seeded store must be byte-identical: save was never called.
    assert.equal(readFileSync(storePath, "utf8"), seeded);
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("status ignores cancellation: closing stdin immediately still yields exactly one valid status line", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  writeFileSync(storePath, JSON.stringify({ version: 1, apiKey: FAKE_KEY, email: "op@example.com", apiKeyExpiresAtMs: 4102444800000 }));
  const run = new LoginRun({ args: ["--mode", "status", "--store-path", storePath] });
  try {
    // Close stdin immediately, before the sidecar has emitted anything.
    run.proc.stdin.end();
    await run.waitFor(() => run.typeLines("status").length > 0, "status line");
    assert.equal(run.lines().length, 1);
    assert.deepEqual(run.lines()[0], { type: "status", present: true, valid: true, email: "op@example.com", expiresAtMs: 4102444800000 });
    assert.equal(await run.exitCode(), 0);
  } finally {
    run.assertNoLeak();
    run.close();
  }
});

test("FAKE_LEAKY: SDK console output carrying the key is redacted and login still succeeds", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath], env: { FAKE_LEAKY: "1" } });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    run.release();
    await run.waitFor(() => run.typeLines("succeeded").length > 0, "succeeded line");
    const succeeded = run.typeLines("succeeded")[0];
    assert.equal(succeeded.email, "op@example.com");
    assert.equal(await run.exitCode(), 0);
  } finally {
    // No stdout or stderr byte sequence may contain the key.
    run.assertNoLeak();
    run.close();
  }
});

test("FAKE_THROW_UNCAUGHT: uncaught throw becomes one redacted failed line, non-zero exit, store untouched", async () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-login-home-"));
  const storePath = join(home, "auth.json");
  const seeded = JSON.stringify({ version: 1, apiKey: "sk-OLD-PRESEED" });
  writeFileSync(storePath, seeded);
  const run = new LoginRun({ args: ["--mode", "login", "--store-path", storePath], env: { FAKE_THROW_UNCAUGHT: "1" } });
  try {
    await run.waitFor(() => run.typeLines("url").length > 0, "url line");
    run.release();
    await run.waitFor(() => run.typeLines("failed").length > 0, "failed line");
    const failed = run.typeLines("failed")[0];
    assert.ok(failed.error.includes("<redacted>"), `expected <redacted> in: ${failed.error}`);
    assert.ok(!failed.error.includes(FAKE_KEY), `key leaked into failed line: ${failed.error}`);
    // Exactly one terminal line in total, and it is the redacted failure.
    assert.equal(run.typeLines("succeeded").length + run.typeLines("failed").length, 1);
    const code = await run.exitCode();
    assert.notEqual(code, 0);

    // The pre-seeded store must be byte-identical: save was never called.
    assert.equal(readFileSync(storePath, "utf8"), seeded);
  } finally {
    // No output anywhere (stdout or stderr) may contain the key.
    run.assertNoLeak();
    run.close();
  }
});
async function collectOutput(proc) {
  const [stdoutText, stderrText] = await Promise.all([collectStream(proc.stdout), collectStream(proc.stderr)]);
  await new Promise((resolvePromise) => proc.once("exit", resolvePromise));
  return [stdoutText, stderrText];
}

function collectStream(stream) {
  return new Promise((resolvePromise) => {
    let text = "";
    stream.on("data", (chunk) => (text += chunk.toString()));
    stream.on("end", () => resolvePromise(text));
  });
}
