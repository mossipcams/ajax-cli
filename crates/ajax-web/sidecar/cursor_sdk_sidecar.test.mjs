// node:test coverage for cursor_sdk_sidecar.mjs, driven against the fake
// @cursor/sdk fixture. Each test spawns a fresh sidecar with HOME and cwd set
// to throwaway mkdtemp directories so the real home is never read.

import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import {
  CursorCredentialError,
  parseApiKeyArg,
  resolveCursorCredential,
} from "./cursor_sdk_sidecar.mjs";

const TEST_SIDECAR_API_KEY = "test-sidecar-api-key-not-a-secret";

const HERE = dirname(fileURLToPath(import.meta.url));
const SIDECAR = resolve(HERE, "cursor_sdk_sidecar.mjs");
const FAKE_SDK = resolve(HERE, "..", "tests", "fixtures", "fake_cursor_sdk.mjs");

function makeSandbox() {
  const cwd = mkdtempSync(join(tmpdir(), "cursor-sidecar-cwd-"));
  const home = mkdtempSync(join(tmpdir(), "cursor-sidecar-home-"));
  return { cwd, home };
}

class Sidecar {
  constructor({ cwd, home } = {}) {
    const { cwd: realCwd, home: realHome } = makeSandbox();
    this.cwd = cwd ?? realCwd;
    this.home = home ?? realHome;
    this.proc = spawn(process.execPath, [
      SIDECAR,
      "--sdk-module",
      FAKE_SDK,
      "--api-key",
      TEST_SIDECAR_API_KEY,
    ], {
      cwd: this.cwd,
      env: { ...process.env, HOME: this.home },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.stdoutLines = [];
    this.consumedResponses = {}; // command type -> how many responses were already handed out
    this.consumedResults = 0;
    this.stdoutDataHandlers = new Set();
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
    this.stdoutDataHandlers = new Set();
    const onStdoutData = () => {
      for (const handler of [...this.stdoutDataHandlers]) handler();
    };
    this.proc.stdout.on("data", onStdoutData);
    this.proc.stderr.on("data", (chunk) => {
      this.stderrText += chunk.toString();
    });
    // Every stdout line must be valid JSON protocol; "sdk noise" from the fake
    // console.log must never appear here. This check is global to all tests.
    for (const line of this.stdoutLines) {
      let parsed;
      assert.doesNotThrow(() => {
        parsed = JSON.parse(line);
      }, `stdout line is not JSON: ${line}`);
      assert.equal(parsed?.type, undefined, `non-protocol line on stdout: ${line}`);
    }
  }

  rawWrite(text) {
    this.proc.stdin.write(text);
  }

  send(command) {
    this.proc.stdin.write(`${JSON.stringify({ id: Math.random().toString(16).slice(2), type: command.type, ...command })}\n`);
  }

  lines() {
    for (const line of this.stdoutLines) JSON.parse(line); // keep the JSON-only invariant hot
    return this.stdoutLines.map((line) => JSON.parse(line));
  }

  events() {
    return this.lines().filter((value) => typeof value.type === "string" && !["response", "result"].includes(value.type));
  }

  results() {
    return this.lines().filter((value) => value.type === "result");
  }

  waitFor(predicate, label) {
    if (predicate()) return Promise.resolve(true);
    return new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(() => {
        this.stdoutDataHandlers.delete(onChange);
        rejectPromise(new Error(`timed out waiting for ${label}; stderr: ${this.stderrText}`));
      }, 5000);
      let settled = false;
      const onChange = () => {
        if (settled) return;
        if (predicate()) {
          settled = true;
          clearTimeout(timer);
          this.stdoutDataHandlers.delete(onChange);
          resolvePromise(true);
        }
      };
      const onExit = (code, signal) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        this.stdoutDataHandlers.delete(onChange);
        rejectPromise(new Error(`sidecar exited (${signal ?? code}) while waiting for ${label}; stderr: ${this.stderrText}`));
      };
      this.stdoutDataHandlers.add(onChange);
      this.proc.once("exit", onExit);
    });
  }

  async readResponse(type) {
    const consumed = this.consumedResponses[type] ?? 0;
    await this.waitFor(
      () => this.lines().filter((value) => value.type === "response" && value.command === type).length > consumed,
      `${type} response`,
    );
    const matches = this.lines().filter((value) => value.type === "response" && value.command === type);
    this.consumedResponses[type] = consumed + 1;
    return matches[consumed];
  }

  async readResult() {
    await this.waitFor(() => this.results().length > this.consumedResults, "result line");
    const result = this.results()[this.consumedResults];
    this.consumedResults += 1;
    return result;
  }


  async exit() {
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
      this.proc.kill();
    } catch {
      // already dead
    }
  }
}

function writeCommandFiles({ cwd, home, project = {}, user = {} }) {
  for (const [dir, files] of [[cwd, project], [home, user]]) {
    if (Object.keys(files).length === 0) continue;
    const commandsDir = join(dir, ".cursor", "commands");
    mkdirSync(commandsDir, { recursive: true });
    for (const [name, body] of Object.entries(files)) writeFileSync(join(commandsDir, `${name}.md`), body);
  }
}

test("init returns sessionId, both models, and scanned commands", async () => {
  const sidecar = new Sidecar();
  writeCommandFiles({
    cwd: sidecar.cwd,
    home: sidecar.home,
    project: { greet: "# Greet the user\nSay hello.\n" },
  });
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    const response = await sidecar.readResponse("init");
    assert.equal(response.success, true);
    assert.equal(response.data.sessionId, "agent-fake-1");
    assert.deepEqual(
      response.data.models.map((model) => model.id),
      ["default", "grok-4.6"],
    );
    assert.deepEqual(response.data.commands, [{ name: "greet", description: "Greet the user" }]);
  } finally {
    sidecar.close();
  }
});

test("init response carries the non-secret credential source and never key material", async () => {
  const sidecar = new Sidecar(); // spawned with --api-key: the credential came from argv/env
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    const response = await sidecar.readResponse("init");
    assert.equal(response.success, true);
    assert.equal(response.data.cursorCredential, "env");
    assert.ok(
      !JSON.stringify(response).includes(TEST_SIDECAR_API_KEY),
      `init response leaks key material: ${JSON.stringify(response)}`,
    );
  } finally {
    sidecar.close();
  }
});

test("init from the protected store reports the non-secret 'store' status and never the key", async () => {
  const { cwd, home } = makeSandbox();
  writeStore(home, { apiKey: SECRET_KEY });
  const proc = spawn(process.execPath, [SIDECAR, "--sdk-module", FAKE_SDK], {
    cwd,
    env: { ...process.env, HOME: home },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stderrText = "";
  proc.stderr.on("data", (chunk) => { stderrText += chunk.toString(); });
  const rawLines = [];
  let buffer = "";
  proc.stdout.setEncoding("utf8");
  proc.stdout.on("data", (chunk) => {
    buffer += chunk;
    let index = buffer.indexOf("\n");
    while (index >= 0) {
      rawLines.push(buffer.slice(0, index));
      buffer = buffer.slice(index + 1);
      index = buffer.indexOf("\n");
    }
  });
  try {
    proc.stdin.write(`${JSON.stringify({ id: "cred-src", type: "init", cwd, model: { id: "default" } })}\n`);
    await new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(() => rejectPromise(new Error(`timed out waiting for init response; stderr: ${stderrText}`)), 5000);
      proc.stdout.on("data", () => {
        if (rawLines.some((line) => line.includes('"command":"init"'))) {
          clearTimeout(timer);
          resolvePromise();
        }
      });
    });
    const response = JSON.parse(rawLines.find((line) => {
      const parsed = JSON.parse(line);
      return parsed.type === "response" && parsed.command === "init";
    }));
    assert.equal(response.success, true);
    assert.equal(response.data.cursorCredential, "store");
    for (const line of rawLines) {
      assert.ok(!line.includes(SECRET_KEY), `key material reached stdout: ${line}`);
    }
  } finally {
    proc.kill();
  }
});

test("prompt streams forwarded messages in order and ends with one finished result", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");
    sidecar.send({ type: "prompt", message: "hello world" });
    await sidecar.readResponse("prompt");
    const result = await sidecar.readResult();

    const events = sidecar.events();
    assert.deepEqual(
      events.map((message) => [message.type, message.status]),
      [
        ["status", "RUNNING"],
        ["thinking", undefined],
        ["assistant", undefined],
        ["tool_call", "running"],
        ["tool_call", "completed"],
        ["usage", undefined],
        ["status", "FINISHED"],
      ],
    );
    assert.equal(events[2].message.content[0].text, "echo[default]: hello world");

    // The tool_call call_id embeds a newline; it must survive as ONE JSON line.
    const rawToolLines = sidecar.stdoutLines.filter((line) => line.includes("call-1"));
    assert.equal(rawToolLines.length, 2);
    for (const line of rawToolLines) {
      const parsed = JSON.parse(line);
      assert.equal(parsed.call_id, "call-1\nfc_1");
      // The newline in call_id must be escaped inside one physical JSONL line.
      assert.ok(line.includes('"call-1\\nfc_1"'), `newline not escaped within one line: ${line}`);
    }

    assert.equal(sidecar.results().length, 1);
    assert.equal(result.status, "finished");
    assert.equal(result.error, undefined);
    assert.deepEqual(result.usage, { inputTokens: 1, outputTokens: 2, totalTokens: 3 });

    // The fake logged to console; it must have landed on stderr, not stdout.
    assert.ok(sidecar.stderrText.includes("sdk noise"));
  } finally {
    sidecar.close();
  }
});

test("slash command expands project file text plus remainder; project wins over user", async () => {
  const sidecar = new Sidecar();
  writeCommandFiles({
    cwd: sidecar.cwd,
    home: sidecar.home,
    project: { greet: "GREET PROJECT BODY" },
    user: { greet: "GREET USER BODY", onlyuser: "# User only\nbody\n" },
  });
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    const initResponse = await sidecar.readResponse("init");
    assert.equal(initResponse.success, true);
    // Both commands are visible; greet's description comes from the project file.
    assert.deepEqual(
      initResponse.data.commands.map((command) => command.name),
      ["greet", "onlyuser"],
    );

    sidecar.send({ type: "prompt", message: "/greet extra words" });
    await sidecar.readResponse("prompt");
    let result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const assistantText = sidecar.events().find((message) => message.type === "assistant").message.content[0].text;
    // Project file text wins over the user file with the same name.
    assert.equal(assistantText, "echo[default]: GREET PROJECT BODY\n\nextra words");

    // An unknown slash command is sent unchanged.
    sidecar.send({ type: "prompt", message: "/nope stay" });
    await sidecar.readResponse("prompt");
    result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const events = sidecar.events().slice(7); // skip the first run's seven messages
    const unchangedText = events.find((message) => message.type === "assistant").message.content[0].text;
    assert.equal(unchangedText, "echo[default]: /nope stay");
  } finally {
    sidecar.close();
  }
});

test("abort during a SLOW prompt yields cancelled and the next prompt works", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "prompt", message: "be SLOW for a bit" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);

    // Let the RUNNING status stream first, then abort.
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
    sidecar.send({ type: "abort" });
    const abortResponse = await sidecar.readResponse("abort");
    assert.equal(abortResponse.success, true);

    const result = await sidecar.readResult();
    assert.equal(result.status, "cancelled");
    assert.ok(sidecar.events().some((message) => message.type === "status" && message.status === "CANCELLED"));

    // A following prompt still works.
    sidecar.send({ type: "prompt", message: "back to work" });
    await sidecar.readResponse("prompt");
    const secondResult = await sidecar.readResult();
    assert.equal(secondResult.status, "finished");
    assert.equal(sidecar.results().length, 2);
  } finally {
    sidecar.close();
  }
});

test("FAIL prompt forwards a status ERROR event and results in an error", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "prompt", message: "this will FAIL" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);

    const result = await sidecar.readResult();
    const events = sidecar.events();
    assert.ok(events.some((message) => message.type === "status" && message.status === "ERROR" && message.message === "out of usage"));
    assert.equal(result.status, "error");
    assert.equal(result.error, "out of usage");
  } finally {
    sidecar.close();
  }
});

test("init with a missing resume id fails and never falls back to create", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" }, resume: "agent-missing" });
    const initResponse = await sidecar.readResponse("init");
    assert.equal(initResponse.success, false);
    assert.match(initResponse.error, /Agent agent-missing not found/);

    // No create fallback happened: the session is not initialized.
    sidecar.send({ type: "prompt", message: "any" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, false);
    assert.match(promptResponse.error, /not initialized/);

    // A valid resume does initialize.
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" }, resume: "agent-resumed-42" });
    const secondInit = await sidecar.readResponse("init");
    assert.equal(secondInit.success, true);
    assert.equal(secondInit.data.sessionId, "agent-resumed-42");

    sidecar.send({ type: "prompt", message: "hello" });
    await sidecar.readResponse("prompt");
    const result = await sidecar.readResult();
    assert.equal(result.status, "finished");
  } finally {
    sidecar.close();
  }
});

test("set_model changes the model used by the next send", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "set_model", model: { id: "grok-4.6" } });
    const setModelResponse = await sidecar.readResponse("set_model");
    assert.equal(setModelResponse.success, true);

    sidecar.send({ type: "prompt", message: "who are you" });
    await sidecar.readResponse("prompt");
    const result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const assistantText = sidecar.events().find((message) => message.type === "assistant").message.content[0].text;
    assert.equal(assistantText, "echo[grok-4.6]: who are you");
  } finally {
    sidecar.close();
  }
});

test("non-JSON input line produces a parse error response with no id", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.rawWrite("{not json at all\n");
    const response = await sidecar.readResponse("parse");
    assert.equal(response.success, false);
    assert.equal(response.id, undefined);
    assert.match(response.error, /invalid JSON/);

    // The session still works afterwards.
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    const initResponse = await sidecar.readResponse("init");
    assert.equal(initResponse.success, true);
  } finally {
    sidecar.close();
  }
});

test("prompt while a run is active is rejected as busy", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "prompt", message: "be SLOW for a bit" });
    await sidecar.readResponse("prompt");
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));

    sidecar.send({ type: "prompt", message: "and one more" });
    const busyResponse = await sidecar.readResponse("prompt");
    assert.equal(busyResponse.success, false);
    assert.match(busyResponse.error, /busy/);

    sidecar.send({ type: "abort" });
    await sidecar.readResponse("abort");
    const result = await sidecar.readResult();
    assert.equal(result.status, "cancelled");
  } finally {
    sidecar.close();
  }
});

test("abort before the run exists (late send) still cancels it", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    // The fake send() for LATE text resolves only after 100ms, so this abort
    // lands while state.run is still null (the race the fix covers).
    sidecar.send({ type: "prompt", message: "LATE SLOW" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);

    // Immediately, without waiting for any stream event.
    sidecar.send({ type: "abort" });
    const abortResponse = await sidecar.readResponse("abort");
    assert.equal(abortResponse.success, true);

    const result = await sidecar.readResult();
    assert.equal(result.status, "cancelled");
  } finally {
    sidecar.close();
  }
});

test("shutdown with an active run cancels it and exits 0 under 3s", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "prompt", message: "SLOW" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);

    const startedAt = Date.now();
    sidecar.send({ type: "shutdown" });
    await sidecar.readResponse("shutdown");

    // The terminal result line is written before the process exits.
    await sidecar.waitFor(() => sidecar.results().length > 0, "cancelled result line");

    const code = await Promise.race([
      sidecar.exit(),
      new Promise((_, reject) => {
        const handle = setTimeout(() => reject(new Error("sidecar did not exit within 3s")), 3000);
        handle.unref();
      }),
    ]);
    assert.equal(code, 0);
    assert.ok(Date.now() - startedAt < 3000, `shutdown took ${Date.now() - startedAt}ms`);

    const result = sidecar.results()[0];
    assert.equal(result.status, "cancelled");
  } finally {
    sidecar.close();
  }
});

test("prompt images echo the count; an invalid image shape is rejected without a run", async () => {
  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({
      type: "prompt",
      message: "with pics",
      images: [{ url: "https://example.com/a.png" }, { data: "aGk=", mimeType: "image/png" }],
    });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);
    const result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const assistantText = sidecar.events().find((message) => message.type === "assistant").message.content[0].text;
    assert.equal(assistantText, "echo[default]: with pics images=2");

    // Invalid shape: rejected, and no run starts.
    sidecar.send({ type: "prompt", message: "bad images", images: [{ nope: 1 }] });
    const badResponse = await sidecar.readResponse("prompt");
    assert.equal(badResponse.success, false);
    assert.match(badResponse.error, /prompt images must be \{url\} or \{data,mimeType\}/);

    // Proof no run started: the next prompt is not rejected as busy.
    sidecar.send({ type: "prompt", message: "after bad images" });
    const secondPrompt = await sidecar.readResponse("prompt");
    assert.equal(secondPrompt.success, true);
    const secondResult = await sidecar.readResult();
    assert.equal(secondResult.status, "finished");
    assert.equal(sidecar.results().length, 2);
  } finally {
    sidecar.close();
  }
});

test("set_mode validates and the mode reaches the next send; init mode is validated too", async () => {
  // init rejects a bogus mode.
  const badInitSidecar = new Sidecar();
  badInitSidecar.send({ type: "init", cwd: badInitSidecar.cwd, model: { id: "default" }, mode: "bogus" });
  const badInit = await badInitSidecar.readResponse("init");
  assert.equal(badInit.success, false);
  assert.match(badInit.error, /mode must be/);
  badInitSidecar.close();

  const sidecar = new Sidecar();
  try {
    sidecar.send({ type: "init", cwd: sidecar.cwd, model: { id: "default" } });
    await sidecar.readResponse("init");

    sidecar.send({ type: "set_mode", mode: "plan" });
    const setModeResponse = await sidecar.readResponse("set_mode");
    assert.equal(setModeResponse.success, true);

    sidecar.send({ type: "prompt", message: "plan this" });
    await sidecar.readResponse("prompt");
    const result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const assistantText = sidecar.events().find((message) => message.type === "assistant").message.content[0].text;
    assert.equal(assistantText, "echo[default]: plan this mode=plan");

    sidecar.send({ type: "set_mode", mode: "bogus" });
    const bogusResponse = await sidecar.readResponse("set_mode");
    assert.equal(bogusResponse.success, false);
    assert.match(bogusResponse.error, /mode must be/);
  } finally {
    sidecar.close();
  }
});

// ---------------------------------------------------------------------------
// Cursor SDK credential reader (resolveCursorCredential): env precedence,
// stored key, expiry rejection, missing rejection, redacted error text.

function writeStore(home, store) {
  const dir = join(home, ".cursor", "sdk");
  mkdirSync(dir, { recursive: true });
  const path = join(dir, "auth.json");
  writeFileSync(path, typeof store === "string" ? store : JSON.stringify(store));
  return path;
}

const SECRET_KEY = "sk-cursor-SECRET-DO-NOT-LEAK";

test("credential reader prefers CURSOR_API_KEY over the stored key", () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-cred-home-"));
  const storePath = writeStore(home, { apiKey: "stored-key" });

  assert.equal(
    resolveCursorCredential({ env: { CURSOR_API_KEY: "env-key" }, storePath }),
    "env-key",
  );
  // An empty-string env var is not a credential; fall through to the store.
  assert.equal(
    resolveCursorCredential({ env: { CURSOR_API_KEY: "" }, storePath }),
    "stored-key",
  );
});

test("credential reader uses the stored key when env is unset", () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-cred-home-"));
  const storePath = writeStore(home, { apiKey: SECRET_KEY });

  assert.equal(resolveCursorCredential({ env: {}, storePath }), SECRET_KEY);
});

test("credential reader rejects an expired stored credential", () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-cred-home-"));
  const nowMs = Date.parse("2026-10-20T00:00:00Z");

  const expiredPath = writeStore(home, { apiKey: SECRET_KEY, expiresAt: nowMs - 1 });
  assert.throws(
    () => resolveCursorCredential({ env: {}, storePath: expiredPath, now: () => nowMs }),
    (error) => error instanceof CursorCredentialError && error.code === "expired",
  );

  // Epoch-seconds expiry field is understood too.
  const expiredSecs = writeStore(home, { apiKey: SECRET_KEY, expires_at: Math.floor(nowMs / 1000) - 1 });
  assert.throws(
    () => resolveCursorCredential({ env: {}, storePath: expiredSecs, now: () => nowMs }),
    (error) => error instanceof CursorCredentialError && error.code === "expired",
  );

  // A credential that has not yet expired is accepted.
  const validPath = writeStore(home, { apiKey: "valid-key", expiresAt: new Date(nowMs + 60_000).toISOString() });
  assert.equal(
    resolveCursorCredential({ env: {}, storePath: validPath, now: () => nowMs }),
    "valid-key",
  );

  // No expiry field at all is tolerated (renewed externally, ~90 days).
  const noExpiry = writeStore(home, { apiKey: "no-expiry-key" });
  assert.equal(resolveCursorCredential({ env: {}, storePath: noExpiry }), "no-expiry-key");
});

test("credential reader rejects missing credentials with a typed error", () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-cred-home-"));
  const missingPath = join(home, ".cursor", "sdk", "auth.json"); // never written

  for (const store of [undefined, "{not json", {}, { apiKey: "" }]) {
    if (store !== undefined) writeStore(home, store);
    assert.throws(
      () => resolveCursorCredential({ env: {}, storePath: missingPath }),
      (error) => error instanceof CursorCredentialError && error.code === "missing",
      `expected a typed "missing" error for store: ${JSON.stringify(store)}`,
    );
  }
});

test("parseApiKeyArg reads --api-key from argv", () => {
  assert.equal(
    parseApiKeyArg(["node", "sidecar.mjs", "--sdk-module", "/x", "--api-key", "argv-key"]),
    "argv-key",
  );
  assert.equal(parseApiKeyArg(["node", "sidecar.mjs"]), undefined);
  assert.equal(parseApiKeyArg(["node", "sidecar.mjs", "--api-key", ""]), undefined);
});

test("init without a credential yields a typed redacted init error", async () => {
  const { cwd, home } = makeSandbox();
  const proc = spawn(process.execPath, [SIDECAR, "--sdk-module", FAKE_SDK], {
    cwd,
    env: { ...process.env, HOME: home },
    stdio: ["pipe", "pipe", "pipe"],
  });
  let stdoutBuffer = "";
  const lines = [];
  proc.stdout.on("data", (chunk) => {
    stdoutBuffer += chunk.toString();
    let index = stdoutBuffer.indexOf("\n");
    while (index >= 0) {
      const line = stdoutBuffer.slice(0, index);
      stdoutBuffer = stdoutBuffer.slice(index + 1);
      if (line.length > 0) lines.push(JSON.parse(line));
      index = stdoutBuffer.indexOf("\n");
    }
  });
  try {
    proc.stdin.write(`${JSON.stringify({ id: "cred-1", type: "init", cwd, model: { id: "default" } })}\n`);
    await new Promise((resolvePromise, rejectPromise) => {
      const timer = setTimeout(() => rejectPromise(new Error("timed out waiting for init response")), 5000);
      const onData = () => {
        const response = lines.find((line) => line.type === "response" && line.command === "init");
        if (!response) return;
        clearTimeout(timer);
        proc.stdout.off("data", onData);
        resolvePromise(response);
      };
      proc.stdout.on("data", onData);
      onData();
    });
    const response = lines.find((line) => line.type === "response" && line.command === "init");
    assert.equal(response.success, false);
    assert.match(response.error, /no Cursor SDK credential found/);
    assert.ok(!response.error.includes(TEST_SIDECAR_API_KEY));
    assert.ok(!response.error.includes("sk-cursor"));
  } finally {
    proc.kill();
  }
});

test("credential reader errors are redacted: no key material in any message", () => {
  const home = mkdtempSync(join(tmpdir(), "cursor-cred-home-"));
  const nowMs = Date.now() + 10_000;
  const expiredPath = writeStore(home, { apiKey: SECRET_KEY, expiresAt: nowMs });

  let error;
  try {
    resolveCursorCredential({ env: {}, storePath: expiredPath, now: () => nowMs });
    assert.fail("expected the reader to throw for an expired credential");
  } catch (thrown) {
    error = thrown;
  }

  assert.ok(!String(error).includes(SECRET_KEY), `expired error leaks key material: ${error}`);
  assert.ok(!error.stack.includes(SECRET_KEY));

  const badPath = join(home, "bogus", "auth.json"); // missing file
  try {
    resolveCursorCredential({ env: {}, storePath: badPath });
    assert.fail("expected the reader to throw for a missing store");
  } catch (thrown) {
    assert.ok(!String(thrown).includes(SECRET_KEY), `missing error leaks key material: ${thrown}`);
  }
});

// ---------------------------------------------------------------------------
// Slice 3: the credential resolved at init must reach the fake SDK's call
// options (create / resume / models.list), and must NEVER appear in any JSONL
// response line or stderr/log line. The shared fixture does not record call
// options, so these tests use their OWN inline fake SDK written to a temp file
// from within this test; the fake appends each entry point's options (raw,
// including apiKey) to a record file named by FAKE_SDK_RECORD.

const RECORDING_FAKE_SDK = [
  "import { appendFileSync } from 'node:fs';",
  "const recordPath = process.env.FAKE_SDK_RECORD;",
  "function rec(entry) { if (recordPath) appendFileSync(recordPath, JSON.stringify(entry) + '\\n'); }",
  "function makeRun(text) {",
  "  return {",
  "    async *stream() {",
  "      yield { type: 'status', status: 'RUNNING' };",
  "      yield { type: 'assistant', message: { content: [{ type: 'text', text }] } };",
  "      yield { type: 'status', status: 'FINISHED' };",
  "    },",
  "    async wait() { return { status: 'finished', usage: { inputTokens: 1, outputTokens: 1, totalTokens: 2 } }; },",
  "    async cancel() {},",
  "  };",
  "}",
  "function makeAgent(id) {",
  "  return { agentId: id, closed: false, close() {}, send(input, options) { rec({ call: 'send', options: options ?? null }); return Promise.resolve(makeRun(typeof input === 'string' ? input : input.text)); } };",
  "}",
  "export const Agent = {",
  "  create(options) { rec({ call: 'create', options: options ?? null }); return Promise.resolve(makeAgent('rec-1')); },",
  "  resume(agentId, options) { rec({ call: 'resume', agentId, options: options ?? null }); return Promise.resolve(makeAgent(agentId)); },",
  "};",
  "export const Cursor = { models: { list(options) { rec({ call: 'models.list', options: options ?? null }); return Promise.resolve([{ id: 'rec-default' }]); } } };",
].join("\n");

function writeRecordingFakeSdk() {
  const dir = mkdtempSync(join(tmpdir(), "cursor-rec-sdk-"));
  const sdkPath = join(dir, "fake_cursor_sdk_recording.mjs");
  writeFileSync(sdkPath, RECORDING_FAKE_SDK);
  return { sdkPath, recordPath: join(dir, "calls.jsonl") };
}

function spawnRecordingSidecar({ sdkPath, recordPath }) {
  const { cwd, home } = makeSandbox();
  const state = { lines: [], stderrText: "", buffer: "" };
  const proc = spawn(process.execPath, [
    SIDECAR,
    "--sdk-module",
    sdkPath,
    "--api-key",
    TEST_SIDECAR_API_KEY,
  ], {
    cwd,
    env: { ...process.env, HOME: home, FAKE_SDK_RECORD: recordPath },
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
    lines: () => state.lines,
    stderr: () => state.stderrText,
    send(command) {
      proc.stdin.write(JSON.stringify({ id: Math.random().toString(16).slice(2), type: command.type, ...command }) + "\n");
    },
    response(type) { return state.lines.map((line) => JSON.parse(line)).find((value) => value.type === "response" && value.command === type); },
    result() { return state.lines.map((line) => JSON.parse(line)).find((value) => value.type === "result"); },
    records() {
      if (!existsSync(recordPath)) return [];
      return readFileSync(recordPath, "utf8").split("\n").filter(Boolean).map((line) => JSON.parse(line));
    },
  };
}

async function until(check, label, timeoutMs = 5000) {
  const startedAt = Date.now();
  while (!check()) {
    if (Date.now() - startedAt > timeoutMs) throw new Error(`timed out waiting for ${label}`);
    await new Promise((resolvePromise) => setTimeout(resolvePromise, 10));
  }
}

function assertNoKeyOnTheWire(sidecar) {
  // No JSONL response line ever contains the key string.
  for (const line of sidecar.lines()) {
    assert.ok(!line.includes(TEST_SIDECAR_API_KEY), `key material reached stdout: ${line}`);
  }
  // And no protocol line carries an apiKey field at all.
  for (const value of sidecar.lines().map((line) => JSON.parse(line))) {
    assert.ok(!JSON.stringify(value).includes("apiKey"), `protocol line exposes an apiKey field: ${JSON.stringify(value)}`);
  }
  // log/stderr output never contains it.
  assert.ok(!sidecar.stderr().includes(TEST_SIDECAR_API_KEY), `key material reached stderr: ${sidecar.stderr()}`);
}

test("init resolves the key and it reaches create, models.list, and resume SDK options", async () => {
  const { sdkPath, recordPath } = writeRecordingFakeSdk();
  const sidecar = spawnRecordingSidecar({ sdkPath, recordPath });
  try {
    // Create path.
    sidecar.send({ type: "init", cwd: "/tmp", model: { id: "rec-default" } });
    await until(() => sidecar.response("init") !== undefined, "create init response");
    const createInit = sidecar.response("init");
    assert.equal(createInit.success, true);
    assert.equal(createInit.data.sessionId, "rec-1");

    let records = sidecar.records();
    const create = records.find((entry) => entry.call === "create");
    assert.ok(create, `fake SDK never recorded a create call; got: ${JSON.stringify(records)}`);
    assert.equal(create.options.apiKey, TEST_SIDECAR_API_KEY, "Agent.create options must carry the resolved key");

    const list = records.find((entry) => entry.call === "models.list");
    assert.ok(list, `fake SDK never recorded a models.list call; got: ${JSON.stringify(records)}`);
    assert.equal(list.options.apiKey, TEST_SIDECAR_API_KEY, "Cursor.models.list options must carry the resolved key");

    // The prompt/send path runs (the key was bound at create; SendOptions
    // carries no credential of its own).
    sidecar.send({ type: "prompt", message: "hello" });
    await until(() => sidecar.result() !== undefined, "finished result line");
    assert.equal(sidecar.result().status, "finished");
    const send = sidecar.records().find((entry) => entry.call === "send");
    assert.ok(send, "fake SDK never recorded a send call");

    assertNoKeyOnTheWire(sidecar);
  } finally {
    sidecar.proc.kill();
  }

  // Resume path: a fresh sidecar resuming an existing agent.
  const second = spawnRecordingSidecar({ sdkPath, recordPath });
  try {
    second.send({ type: "init", cwd: "/tmp", model: { id: "rec-default" }, resume: "agent-resumed-7" });
    await until(() => second.response("init") !== undefined, "resume init response");
    const resumeInit = second.response("init");
    assert.equal(resumeInit.success, true);
    assert.equal(resumeInit.data.sessionId, "agent-resumed-7");

    const records = second.records();
    const resume = records.find((entry) => entry.call === "resume" && entry.agentId === "agent-resumed-7");
    assert.ok(resume, `fake SDK never recorded the resume call; got: ${JSON.stringify(records)}`);
    assert.equal(resume.options.apiKey, TEST_SIDECAR_API_KEY, "Agent.resume options must carry the resolved key");
    assert.equal(resume.options.model.id, "rec-default", "resume still passes the model");

    const list = records.find((entry) => entry.call === "models.list");
    assert.ok(list && list.options.apiKey === TEST_SIDECAR_API_KEY, "models.list after resume must carry the key too");

    assertNoKeyOnTheWire(second);
  } finally {
    second.proc.kill();
  }
});
