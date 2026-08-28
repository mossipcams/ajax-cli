import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

test("applyVerifierConfirmationGate demotes agent-only confirmed findings", async () => {
  const { applyVerifierConfirmationGate, hasIndependentVerifierEvidence } = await import(
    "./exploratory/lib.mjs"
  );
  const agentOnly = {
    id: "finding-agent-1",
    title: "Banner stuck after navigation",
    status: "confirmed",
    confidence: "high",
    area: "navigation",
    severity: "medium",
    reproductionAttempts: 2,
    reproductionSuccesses: 2,
    steps: ["open dashboard", "navigate away", "return"],
    expected: "banner clears",
    actual: "banner persists",
    evidence: {
      screenshots: ["exploratory-results/screenshots/agent-banner.png"],
      consoleErrors: [],
      networkFailures: [],
    },
    fingerprint: "navigation|banner-stuck",
  };
  const verifierDoc = { version: 1, verifications: [] };
  assert.equal(hasIndependentVerifierEvidence(agentOnly, verifierDoc), false);
  const gated = applyVerifierConfirmationGate(
    { version: 1, findings: [agentOnly] },
    verifierDoc,
  );
  assert.equal(gated.findings[0].status, "observation");
  assert.equal(gated.findings[0].reproductionSuccesses, 0);

  const verifierDir = join(root, "exploratory-results", "verifier");
  mkdirSync(verifierDir, { recursive: true });
  writeFileSync(join(verifierDir, "finding-agent-1.zip"), "trace\n");
  writeFileSync(join(verifierDir, "finding-agent-1.png"), "shot\n");

  const verified = applyVerifierConfirmationGate(
    { version: 1, findings: [agentOnly] },
    {
      version: 1,
      verifications: [
        {
          findingId: "finding-agent-1",
          source: "deterministic-verifier",
          reproductionSuccesses: 2,
          evidence: {
            trace: "exploratory-results/verifier/finding-agent-1.zip",
            screenshots: ["exploratory-results/verifier/finding-agent-1.png"],
          },
        },
      ],
    },
  );
  assert.equal(verified.findings[0].status, "confirmed");
  assert.equal(verified.findings[0].reproductionSuccesses, 2);

  rmSync(join(verifierDir, "finding-agent-1.zip"));
  rmSync(join(verifierDir, "finding-agent-1.png"));
});

test("classifyFindings skips classification without independent verifier evidence", async () => {
  const { classifyFindings } = await import("./exploratory/classify-findings.mjs");
  const finding = {
    id: "finding-agent-2",
    status: "confirmed",
    fingerprint: "session|composer-pending",
    title: "Composer pending",
    area: "session",
    reproductionSuccesses: 2,
    steps: ["reconnect"],
    expected: "send works",
    actual: "pending",
    evidence: {},
  };
  const withoutVerifier = classifyFindings(
    { version: 1, findings: [finding] },
    { openBugs: [], closedBugs: [], memory: {}, verifierDoc: { version: 1, verifications: [] } },
  );
  assert.equal(withoutVerifier.findings[0].classification, undefined);

  const verifierEvidence = "exploratory-results/verifier/finding-agent-2.png";
  mkdirSync(join(root, "exploratory-results", "verifier"), { recursive: true });
  writeFileSync(join(root, verifierEvidence), "verifier-evidence\n");

  const withVerifier = classifyFindings(
    { version: 1, findings: [finding] },
    {
      openBugs: [],
      closedBugs: [],
      memory: {},
      verifierDoc: {
        version: 1,
        verifications: [
          {
            findingId: "finding-agent-2",
            source: "deterministic-verifier",
            reproductionSuccesses: 2,
            evidence: { screenshots: [verifierEvidence] },
          },
        ],
      },
    },
  );
  assert.equal(withVerifier.findings[0].classification, "novel");
  rmSync(join(root, verifierEvidence));
});

test("classifyFinding marks novel, known, regression from open/closed issues", async () => {
  const { classifyFindings } = await import("./exploratory/classify-findings.mjs");
  const verifierDir = join(root, "exploratory-results", "verifier");
  mkdirSync(verifierDir, { recursive: true });
  for (const id of ["a", "b", "c", "d"]) {
    writeFileSync(join(verifierDir, `${id}.png`), "verifier-evidence\n");
  }
  const verifierDoc = {
    version: 1,
    verifications: [
      { findingId: "a", source: "deterministic-verifier", reproductionSuccesses: 2, evidence: { screenshots: ["exploratory-results/verifier/a.png"] } },
      { findingId: "b", source: "deterministic-verifier", reproductionSuccesses: 2, evidence: { screenshots: ["exploratory-results/verifier/b.png"] } },
      { findingId: "c", source: "deterministic-verifier", reproductionSuccesses: 2, evidence: { screenshots: ["exploratory-results/verifier/c.png"] } },
      { findingId: "d", source: "deterministic-verifier", reproductionSuccesses: 2, evidence: { screenshots: ["exploratory-results/verifier/d.png"] } },
    ],
  };
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
    verifierDoc,
  });
  assert.deepEqual(classified.findings.map((f) => f.classification), ["known", "known", "regression", "novel"]);
  for (const id of ["a", "b", "c", "d"]) {
    rmSync(join(verifierDir, `${id}.png`));
  }
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

const fallbackMissionFixture = {
  version: 1,
  primary: {
    id: "happy-path-session",
    charter: "Happy path",
    area: "session",
    needsFakeAcp: true,
    seed: { kind: "task", repo: "demo", title: "exploratory-test-task-alpha", agent: "cursor" },
  },
  fallback: {
    id: "garbage-hashes",
    charter: "Garbage hashes",
    area: "navigation",
    needsFakeAcp: false,
    seed: null,
  },
};

function writeFallbackFixture(resultsDir, { active = "happy-path-session", fallbackActivated = false } = {}) {
  writeFileSync(join(resultsDir, "mission.json"), JSON.stringify(fallbackMissionFixture));
  writeFileSync(
    join(resultsDir, "run.json"),
    JSON.stringify({
      version: 1,
      mission: {
        primary: "happy-path-session",
        fallback: "garbage-hashes",
        active,
        fallbackActivated,
      },
    }),
  );
}

test("activateFallbackMission promotes fallback and records run metadata", async () => {
  const { activateFallbackMission } = await import("./exploratory/plan-mission.mjs");
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-fallback-"));
  writeFallbackFixture(resultsDir);

  const activated = activateFallbackMission({ results: resultsDir });
  assert.equal(activated.ok, true);
  assert.equal(activated.mission.id, "garbage-hashes");

  const mission = JSON.parse(readFileSync(join(resultsDir, "mission.json"), "utf8"));
  assert.equal(mission.primary.id, "garbage-hashes");
  assert.equal(mission.plannedPrimary.id, "happy-path-session");

  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.mission.active, "garbage-hashes");
  assert.equal(run.mission.fallbackActivated, true);
  assert.equal(run.mission.primary, "happy-path-session");

  rmSync(resultsDir, { recursive: true, force: true });
});

test("runMissionSeedWithFallback activates fallback when primary seed fails", async () => {
  const { runMissionSeedWithFallback } = await import("./exploratory/plan-mission.mjs");
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-seed-fallback-"));
  writeFallbackFixture(resultsDir);
  const seedCalls = [];

  const result = await runMissionSeedWithFallback({
    results: resultsDir,
    seed: async ({ mission }) => {
      seedCalls.push(mission.id);
      if (mission.id === "happy-path-session") {
        return { status: "failed", error: "primary seed broke" };
      }
      return { status: "ok", handle: "demo/fallback-task" };
    },
  });

  assert.equal(result.status, "ok");
  assert.deepEqual(seedCalls, ["happy-path-session", "garbage-hashes"]);
  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.mission.fallbackActivated, true);
  assert.equal(run.mission.active, "garbage-hashes");
  rmSync(resultsDir, { recursive: true, force: true });
});

test("runMissionSeedWithFallback fails when fallback seed also fails", async () => {
  const { runMissionSeedWithFallback } = await import("./exploratory/plan-mission.mjs");
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-seed-fallback-fail-"));
  writeFallbackFixture(resultsDir);

  const result = await runMissionSeedWithFallback({
    results: resultsDir,
    seed: async ({ mission }) => ({
      status: "failed",
      error: `${mission.id} seed failed`,
    }),
  });

  assert.equal(result.status, "failed");
  assert.match(result.error, /garbage-hashes seed failed/);
  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.mission.fallbackActivated, true);
  rmSync(resultsDir, { recursive: true, force: true });
});

test("runPreflightWithFallback retries fallback after primary preflight blocked", async () => {
  const { runPreflightWithFallback } = await import("./exploratory/preflight-fake-acp.mjs");
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-preflight-fallback-"));
  writeFallbackFixture(resultsDir);
  const preflightCalls = [];

  const result = await runPreflightWithFallback({
    results: resultsDir,
    preflight: async ({ mission }) => {
      preflightCalls.push(mission.id);
      if (mission.id === "happy-path-session") {
        return { status: "blocked", error: "primary preflight blocked" };
      }
      return { status: "ok", sessionId: "fallback-session" };
    },
    seed: async ({ mission }) => {
      assert.equal(mission.id, "garbage-hashes");
      return { status: "skipped", reason: "no seed" };
    },
  });

  assert.equal(result.status, "ok");
  assert.deepEqual(preflightCalls, ["happy-path-session", "garbage-hashes"]);
  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.mission.fallbackActivated, true);
  assert.equal(run.mission.active, "garbage-hashes");
  assert.equal(run.preflight.status, "ok");
  rmSync(resultsDir, { recursive: true, force: true });
});

test("runPreflightWithFallback stays blocked when fallback preflight fails", async () => {
  const { runPreflightWithFallback } = await import("./exploratory/preflight-fake-acp.mjs");
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-preflight-fallback-blocked-"));
  writeFallbackFixture(resultsDir);

  const result = await runPreflightWithFallback({
    results: resultsDir,
    preflight: async () => ({ status: "blocked", error: "still blocked" }),
    seed: async () => ({ status: "skipped", reason: "no seed" }),
  });

  assert.equal(result.status, "blocked");
  assert.match(result.error, /still blocked/);
  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.mission.fallbackActivated, true);
  assert.equal(run.preflight.status, "blocked");
  rmSync(resultsDir, { recursive: true, force: true });
});

test("resolveActiveMissionId prefers active mission over planned primary", async () => {
  const { resolveActiveMissionId, missionAreaFromDoc } = await import("./exploratory/lib.mjs");
  const run = {
    mission: {
      primary: "happy-path-session",
      fallback: "garbage-hashes",
      active: "garbage-hashes",
      fallbackActivated: true,
    },
  };
  const missionDoc = {
    primary: { id: "garbage-hashes", area: "navigation" },
    fallback: { id: "garbage-hashes", area: "navigation" },
    plannedPrimary: { id: "happy-path-session", area: "session" },
  };
  assert.equal(resolveActiveMissionId(run, missionDoc), "garbage-hashes");
  assert.equal(missionAreaFromDoc(missionDoc, run), "navigation");
});

test("update-memory records active mission for fallback-activated runs", () => {
  const resultsDir = mkdtempSync(join(tmpdir(), "expl-fallback-memory-"));
  const memoryFile = join(resultsDir, "memory.json");
  writeFileSync(
    memoryFile,
    JSON.stringify({
      version: 1,
      updatedAt: null,
      lastRunSha: null,
      runs: [],
      missions: {
        "happy-path-session": { runs: 1, lastRunAt: null, lastRunSha: null, lastOutcome: null },
        "garbage-hashes": { runs: 0, lastRunAt: null, lastRunSha: null, lastOutcome: null },
      },
      areas: {},
      confirmedFindings: [],
      regressions: [],
      observations: [],
      dullActions: [],
    }),
  );
  writeFileSync(
    join(resultsDir, "memory-delta.json"),
    JSON.stringify({
      version: 1,
      areasVisited: ["navigation"],
      dullActions: [],
      confirmedFindingFingerprints: [],
      recommendedFocus: [],
      notes: "fallback mission completed",
    }),
  );
  writeFileSync(join(resultsDir, "findings.json"), JSON.stringify({ version: 1, findings: [] }));
  writeFileSync(join(resultsDir, "observations.json"), JSON.stringify({ version: 1, observations: [] }));
  writeFileSync(join(resultsDir, "oracles.json"), JSON.stringify({ closedBugs: [] }));
  writeFileSync(
    join(resultsDir, "mission.json"),
    JSON.stringify({
      primary: { id: "garbage-hashes", area: "navigation" },
      fallback: { id: "garbage-hashes", area: "navigation" },
      plannedPrimary: { id: "happy-path-session", area: "session" },
    }),
  );
  writeFileSync(
    join(resultsDir, "run.json"),
    JSON.stringify({
      version: 1,
      headSha: "fallbacksha1",
      mission: {
        primary: "happy-path-session",
        fallback: "garbage-hashes",
        active: "garbage-hashes",
        fallbackActivated: true,
      },
    }),
  );

  const result = spawnSync(process.execPath, ["scripts/exploratory/update-memory.mjs"], {
    cwd: root,
    encoding: "utf8",
    env: {
      ...process.env,
      AJAX_EXPLORATORY_RESULTS: resultsDir,
      AJAX_EXPLORATORY_MEMORY: memoryFile,
    },
  });
  assert.equal(result.status, 0, result.stderr);

  const memory = JSON.parse(readFileSync(memoryFile, "utf8"));
  assert.equal(memory.runs.at(-1).mission, "garbage-hashes");
  assert.equal(memory.missions["garbage-hashes"].runs, 1);
  assert.equal(memory.missions["happy-path-session"].runs, 1);
  rmSync(resultsDir, { recursive: true, force: true });
});
