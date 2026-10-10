// Throwaway live smoke (plan item C8): drive the PRODUCTION Claude SDK sidecar
// (crates/ajax-web/sidecar/claude_sdk_sidecar.mjs) exactly the production way:
//   node --input-type=module -e <file contents>      (no extra args)
// so it resolves the REAL @anthropic-ai/claude-agent-sdk on its own.
// Read-only spike: no repository source is edited. Transcript -> claude-sidecar-live.jsonl.
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { spawn } from "node:child_process";
import { randomUUID } from "node:crypto";
import { fileURLToPath } from "node:url";

const DIR = path.dirname(fileURLToPath(import.meta.url));
const SIDECAR_FILE = path.resolve(DIR, "..", "..", "..", "crates", "ajax-web", "sidecar", "claude_sdk_sidecar.mjs");
const OUT = path.join(DIR, "claude-sidecar-live.jsonl");
writeFileSync(OUT, "");

const out = (obj) => appendFileSync(OUT, JSON.stringify(obj) + "\n");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

class Sidecar {
  constructor(scenario, deadlineMs = 60_000) {
    this.scenario = scenario;
    this.deadlineMs = deadlineMs;
    this.lines = [];
    this.waiters = [];
    this.stderrLines = [];
    this.stdoutBuf = "";
    this.deadlineAt = Date.now() + deadlineMs;
    const code = readFileSync(SIDECAR_FILE, "utf8");
    out({ scenario, kind: "driver.scenario_start", ts: Date.now(), spawn: ["node", "--input-type=module", "-e", "<" + SIDECAR_FILE + ">"] });
    this.child = spawn("node", ["--input-type=module", "-e", code], { cwd: DIR });
    let errBuf = "";
    this.child.stderr.setEncoding("utf8");
    this.child.stderr.on("data", (chunk) => {
      errBuf += chunk;
      let nl;
      while ((nl = errBuf.indexOf("\n")) >= 0) {
        const l = errBuf.slice(0, nl).trimEnd();
        errBuf = errBuf.slice(nl + 1);
        if (l) this.stderrLines.push(l);
      }
    });
    this.child.stdout.setEncoding("utf8");
    this.child.stdout.on("data", (chunk) => {
      this.stdoutBuf += chunk;
      let nl;
      while ((nl = this.stdoutBuf.indexOf("\n")) >= 0) {
        const l = this.stdoutBuf.slice(0, nl);
        this.stdoutBuf = this.stdoutBuf.slice(nl + 1);
        if (l.trim().length === 0) continue;
        let parsed;
        try { parsed = JSON.parse(l); } catch { parsed = null; }
        const rec = parsed !== null
          ? { scenario, kind: "sidecar.line", data: parsed }
          : { scenario, kind: "sidecar.unparsed", raw: l };
        out(rec);
        this.lines.push(rec.data ?? rec.raw);
        const w = this.waiters.shift();
        if (w) w(this.scenarioRecord(rec));
      }
    });
    this.child.on("error", (e) => {
      out({ scenario, kind: "driver.spawn_error", message: String(e?.message ?? e) });
    });
    this.exitPromise = new Promise((res) => this.child.once("exit", (code, sig) => res({ code, sig })));
  }

  scenarioRecord(rec) { return rec; }

  send(obj) {
    if (!this.child.stdin.writable) throw new Error("stdin not writable");
    this.child.stdin.write(JSON.stringify(obj) + "\n");
  }

  // Resolve with the next stdout line, enforcing the scenario deadline.
  async next() {
    const remaining = this.deadlineAt - Date.now();
    if (remaining <= 0) throw new Error(`scenario ${this.scenario} deadline exceeded`);
    if (this.lines.length) return this.lines.shift();
    return await Promise.race([
      new Promise((res) => this.waiters.push(res)),
      new Promise((_, rej) => setTimeout(() => rej(new Error(`scenario ${this.scenario} deadline exceeded waiting for line`)), remaining, { unref: true })),
    ]);
  }

  stderrTail(n = 20) { return this.stderrLines.slice(-n); }

  async shutdownAndExit() {
    try { this.send({ id: "9", type: "shutdown" }); } catch (e) { out({ scenario: this.scenario, kind: "driver.shutdown_send_error", message: String(e?.message ?? e) }); }
    const remaining = Math.max(1_000, this.deadlineAt - Date.now());
    const exit = await Promise.race([this.exitPromise, sleep(remaining).then(() => null)]);
    if (exit === null) { out({ scenario: this.scenario, kind: "driver.kill_after_shutdown", note: "child did not exit in time" }); this.child.kill("SIGKILL"); return { code: null, sig: "killed" }; }
    return exit;
  }

  kill() { try { this.child.kill("SIGKILL"); } catch {} }
}

const summarize = (arr) => ({
  count: Array.isArray(arr) ? arr.length : null,
  first3: Array.isArray(arr) ? arr.slice(0, 3).map((x) => (typeof x === "string" ? x : x?.name ?? x?.value ?? JSON.stringify(x))) : null,
});

async function runScenario(name, fn) {
  const s = new Sidecar(name);
  try {
    await fn(s);
    out({ scenario: name, kind: "driver.scenario_done", ts: Date.now() });
  } catch (e) {
    out({ scenario: name, kind: "driver.error", message: String(e?.message ?? e), stack: e?.stack ? String(e.stack).split("\n").slice(0, 6) : null });
    s.kill();
    await s.exitPromise.catch(() => {});
  } finally {
    out({ scenario: name, kind: "driver.stderr_tail", tail: s.stderrTail() });
  }
  return s;
}

const results = {};

async function main() {
  const cwd = DIR; // init cwd for all scenarios (identical across A and B)

  // ---- Scenario A: fresh init + trivial prompt ----------------------------
  await runScenario("A", async (s) => {
    const t0 = Date.now();
    s.send({ id: "1", type: "init", cwd, model: "haiku" });
    out({ scenario: "A", kind: "driver.init_sent", cwd });
    let initResp = null;
    while (!(initResp && initResp.type === "response" && initResp.command === "init")) {
      const line = await s.next();
      if (line && line.type === "response" && line.command === "init") initResp = line;
    }
    results.a_init_latency_ms = Date.now() - t0;
    out({ scenario: "A", kind: "driver.init_recorded", success: initResp.success, sessionId: initResp.data?.sessionId ?? null, models: summarize(initResp.data?.models), commands: summarize(initResp.data?.commands), initialization_present: initResp.data ? "initialization" in initResp.data : null, latency_ms: results.a_init_latency_ms });
    if (!initResp.success) { out({ scenario: "A", kind: "driver.blocked", reason: "init failed", error: initResp.error }); throw new Error("scenario A blocked at init: " + initResp.error); }
    const sessionId = initResp.data.sessionId;
    results.a_session_id = sessionId;
    s.send({ id: "2", type: "prompt", message: "Reply with the single word ok" });
    out({ scenario: "A", kind: "driver.prompt_sent", message: "Reply with the single word ok" });
    let result = null;
    while (!result) {
      const line = await s.next();
      if (line && line.type === "result") result = line;
    }
    const usage = result.usage ?? {};
    results.a_result = { subtype: result.subtype ?? null, is_error: result.is_error ?? null };
    out({ scenario: "A", kind: "driver.result_recorded", subtype: result.subtype ?? null, is_error: result.is_error ?? null, usage_input_tokens: usage.input_tokens ?? null, usage_output_tokens: usage.output_tokens ?? null, modelUsage_keys_with_contextWindow: Object.fromEntries(Object.entries(result.modelUsage ?? {}).map(([k, v]) => [k, { contextWindow: v?.contextWindow ?? null, inputTokens: v?.input_tokens ?? null, outputTokens: v?.output_tokens ?? null }])), text_head: String(result.result ?? "").slice(0, 80) });
    await s.shutdownAndExit();
  });

  if (results.a_session_id === undefined) {
    out({ kind: "driver.stopped_after_A", reason: results.a_init_blocked ? "scenario A init failed; auth or SDK unavailable" : "scenario A did not produce a session id" });
    out({ kind: "driver.finished", completed: ["A"] });
    process.exit(results.a_session_id === null ? 2 : 0);
  }

  // ---- Scenario B: NEW child, resume A's sessionId -------------------------
  await runScenario("B", async (s) => {
    const t0 = Date.now();
    s.send({ id: "1", type: "init", cwd, model: "haiku", resume: results.a_session_id });
    out({ scenario: "B", kind: "driver.init_sent", cwd, resume: results.a_session_id });
    let initResp = null;
    while (!(initResp && initResp.type === "response" && initResp.command === "init")) {
      const line = await s.next();
      if (line && line.type === "response" && line.command === "init") initResp = line;
    }
    results.b_init_latency_ms = Date.now() - t0;
    out({ scenario: "B", kind: "driver.init_recorded", success: initResp.success, sessionId: initResp.data?.sessionId ?? null, error: initResp.error ?? null, latency_ms: results.b_init_latency_ms });
    if (!initResp.success) { out({ scenario: "B", kind: "driver.resume_rejected", error: initResp.error }); return; }
    s.send({ id: "2", type: "prompt", message: "What single word did you reply earlier?" });
    out({ scenario: "B", kind: "driver.prompt_sent" });
    let result = null;
    while (!result) {
      const line = await s.next();
      if (line && line.type === "result") result = line;
    }
    out({ scenario: "B", kind: "driver.result_recorded", subtype: result.subtype ?? null, is_error: result.is_error ?? null, text: String(result.result ?? "").slice(0, 200) });
    await s.shutdownAndExit();
  });

  // ---- Scenario C: NEW child, resume a random UUID (expected failure) ------
  const fake = randomUUID();
  await runScenario("C", async (s) => {
    s.send({ id: "1", type: "init", cwd, model: "haiku", resume: fake });
    out({ scenario: "C", kind: "driver.init_sent", cwd, resume: fake });
    let initResp = null;
    while (!(initResp && initResp.type === "response" && initResp.command === "init")) {
      const line = await s.next();
      if (line && line.type === "response" && line.command === "init") initResp = line;
    }
    out({ scenario: "C", kind: "driver.init_recorded", success: initResp.success, error: initResp.error ?? null, expected_failure: !initResp.success && String(initResp.error ?? "").includes("session not found") });
    await s.shutdownAndExit();
  });

  // ---- Scenario D (only if A succeeded): interrupt then reuse --------------
  if (results.a_result) {
    await runScenario("D", async (s) => {
      s.send({ id: "1", type: "init", cwd, model: "haiku" });
      let initResp = null;
      while (!(initResp && initResp.type === "response" && initResp.command === "init")) {
        const line = await s.next();
        if (line && line.type === "response" && line.command === "init") initResp = line;
      }
      if (!initResp.success) throw new Error("D init failed: " + initResp.error);
      s.send({ id: "2", type: "prompt", message: "Count slowly from 1 to 100, one number per line" });
      out({ scenario: "D", kind: "driver.prompt_sent" });
      let streamEvents = 0;
      let aborted = false;
      let firstResult = null;
      let secondSent = false;
      while (!secondSent || !firstResult) {
        const line = await s.next();
        if (line && line.type === "stream_event") streamEvents += 1;
        if (!aborted && streamEvents >= 3) {
          aborted = true;
          out({ scenario: "D", kind: "driver.abort_sent", at_stream_events: streamEvents });
          s.send({ id: "3", type: "abort" });
        }
        if (line && line.type === "result" && !firstResult) {
          firstResult = line;
          out({ scenario: "D", kind: "driver.interrupted_result_recorded", subtype: firstResult.subtype ?? null, terminal_reason: firstResult.terminal_reason ?? null, is_error: firstResult.is_error ?? null });
          s.send({ id: "4", type: "prompt", message: "Reply with the single word ok" });
          out({ scenario: "D", kind: "driver.second_prompt_sent" });
        }
        if (line && line.type === "result" && firstResult) {
          out({ scenario: "D", kind: "driver.second_result_recorded", subtype: line.subtype ?? null, is_error: line.is_error ?? null, text_head: String(line.result ?? "").slice(0, 80) });
          secondSent = true;
        }
      }
      await s.shutdownAndExit();
    });
  }

  out({ kind: "driver.finished", summary: { a_session_id: results.a_session_id ?? null, a_init_latency_ms: results.a_init_latency_ms ?? null, b_init_latency_ms: results.b_init_latency_ms ?? null, a_result: results.a_result ?? null } });
}

main().then(() => process.exit(0)).catch((e) => { out({ kind: "driver.fatal", message: String(e?.message ?? e) }); process.exit(1); });
