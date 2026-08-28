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

test("exploratory workflow stays off local verify path", async () => {
  const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const scripts = Object.values(packageJson.scripts).join("\n");
  assert.doesNotMatch(scripts, /exploratory\/run-agent/);
  assert.doesNotMatch(scripts, /exploratory-testing/);
});
