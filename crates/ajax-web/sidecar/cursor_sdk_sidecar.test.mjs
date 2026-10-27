import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";

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
    this.proc = spawn(process.execPath, [SIDECAR, "--sdk-module", FAKE_SDK], {
      cwd: this.cwd,
      env: { ...process.env, HOME: this.home },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.stdoutLines = [];
    this.consumedResponses = {};
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
    for (const line of this.stdoutLines) JSON.parse(line);
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

    const rawToolLines = sidecar.stdoutLines.filter((line) => line.includes("call-1"));
    assert.equal(rawToolLines.length, 2);
    for (const line of rawToolLines) {
      const parsed = JSON.parse(line);
      assert.equal(parsed.call_id, "call-1\nfc_1");
      assert.ok(line.includes('"call-1\\nfc_1"'), `newline not escaped within one line: ${line}`);
    }

    assert.equal(sidecar.results().length, 1);
    assert.equal(result.status, "finished");
    assert.equal(result.error, undefined);
    assert.deepEqual(result.usage, { inputTokens: 1, outputTokens: 2, totalTokens: 3 });

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
    assert.deepEqual(
      initResponse.data.commands.map((command) => command.name),
      ["greet", "onlyuser"],
    );

    sidecar.send({ type: "prompt", message: "/greet extra words" });
    await sidecar.readResponse("prompt");
    let result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const assistantText = sidecar.events().find((message) => message.type === "assistant").message.content[0].text;
    assert.equal(assistantText, "echo[default]: GREET PROJECT BODY\n\nextra words");

    sidecar.send({ type: "prompt", message: "/nope stay" });
    await sidecar.readResponse("prompt");
    result = await sidecar.readResult();
    assert.equal(result.status, "finished");
    const events = sidecar.events().slice(7);
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

    await new Promise((resolvePromise) => setTimeout(resolvePromise, 100));
    sidecar.send({ type: "abort" });
    const abortResponse = await sidecar.readResponse("abort");
    assert.equal(abortResponse.success, true);

    const result = await sidecar.readResult();
    assert.equal(result.status, "cancelled");
    assert.ok(sidecar.events().some((message) => message.type === "status" && message.status === "CANCELLED"));

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

    sidecar.send({ type: "prompt", message: "any" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, false);
    assert.match(promptResponse.error, /not initialized/);

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

    sidecar.send({ type: "prompt", message: "LATE SLOW" });
    const promptResponse = await sidecar.readResponse("prompt");
    assert.equal(promptResponse.success, true);

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

    sidecar.send({ type: "prompt", message: "bad images", images: [{ nope: 1 }] });
    const badResponse = await sidecar.readResponse("prompt");
    assert.equal(badResponse.success, false);
    assert.match(badResponse.error, /prompt images must be \{url\} or \{data,mimeType\}/);

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
