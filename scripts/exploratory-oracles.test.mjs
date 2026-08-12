import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(import.meta.url), "..", "..");

function tmpExploratoryDirs() {
  const tmp = mkdtempSync(join(tmpdir(), "expl-oracles-"));
  const resultsDir = join(tmp, "results");
  const memoryFile = join(tmp, "memory.json");
  mkdirSync(resultsDir, { recursive: true });
  return { tmp, resultsDir, memoryFile };
}

test("buildOracles prefers Web Cockpit / [defect] bugs from gh", async () => {
  const { buildOracles } = await import("./exploratory/prepare-oracles.mjs");
  const fakeIssues = [
    { number: 100, title: "cli: unrelated bug", url: "https://github.com/x/y/issues/100" },
    { number: 835, title: "[defect] Web Cockpit shows task after missing route", url: "https://github.com/x/y/issues/835" },
    { number: 810, title: "[defect] routing slash-only task hash", url: "https://github.com/x/y/issues/810" },
    { number: 200, title: "core: other", url: "https://github.com/x/y/issues/200" },
    { number: 201, title: "another", url: "https://github.com/x/y/issues/201" },
    { number: 202, title: "more", url: "https://github.com/x/y/issues/202" },
  ];

  const oracles = buildOracles({
    repo: "mossipcams/ajax-cli",
    execGh: () => JSON.stringify(fakeIssues),
    execGit: () => "abc1234 fix(web): route guard\n",
  });

  assert.ok(oracles.openBugs.some((bug) => bug.number === 835));
  assert.ok(oracles.openBugs.some((bug) => bug.number === 810));
  assert.equal(oracles.openBugs[0].number, 835);
  assert.deepEqual(oracles.recentWebCommits, ["abc1234 fix(web): route guard"]);
});

test("buildOracles survives gh failure and still fills commits/routes", async () => {
  const { buildOracles, ROUTES, BOUNDARY_HASHES } = await import(
    "./exploratory/prepare-oracles.mjs"
  );

  const oracles = buildOracles({
    repo: "mossipcams/ajax-cli",
    execGh: () => {
      throw new Error("gh not found");
    },
    execGit: () => "deadbeef chore(web): tweak banner\n",
  });

  assert.deepEqual(oracles.openBugs, []);
  assert.ok(oracles.bugsError);
  assert.deepEqual(oracles.recentWebCommits, ["deadbeef chore(web): tweak banner"]);
  assert.deepEqual(oracles.routes, ROUTES);
  assert.deepEqual(oracles.boundaryHashes, BOUNDARY_HASHES);
});

test("boundaryHashes and routes are non-empty", async () => {
  const { ROUTES, BOUNDARY_HASHES, emptyOracles } = await import(
    "./exploratory/prepare-oracles.mjs"
  );
  const oracles = emptyOracles();
  assert.ok(ROUTES.length > 0);
  assert.ok(BOUNDARY_HASHES.length > 0);
  assert.ok(oracles.routes.length > 0);
  assert.ok(oracles.boundaryHashes.length > 0);
});

test("memory fingerprints and dullActions round-trip from fixture", async () => {
  const { buildOracles } = await import("./exploratory/prepare-oracles.mjs");
  const { resultsDir, memoryFile } = tmpExploratoryDirs();

  writeFileSync(
    memoryFile,
    JSON.stringify({
      version: 1,
      dullActions: ["open empty inbox repeatedly"],
      confirmedFindings: [{ fingerprint: "navigation|hash-garbage", title: "x", area: "navigation" }],
      runs: [{ at: "2026-01-01T00:00:00.000Z", recommendedFocus: ["Garbage hashes", "terminal"] }],
    }),
  );

  const oracles = buildOracles({
    repo: null,
    execGit: () => "",
    memoryFile,
  });

  assert.deepEqual(oracles.memory.dullActions, ["open empty inbox repeatedly"]);
  assert.deepEqual(oracles.memory.recommendedFocus, ["Garbage hashes", "terminal"]);
  assert.deepEqual(oracles.memory.confirmedFingerprints, ["navigation|hash-garbage"]);

  const scriptResult = spawnSync(
    process.execPath,
    ["scripts/exploratory/prepare-oracles.mjs"],
    {
      cwd: root,
      encoding: "utf8",
      env: {
        ...process.env,
        AJAX_EXPLORATORY_RESULTS: resultsDir,
        AJAX_EXPLORATORY_MEMORY: memoryFile,
        GH_REPO: "",
      },
    },
  );
  assert.equal(scriptResult.status, 0, scriptResult.stderr);
  const written = JSON.parse(readFileSync(join(resultsDir, "oracles.json"), "utf8"));
  assert.deepEqual(written.memory.dullActions, oracles.memory.dullActions);
});

test("prepare-prompt embeds oracles and Garbage hashes charter", () => {
  const { resultsDir } = tmpExploratoryDirs();
  writeFileSync(
    join(resultsDir, "oracles.json"),
    JSON.stringify({
      version: 1,
      openBugs: [{ number: 835, title: "[defect] Web Cockpit banner mismatch", url: "https://x" }],
      recentWebCommits: ["abc fix(web): routes"],
      routes: ["#/"],
      boundaryHashes: ["#/garbage"],
      memory: { dullActions: [], recommendedFocus: [], confirmedFingerprints: [] },
    }),
  );

  const result = spawnSync(process.execPath, ["scripts/exploratory/prepare-prompt.mjs"], {
    cwd: root,
    encoding: "utf8",
    env: {
      ...process.env,
      AJAX_EXPLORATORY_RESULTS: resultsDir,
      AJAX_EXPLORATORY_MEMORY: join(resultsDir, "missing-memory.json"),
    },
  });
  assert.equal(result.status, 0, result.stderr);

  const prompt = readFileSync(join(resultsDir, "prompt.txt"), "utf8");
  assert.match(prompt, /## Oracles \(this run\)/);
  assert.match(prompt, /Garbage hashes/);
  assert.match(prompt, /abc fix\(web\): routes/);
});
