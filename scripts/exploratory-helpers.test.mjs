import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");

test("simulated finding validates and fixture mode writes artifacts", () => {
  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/validate-run.mjs", "--fixture", "--skip-readonly"],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  const findings = JSON.parse(
    readFileSync(join(root, "exploratory-results", "findings.json"), "utf8"),
  );
  assert.equal(findings.version, 1);
  assert.equal(findings.findings[0].status, "confirmed");
  assert.equal(findings.findings[0].reproductionSuccesses, 2);
});

test("validateFindingsDocument rejects confirmed without reproduction", async () => {
  const { validateFindingsDocument } = await import("./exploratory/lib.mjs");
  const problems = validateFindingsDocument({
    version: 1,
    findings: [
      {
        id: "x",
        title: "Broken",
        status: "confirmed",
        confidence: "high",
        area: "cockpit",
        severity: "high",
        reproductionAttempts: 1,
        reproductionSuccesses: 0,
        steps: ["open app"],
        expected: "works",
        actual: "fails",
        evidence: {},
      },
    ],
  });
  assert.ok(problems.some((problem) => problem.includes("2 successful reproduction")));
});

test("normalizeFinding maps agent output to valid schema", async () => {
  const {
    normalizeFinding,
    normalizeFindingsDocument,
    validateFindingsDocument,
  } = await import("./exploratory/lib.mjs");
  const agentFinding = {
    id: "finding-001",
    status: "confirmed",
    fingerprint: "disconnected-404-persists-dashboard",
    title: "Disconnected banner persists on dashboard after visiting nonexistent task URL",
    area: "navigation",
    charter: "Garbage hashes",
    relatedIssues: [835],
    severity: "medium",
    reproSteps: [
      "Open https://127.0.0.1:18790/#/ (clean dashboard, no disconnected banner)",
      "Navigate to https://127.0.0.1:18790/#/t/missing%2Ftask-id",
      "Observe disconnected banner: disconnected: HTTP 404",
      "Return to dashboard via Dashboard nav button or https://127.0.0.1:18790/#/",
      "Banner remains visible on dashboard despite /api/health and /api/cockpit returning 200",
    ],
    expected: "Disconnected banner clears when returning to dashboard with healthy backend",
    actual: "disconnected: HTTP 404 banner persists on dashboard; full page reload clears it",
    evidence: [
      "exploratory-results/screenshots/disconnected-404-persists-dashboard.png",
      "exploratory-results/screenshots/disconnected-404-dashboard-button-click.png",
    ],
    observedAt: "2026-08-12T19:15:57.273Z",
  };

  const normalized = normalizeFinding(agentFinding);
  assert.deepEqual(normalized.steps, agentFinding.reproSteps);
  assert.equal(normalized.confidence, "high");
  assert.equal(normalized.reproductionSuccesses, 0);
  assert.equal(normalized.reproductionAttempts, 1);
  assert.equal(normalized.evidence.screenshots.length, 2);
  assert.deepEqual(normalized.relatedIssues, [835]);
  assert.equal(normalized.charter, undefined);
  assert.equal(normalized.reproSteps, undefined);

  const doc = normalizeFindingsDocument({ version: 1, findings: [agentFinding] });
  const problems = validateFindingsDocument(doc);
  assert.ok(
    problems.some((problem) => problem.includes("2 successful reproduction")),
    problems.join("; "),
  );
});

test("observation without expected/actual normalizes from title", async () => {
  const {
    normalizeFindingsDocument,
    validateFindingsDocument,
  } = await import("./exploratory/lib.mjs");
  const doc = normalizeFindingsDocument({
    version: 1,
    findings: [
      {
        id: "finding-2026-08-18-001",
        title: "Dashboard inline Drop buttons are not clickable",
        status: "observation",
        confidence: "high",
        area: "cockpit",
        severity: "medium",
        reproductionAttempts: 0,
        reproductionSuccesses: 0,
        steps: [],
        evidence: {
          screenshots: ["exploratory-results/screenshots/dashboard-drop-blocked.png"],
          consoleErrors: [],
          networkFailures: [],
        },
        fingerprint: "dashboard-drop-pointer-blocked",
      },
    ],
  });
  assert.match(doc.findings[0].expected, /Not yet characterized/);
  assert.equal(doc.findings[0].actual, "Dashboard inline Drop buttons are not clickable");
  const problems = validateFindingsDocument(doc);
  assert.equal(problems.length, 0, problems.join("; "));
});

test("confirmed without steps demotes to observation", async () => {
  const { normalizeFinding } = await import("./exploratory/lib.mjs");
  const normalized = normalizeFinding({
    id: "x",
    title: "Broken",
    status: "confirmed",
    area: "cockpit",
    severity: "high",
    expected: "works",
    actual: "fails",
    evidence: [],
  });
  assert.equal(normalized.status, "observation");
  assert.equal(normalized.reproductionAttempts, 0);
  assert.equal(normalized.reproductionSuccesses, 0);
  assert.deepEqual(normalized.steps, []);
  const { validateFindingsDocument, normalizeFindingsDocument } = await import(
    "./exploratory/lib.mjs"
  );
  const problems = validateFindingsDocument(
    normalizeFindingsDocument({ version: 1, findings: [normalized] }),
  );
  assert.equal(problems.length, 0, problems.join("; "));
});

test("update-memory merges delta without requiring prior corpus", () => {
  const memoryDir = join(root, "exploratory-memory");
  mkdirSync(memoryDir, { recursive: true });
  mkdirSync(join(root, "exploratory-results"), { recursive: true });
  writeFileSync(
    join(root, "exploratory-results", "memory-delta.json"),
    JSON.stringify({
      version: 1,
      areasVisited: ["settings", "cockpit"],
      dullActions: ["open empty inbox repeatedly"],
      confirmedFindingFingerprints: [],
      recommendedFocus: ["settings"],
      notes: "test",
    }),
  );
  writeFileSync(
    join(root, "exploratory-results", "findings.json"),
    JSON.stringify({
      version: 1,
      findings: [
        {
          id: "a",
          title: "Settings sheet traps focus",
          status: "confirmed",
          confidence: "high",
          area: "settings",
          severity: "medium",
          reproductionAttempts: 2,
          reproductionSuccesses: 2,
          steps: ["open settings", "tab away"],
          expected: "focus returns",
          actual: "focus trap",
          evidence: {},
          fingerprint: "settings|focus-trap",
        },
      ],
    }),
  );
  writeFileSync(
    join(root, "exploratory-results", "run.json"),
    JSON.stringify({ version: 1, headSha: "abc123" }),
  );

  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/update-memory.mjs"],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  const memory = JSON.parse(readFileSync(join(memoryDir, "memory.json"), "utf8"));
  assert.equal(memory.lastRunSha, "abc123");
  assert.ok(memory.areas.settings.visits >= 1);
  assert.equal(memory.confirmedFindings[0].fingerprint, "settings|focus-trap");
});

test("prepare-instance creates isolated config and result skeleton", () => {
  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/prepare-instance.mjs"],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr + result.stdout);
  const env = JSON.parse(
    readFileSync(join(root, "target/exploratory-instance/env.json"), "utf8"),
  );
  assert.equal(env.AJAX_EXPLORATORY_PORT, 18790);
  assert.match(env.config, /config\.toml$/);

  const demo = join(root, "target/exploratory-instance/repos/demo");
  const originUrl = spawnSync("git", ["-C", demo, "remote", "get-url", "origin"], {
    encoding: "utf8",
    env: Object.fromEntries(
      Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")),
    ),
  });
  assert.equal(originUrl.status, 0, originUrl.stderr);
  assert.ok(originUrl.stdout.trim().length > 0);

  const originMain = spawnSync("git", ["-C", demo, "rev-parse", "origin/main"], {
    encoding: "utf8",
    env: Object.fromEntries(
      Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")),
    ),
  });
  assert.equal(originMain.status, 0, originMain.stderr);

  const fetch = spawnSync("git", ["-C", demo, "fetch", "origin", "main"], {
    encoding: "utf8",
    env: Object.fromEntries(
      Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")),
    ),
  });
  assert.equal(fetch.status, 0, fetch.stderr);
});

test("prepare-instance stays isolated when GIT_DIR points at the parent repo", () => {
  const gitDir = spawnSync("git", ["rev-parse", "--git-dir"], {
    cwd: root,
    encoding: "utf8",
  });
  assert.equal(gitDir.status, 0, gitDir.stderr);
  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/prepare-instance.mjs"],
    {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        GIT_DIR: gitDir.stdout.trim(),
        GIT_INDEX_FILE: join(root, ".git-index-should-not-be-used"),
      },
    },
  );
  assert.equal(result.status, 0, result.stderr + result.stdout);
  const demo = join(root, "target/exploratory-instance/repos/demo");
  const nested = spawnSync("git", ["rev-parse", "--show-toplevel"], {
    cwd: demo,
    encoding: "utf8",
    env: Object.fromEntries(
      Object.entries(process.env).filter(([key]) => !key.startsWith("GIT_")),
    ),
  });
  assert.equal(nested.status, 0, nested.stderr);
  assert.equal(nested.stdout.trim(), demo);
});

test("validateFindingsDocument rejects confirmed evidence without existing files", async () => {
  const { validateFindingsDocument } = await import("./exploratory/lib.mjs");
  const problems = validateFindingsDocument({
    version: 1,
    findings: [
      {
        id: "x",
        title: "Broken",
        status: "confirmed",
        confidence: "high",
        area: "cockpit",
        severity: "high",
        reproductionAttempts: 2,
        reproductionSuccesses: 2,
        steps: ["open app"],
        expected: "works",
        actual: "fails",
        evidence: {
          notes: "only notes",
          screenshots: ["exploratory-results/screenshots/missing.png"],
        },
        fingerprint: "cockpit|broken",
      },
    ],
  });
  assert.ok(problems.some((p) => p.includes("missing evidence file")));
});

test("validateFindingsSchema accepts observation with empty steps", async () => {
  const { normalizeFindingsDocument, validateFindingsSchema } = await import("./exploratory/lib.mjs");
  const doc = normalizeFindingsDocument({
    version: 1,
    findings: [
      {
        id: "obs-1",
        title: "Maybe broken",
        status: "observation",
        area: "cockpit",
        severity: "low",
        reproSteps: [],
        expected: "n/a",
        actual: "Maybe broken",
        evidence: {},
      },
    ],
  });
  const problems = validateFindingsSchema(doc);
  assert.equal(problems.length, 0, problems.join("; "));
});

test("computeAgentBudget subtracts finalization reserve from agent timeout", async () => {
  const { computeAgentBudget } = await import("./exploratory/lib.mjs");
  assert.deepEqual(computeAgentBudget({ budgetMinutes: 12, finalizationReserveMinutes: 2 }), {
    budgetMinutes: 12,
    finalizationReserveMinutes: 2,
    explorationMinutes: 10,
    agentTimeoutSeconds: 600,
  });
  assert.deepEqual(computeAgentBudget({ budgetMinutes: 3, finalizationReserveMinutes: 5 }), {
    budgetMinutes: 3,
    finalizationReserveMinutes: 5,
    explorationMinutes: 1,
    agentTimeoutSeconds: 60,
  });
});

test("prepare-prompt stop-after minutes match computeAgentBudget exploration window", () => {
  const resultsDir = mkdtempSync(join(tmpdir(), "ajax-exploratory-prompt-"));
  try {
    writeFileSync(
      join(resultsDir, "oracles.json"),
      JSON.stringify({
        version: 1,
        openBugs: [],
        recentWebCommits: [],
        routes: ["#/"],
        boundaryHashes: [],
        memory: { dullActions: [], recommendedFocus: [], confirmedFingerprints: [] },
      }),
    );
    writeFileSync(
      join(resultsDir, "mission.json"),
      JSON.stringify({
        version: 1,
        headSha: "abc",
        sinceSha: null,
        primary: { id: "garbage-hashes", charter: "Garbage hashes", area: "navigation", needsFakeAcp: false, seed: null },
        fallback: { id: "happy-path-session", charter: "Happy path", area: "session", needsFakeAcp: true, seed: null },
      }),
    );

    const budgetMinutes = 12;
    const finalizationReserveMinutes = 2;
    const result = spawnSync(process.execPath, ["scripts/exploratory/prepare-prompt.mjs"], {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        AJAX_EXPLORATORY_RESULTS: resultsDir,
        AJAX_EXPLORATORY_MEMORY: join(resultsDir, "missing-memory.json"),
        AJAX_EXPLORATORY_BUDGET_MINUTES: String(budgetMinutes),
        AJAX_EXPLORATORY_FINALIZATION_MINUTES: String(finalizationReserveMinutes),
      },
    });
    assert.equal(result.status, 0, result.stderr);

    const meta = JSON.parse(readFileSync(join(resultsDir, "prompt-meta.json"), "utf8"));
    const prompt = readFileSync(join(resultsDir, "prompt.txt"), "utf8");
    assert.equal(meta.explorationMinutes, budgetMinutes - finalizationReserveMinutes);
    assert.match(
      prompt,
      new RegExp(`Stop active exploration after ~${meta.explorationMinutes} minutes`),
    );
    assert.doesNotMatch(
      prompt,
      new RegExp(`Stop active exploration after ~${meta.explorationMinutes - finalizationReserveMinutes} minutes`),
    );
  } finally {
    rmSync(resultsDir, { recursive: true, force: true });
  }
});

test("run-agent.sh applies finalization reserve to hard timeout", () => {
  const script = readFileSync(join(root, "scripts/exploratory/run-agent.sh"), "utf8");
  assert.match(script, /FINALIZATION_RESERVE_MINUTES/);
  assert.match(script, /AGENT_TIMEOUT_SECONDS=\$\(\(EXPLORATION_MINUTES \* 60\)\)/);
  assert.match(script, /"\$\{AGENT_TIMEOUT_SECONDS\}s"/);
  assert.doesNotMatch(script, /BUDGET_SECONDS=\$\(\(BUDGET_MINUTES \* 60\)\)/);
});

test("run-agent.sh initializes budget variables before early-exit write_agent_status", () => {
  const scriptPath = join(root, "scripts/exploratory/run-agent.sh");
  const script = readFileSync(scriptPath, "utf8");
  const lines = script.split("\n");

  function firstLineIndex(matcher) {
    for (let index = 0; index < lines.length; index += 1) {
      if (matcher(lines[index])) return index;
    }
    return -1;
  }

  const firstEarlyExitStatus = firstLineIndex((line) =>
    /write_agent_status\s+[12]\s+"/.test(line),
  );
  assert.notEqual(firstEarlyExitStatus, -1, "expected an early-exit write_agent_status call");

  for (const variable of [
    "FINALIZATION_RESERVE_MINUTES",
    "EXPLORATION_MINUTES",
    "AGENT_TIMEOUT_SECONDS",
  ]) {
    const assignmentLine = firstLineIndex((line) => line.includes(`${variable}=`));
    assert.notEqual(assignmentLine, -1, `missing ${variable} assignment`);
    assert.ok(
      assignmentLine < firstEarlyExitStatus,
      `${variable} must be assigned before early-exit write_agent_status (line ${assignmentLine + 1} vs ${firstEarlyExitStatus + 1})`,
    );
  }

  const resultsDir = join(root, "exploratory-results");
  mkdirSync(join(resultsDir, "logs"), { recursive: true });
  writeFileSync(join(resultsDir, "run.json"), JSON.stringify({ version: 1 }));

  const env = { ...process.env };
  delete env.CURSOR_API_KEY;
  const result = spawnSync("bash", [scriptPath], {
    cwd: root,
    encoding: "utf8",
    env,
  });
  assert.equal(result.status, 2, result.stderr + result.stdout);
  assert.doesNotMatch(result.stderr, /unbound variable/i, result.stderr);
  assert.match(result.stderr, /CURSOR_API_KEY/);

  const run = JSON.parse(readFileSync(join(resultsDir, "run.json"), "utf8"));
  assert.equal(run.agent.status, "failed");
  assert.equal(run.agent.error, "missing CURSOR_API_KEY");
  assert.equal(run.agent.budgetMinutes, 12);
  assert.equal(run.agent.finalizationReserveMinutes, 2);
  assert.equal(run.agent.explorationMinutes, 10);
  assert.equal(run.agent.agentTimeoutSeconds, 600);
});

test("cli.json denies explorer writes to verifier evidence directory", () => {
  const cli = JSON.parse(readFileSync(join(root, ".github/exploratory/cli.json"), "utf8"));
  const deny = cli.permissions.deny.join("\n");
  assert.match(deny, /Write\(exploratory-results\/verifier\/\*\*\)/);
});

test("hasIndependentVerifierEvidence requires deterministic-verifier source and on-disk verifier files", async () => {
  const { hasIndependentVerifierEvidence } = await import("./exploratory/lib.mjs");
  const finding = { id: "ownership-finding-1" };
  const verifierDir = join(root, "exploratory-results", "verifier");
  mkdirSync(verifierDir, { recursive: true });
  const evidencePath = "exploratory-results/verifier/ownership-finding-1.png";
  writeFileSync(join(root, evidencePath), "verifier-evidence\n");

  const baseEntry = {
    findingId: "ownership-finding-1",
    reproductionSuccesses: 2,
    evidence: { screenshots: [evidencePath] },
  };

  assert.equal(hasIndependentVerifierEvidence(finding, { version: 1, verifications: [] }), false);
  assert.equal(
    hasIndependentVerifierEvidence(finding, {
      version: 1,
      verifications: [{ ...baseEntry, source: "explorer-agent" }],
    }),
    false,
  );
  assert.equal(
    hasIndependentVerifierEvidence(finding, {
      version: 1,
      verifications: [{ ...baseEntry, source: "deterministic-verifier" }],
    }),
    true,
  );

  rmSync(join(root, evidencePath));
});

test("isEligibleFinding rejects confirmed findings without verifier evidence", async () => {
  const { isEligibleFinding } = await import("./exploratory/file-issues.mjs");
  const { hasIndependentVerifierEvidence } = await import("./exploratory/lib.mjs");
  const finding = {
    id: "finding-agent-3",
    status: "confirmed",
    reproductionSuccesses: 2,
    classification: "novel",
    title: "Composer pending",
    area: "session",
    steps: ["reconnect"],
    expected: "send works",
    actual: "pending",
    evidence: {},
    fingerprint: "session|composer-pending",
  };
  const verifierDoc = { version: 1, verifications: [] };
  assert.equal(hasIndependentVerifierEvidence(finding, verifierDoc), false);
  assert.equal(isEligibleFinding(finding, { verifierDoc }), false);

  const verifierEvidence = "exploratory-results/verifier/finding-agent-3.png";
  mkdirSync(join(root, "exploratory-results", "verifier"), { recursive: true });
  writeFileSync(join(root, verifierEvidence), "verifier-evidence\n");
  assert.equal(
    isEligibleFinding(finding, {
      verifierDoc: {
        version: 1,
        verifications: [
          {
            findingId: "finding-agent-3",
            source: "deterministic-verifier",
            reproductionSuccesses: 2,
            evidence: { screenshots: [verifierEvidence] },
          },
        ],
      },
    }),
    true,
  );
  rmSync(join(root, verifierEvidence));
});

test("exploratory workflow stays off local verify path", async () => {
  const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const scripts = Object.values(packageJson.scripts).join("\n");
  assert.doesNotMatch(scripts, /exploratory\/run-agent/);
  assert.doesNotMatch(scripts, /exploratory-testing/);
});
