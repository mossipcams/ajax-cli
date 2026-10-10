#!/usr/bin/env node
// Throwaway driver for the pi RPC protocol spike.
// Spawns `pi --mode rpc` as a child process, drives it with JSONL commands on
// stdin, and records every stdout line to the .jsonl evidence files listed in
// pi-rpc-findings.md. Not part of any product; do not reuse.
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SPIKE = path.dirname(fileURLToPath(import.meta.url));
const SESSIONS = path.join(SPIKE, "sessions");
fs.mkdirSync(SESSIONS, { recursive: true });

const PI_BIN = process.env.PI_BIN ?? "pi";
const BASE_ARGS = ["--mode", "rpc", "--session-dir", SESSIONS, "-na"];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class PiRpc {
  constructor(name, extraArgs) {
    this.name = name;
    this.rawLines = [];
    this.events = [];
    this.stderr = "";
    this.exitInfo = null;
    this.spawnError = null;
    this.proc = spawn(PI_BIN, [...BASE_ARGS, ...extraArgs], {
      cwd: SPIKE,
      stdio: ["pipe", "pipe", "pipe"],
      env: process.env,
    });
    let buf = Buffer.alloc(0);
    this.proc.stdout.on("data", (chunk) => {
      buf = Buffer.concat([buf, chunk]);
      let idx;
      while ((idx = buf.indexOf(0x0a)) !== -1) {
        const line = buf.subarray(0, idx).toString("utf8");
        buf = buf.subarray(idx + 1);
        const rec = line.endsWith("\r") ? line.slice(0, -1) : line;
        if (rec.trim().length === 0) continue;
        this.rawLines.push(rec);
        try {
          this.events.push(JSON.parse(rec));
        } catch {
          this.parseFailures.push(rec);
        }
      }
    });
    this.proc.stderr.on("data", (c) => { this.stderr += c.toString("utf8"); });
    this.proc.on("exit", (code, signal) => { this.exitInfo = { code, signal }; });
    this.proc.on("error", (err) => { this.spawnError = err; });
  }

  send(obj) {
    if (this.exitInfo || this.spawnError) {
      throw new Error(`${this.name}: process already gone`);
    }
    this.proc.stdin.write(JSON.stringify(obj) + "\n");
  }

  async waitUntil(pred, label, timeoutMs = 180000) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const ev = this.events.find((e, i) => pred(e, i));
      if (ev) return ev;
      if (this.spawnError) {
        throw new Error(`${this.name}: spawn failed (${this.spawnError.message})`);
      }
      if (this.exitInfo) {
        throw new Error(
          `${this.name}: exited before ${label}; exit=${JSON.stringify(this.exitInfo)}; stderr tail: ${this.stderr.slice(-4000)}`
        );
      }
      if (Date.now() > deadline) {
        throw new Error(
          `${this.name}: timeout waiting for ${label}; last lines: ${this.rawLines.slice(-3).join(" | ").slice(-1000)}`
        );
      }
      await sleep(25);
    }
  }

  response(id, timeoutMs = 180000) {
    return this.waitUntil((e) => e.type === "response" && e.id === id, `response ${id}`, timeoutMs);
  }

  requireSuccess(resp) {
    if (!resp.success) {
      throw new Error(`command ${resp.command} failed: ${JSON.stringify(resp).slice(0, 2000)}`);
    }
    return resp;
  }

  async finish(outfile, { gracefulMs = 20000 } = {}) {
    try { this.proc.stdin.end(); } catch {}
    const deadline = Date.now() + gracefulMs;
    while (!this.exitInfo && Date.now() < deadline) await sleep(50);
    if (!this.exitInfo) {
      this.proc.kill("SIGKILL");
      await sleep(300);
    }
    fs.writeFileSync(path.join(SPIKE, outfile), this.rawLines.join("\n") + "\n");
    fs.writeFileSync(path.join(SPIKE, `stderr-${this.name}.log`), this.stderr);
    return { exit: this.exitInfo, lines: this.rawLines.length };
  }

  kill() { try { this.proc.kill("SIGKILL"); } catch {} }
}

async function ensureModel(r) {
  const st = await r.response("m-state");
  if (st.success && st.data?.model) return st;
  const models = await r.response("m-models");
  if (models.success && Array.isArray(models.data?.models) && models.data.models.length > 0) {
    const m = models.data.models[0];
    const set = await r.response("m-set");
    if (set.success) return { ...st, data: { ...st.data, model: m } };
  }
  throw new Error(`NO USABLE MODEL. get_state=${JSON.stringify(st).slice(0, 800)} stderr=${r.stderr.slice(-3000)}`);
}

// ---------- Run 1: plain prompt through agent_settled ----------
async function runPrompt() {
  const r = new PiRpc("prompt", []);
  r.send({ id: "m-state", type: "get_state" });
  const st0 = await ensureModel(r);
  const sessionId = st0.data.sessionId;
  const sessionFile = st0.data.sessionFile;

  r.send({ id: "p1", type: "prompt", message: "Reply with the single word ok" });
  const resp = r.requireSuccess(await r.response("p1"));
  if (resp.data?.disposition !== "started") {
    throw new Error(`unexpected disposition: ${JSON.stringify(resp.data)}`);
  }
  await r.waitUntil((e) => e.type === "agent_settled", "agent_settled");

  r.send({ id: "p2", type: "get_state" });
  r.requireSuccess(await r.response("p2")); // idle-state evidence: also asserts session still responsive

  const fin = await r.finish("pi-rpc-prompt.jsonl");
  return { sessionId, sessionFile, modelId: st0.data.model?.id, exit: fin.exit, lines: fin.lines };
}

// ---------- Run 2: tool call (bash) ----------
async function runToolcall() {
  const r = new PiRpc("toolcall", []);
  r.send({ id: "m-state", type: "get_state" });
  await ensureModel(r); // aborts the run if no usable model

  r.send({
    id: "t1",
    type: "prompt",
    message: "Run the bash command: echo hi\nThen reply with just: done",
  });
  r.requireSuccess(await r.response("t1"));
  await r.waitUntil((e) => e.type === "agent_settled", "agent_settled");

  const toolEvents = r.events.filter((e) => e.type.startsWith("tool_"));
  const hadTool = toolEvents.length > 0;
  const fin = await r.finish("pi-rpc-toolcall.jsonl");
  if (!hadTool) throw new Error("agent settled without any tool_* events: " + r.rawLines.slice(-6).join(" | "));
  return { toolEvents: toolEvents.map((e) => e.type), exit: fin.exit, lines: fin.lines };
}

// ---------- Run 3: abort mid-run ----------
async function runAbort() {
  const r = new PiRpc("abort", []);
  r.send({ id: "m-state", type: "get_state" });
  await ensureModel(r);

  r.send({
    id: "a1",
    type: "prompt",
    message: "Write a short essay of roughly 300 words about ocean tides.",
  });
  r.requireSuccess(await r.response("a1"));

  await r.waitUntil(
    (e) => e.type === "message_update" && e.assistantMessageEvent?.type === "text_delta",
    "first text_delta"
  );
  const streamingAt = r.events.length;

  r.send({ id: "a2", type: "abort" });
  const abortResp = r.requireSuccess(await r.response("a2"));
  const settledAfterAbort = await r.waitUntil(
    (e, i) => e.type === "agent_settled" && i >= streamingAt,
    "agent_settled after abort"
  );

  const fin = await r.finish("pi-rpc-abort.jsonl");
  return { abortResp, settledAfterAbort, exit: fin.exit, lines: fin.lines };
}

// ---------- Run 4: resume the run-1 session in a second process ----------
async function runResume(first) {
  const attempts = [];
  for (const [mode, arg] of [["session-id", first.sessionId], ["session-file", first.sessionFile]]) {
    let r;
    try {
      r = new PiRpc("resume", ["--session", arg]);
      r.send({ id: "r1", type: "get_state" });
      const st = r.requireSuccess(await r.response("r1", 60000));
      if (st.data?.sessionId !== first.sessionId) {
        throw new Error(`resume via ${mode} landed on wrong session: got ${JSON.stringify(st.data?.sessionId)} want ${first.sessionId}`);
      }
      // Discovery commands, in findings order.
      r.send({ id: "r2", type: "get_messages" });
      r.requireSuccess(await r.response("r2"));
      r.send({ id: "r3", type: "get_available_models" });
      r.requireSuccess(await r.response("r3"));
      r.send({ id: "r4", type: "get_available_thinking_levels" });
      r.requireSuccess(await r.response("r4"));
      r.send({ id: "r5", type: "get_commands" });
      r.requireSuccess(await r.response("r5"));
      r.send({ id: "r6", type: "get_session_stats" });
      r.requireSuccess(await r.response("r6"));

      const promptRespIdx = r.events.length;
      r.send({
        id: "r7",
        type: "prompt",
        message: "In one short sentence, what exact request did I make earlier in this session?",
      });
      r.requireSuccess(await r.response("r7"));
      await r.waitUntil((e, i) => e.type === "agent_settled" && i > promptRespIdx, "agent_settled after resume prompt");

      const fin = await r.finish("pi-rpc-resume.jsonl");
      return { mode, exit: fin.exit, lines: fin.lines };
    } catch (err) {
      attempts.push({ mode, error: String(err.message ?? err) });
      if (r) {
        fs.writeFileSync(
          path.join(SPIKE, `pi-rpc-resume-attempt-${mode}.jsonl`),
          (r.rawLines ?? []).join("\n") + "\n"
        );
        r.kill();
      }
    }
  }
  throw new Error("resume failed both ways: " + JSON.stringify(attempts, null, 2));
}

// ---------- main ----------
const log = (...a) => console.log(`[driver]`, ...a);

try {
  log("run1: prompt run starting");
  const first = await runPrompt();
  log("run1 done:", JSON.stringify(first));

  log("run2: toolcall run starting");
  const second = await runToolcall();
  log("run2 done:", JSON.stringify(second));

  log("run3: abort run starting");
  const third = await runAbort();
  log("run3 done:", JSON.stringify(third));

  log("run4: resume run starting");
  const fourth = await runResume(first);
  log("run4 done:", JSON.stringify(fourth));

  fs.writeFileSync(
    path.join(SPIKE, "driver-summary.json"),
    JSON.stringify({ first, second, third, fourth }, null, 2)
  );
  log("ALL RUNS COMPLETE");
} catch (err) {
  console.error("[driver] FAILURE:", err.message ?? err);
  process.exitCode = 1;
}
