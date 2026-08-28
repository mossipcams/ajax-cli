import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");

test("selectMission picks primary and fallback with change-aware rotation", async () => {
  const { selectMission, MISSIONS } = await import("./exploratory/missions.mjs");
  const plan = selectMission({
    memory: {
      version: 1,
      lastRunSha: "aaa1111",
      confirmedFindings: [{ fingerprint: "navigation|hash-garbage" }],
      missions: {
        "garbage-hashes": { lastRunAt: "2026-01-01T00:00:00.000Z", runs: 5 },
        "happy-path-session": { lastRunAt: "2026-01-02T00:00:00.000Z", runs: 1 },
        "terminal-input": { lastRunAt: null, runs: 0 },
      },
    },
    headSha: "bbb2222",
    changedPaths: ["crates/ajax-web/web/src/features/chat/session/transport.ts"],
    changedCommits: ["bbb2222 fix(web): session reconnect"],
  });
  assert.ok(plan.primary && plan.fallback);
  assert.notEqual(plan.primary.id, plan.fallback.id);
  assert.equal(plan.primary.id, "happy-path-session");
  assert.ok(MISSIONS.some((m) => m.id === plan.fallback.id));
  assert.notEqual(plan.primary.id, "garbage-hashes");
});

test("selectMission avoids missions matching known confirmed fingerprints", async () => {
  const { selectMission } = await import("./exploratory/missions.mjs");
  const plan = selectMission({
    memory: {
      lastRunSha: null,
      confirmedFindings: [{ fingerprint: "navigation|banner-stuck" }],
      missions: {},
    },
    headSha: "ccc3333",
    changedPaths: [],
    changedCommits: [],
  });
  assert.notEqual(plan.primary.id, "garbage-hashes");
  assert.notEqual(plan.fallback.id, "garbage-hashes");
});

test("preflightFakeAcp proves initialize, session/new, and prompt on agent stub", async () => {
  const { preflightFakeAcp } = await import("./exploratory/preflight-fake-acp.mjs");
  const result = await preflightFakeAcp({ root });
  assert.equal(result.status, "ok", result.error ?? JSON.stringify(result));
  assert.match(result.fixturePath, /fake_acp\.js$/);
  assert.match(result.agentPath, /agent-stubs\/agent$/);
});

test("preflightFakeAcp fails cleanly when fixture missing", async () => {
  const { preflightFakeAcp } = await import("./exploratory/preflight-fake-acp.mjs");
  const result = await preflightFakeAcp({ root: join(tmpdir(), "missing-root") });
  assert.equal(result.status, "blocked");
  assert.ok(result.error);
});

test("validateFindingsDocument requires two reproduction successes for confirmed", async () => {
  const { validateFindingsDocument } = await import("./exploratory/lib.mjs");
  const problems = validateFindingsDocument({
    version: 1,
    findings: [{
      id: "x", title: "Broken", status: "confirmed", confidence: "high", area: "cockpit",
      severity: "high", reproductionAttempts: 2, reproductionSuccesses: 1, steps: ["open app"],
      expected: "works", actual: "fails",
      evidence: { screenshots: ["exploratory-results/screenshots/x.png"] },
      fingerprint: "cockpit|broken",
    }],
  });
  assert.ok(problems.some((p) => p.includes("2 successful reproduction")));
});

test("normalizeFinding does not fabricate reproduction successes", async () => {
  const { normalizeFinding } = await import("./exploratory/lib.mjs");
  const normalized = normalizeFinding({
    id: "x", title: "Broken", status: "confirmed", area: "cockpit", severity: "high",
    reproSteps: ["step one"], expected: "works", actual: "fails", evidence: [],
  });
  assert.equal(normalized.reproductionSuccesses, 0);
});

test("classifyFinding marks novel, known, regression from open/closed issues", async () => {
  const { classifyFindings } = await import("./exploratory/classify-findings.mjs");
  const classified = classifyFindings({
    version: 1,
    findings: [
      { id: "a", status: "confirmed", fingerprint: "navigation|banner-stuck", title: "Banner stuck", area: "navigation", relatedIssues: [999] },
      { id: "b", status: "confirmed", fingerprint: "session|composer-pending", title: "Composer pending", area: "session" },
      { id: "c", status: "confirmed", fingerprint: "terminal|paste-broken", title: "Paste broken", area: "terminal" },
      { id: "d", status: "confirmed", fingerprint: "settings|focus-trap", title: "Focus trap", area: "settings" },
    ],
  }, {
    openBugs: [{ number: 42, title: "[defect] Web Cockpit banner stuck", body: "<!-- exploratory-fingerprint: navigation|banner-stuck -->" }],
    closedBugs: [{ number: 99, title: "[defect] Web Cockpit paste broken", body: "<!-- exploratory-fingerprint: terminal|paste-broken -->" }],
    memory: { confirmedFindings: [{ fingerprint: "session|composer-pending" }] },
  });
  assert.deepEqual(classified.findings.map((f) => f.classification), ["known", "known", "regression", "novel"]);
});

test("assessRunUsefulness rejects empty skeleton and blocked preflight", async () => {
  const { assessRunUsefulness } = await import("./exploratory/lib.mjs");
  const cases = [
    [{ run: { preflight: { status: "blocked", error: "fixture missing" }, agent: { status: "completed" } }, memoryDelta: { areasVisited: ["cockpit"] }, findings: { findings: [] }, observations: { observations: [] } }, false],
    [{ run: { preflight: { status: "ok" }, agent: { status: "completed" } }, memoryDelta: { areasVisited: [] }, findings: { findings: [] }, observations: { observations: [] } }, false],
    [{ run: { preflight: { status: "skipped" }, agent: { status: "completed" } }, memoryDelta: { areasVisited: ["navigation"] }, findings: { findings: [] }, observations: { observations: [{ summary: "probed hashes" }] }, missionDoc: { primary: { id: "garbage-hashes", area: "navigation" } } }, true],
    [{ run: { preflight: { status: "skipped" }, agent: { status: "completed" }, mission: { primary: "happy-path-session" } }, memoryDelta: { areasVisited: [], notes: "nothing useful" }, findings: { findings: [] }, observations: { observations: [] }, missionDoc: { primary: { id: "happy-path-session", area: "session" } } }, false],
  ];
  for (const [input, expected] of cases) {
    assert.equal(assessRunUsefulness(input).ok, expected);
  }
});

test("plan-mission script writes mission.json", () => {
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-plan-"));
  const memoryFile = join(resultsDir, "memory.json");
  writeFileSync(memoryFile, JSON.stringify({ version: 1, lastRunSha: null, missions: {}, confirmedFindings: [] }));
  writeFileSync(join(resultsDir, "run.json"), JSON.stringify({ version: 1, headSha: "deadbeef" }));
  const result = spawnSync(process.execPath, ["scripts/exploratory/plan-mission.mjs"], {
    cwd: root,
    encoding: "utf8",
    env: { ...process.env, AJAX_EXPLORATORY_RESULTS: resultsDir, AJAX_EXPLORATORY_MEMORY: memoryFile },
  });
  assert.equal(result.status, 0, result.stderr);
  const mission = JSON.parse(readFileSync(join(resultsDir, "mission.json"), "utf8"));
  assert.ok(mission.primary?.id && mission.fallback?.id);
});
