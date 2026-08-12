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
  assert.ok(problems.some((problem) => problem.includes("successful reproduction")));
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

test("exploratory workflow stays off local verify path", async () => {
  const packageJson = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
  const scripts = Object.values(packageJson.scripts).join("\n");
  assert.doesNotMatch(scripts, /exploratory\/run-agent/);
  assert.doesNotMatch(scripts, /exploratory-testing/);
});
