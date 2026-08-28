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
    execGh: (_repo, state) =>
      state === "closed"
        ? JSON.stringify([
            {
              number: 77,
              title: "[defect] Web Cockpit closed regression",
              body: "<!-- exploratory-fingerprint: terminal|paste-broken -->",
              url: "https://github.com/x/y/issues/77",
              state: "CLOSED",
            },
          ])
        : JSON.stringify(fakeIssues),
    execGit: () => "abc1234 fix(web): route guard\n",
  });

  assert.ok(oracles.openBugs.some((bug) => bug.number === 835));
  assert.ok(oracles.closedBugs.some((bug) => bug.number === 77));
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

test("prepare-prompt embeds mission and oracles", () => {
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

  const result = spawnSync(process.execPath, ["scripts/exploratory/prepare-prompt.mjs"], {
    cwd: root,
    encoding: "utf8",
    env: {
      ...process.env,
      AJAX_EXPLORATORY_RESULTS: resultsDir,
      AJAX_EXPLORATORY_MEMORY: join(resultsDir, "missing-memory.json"),
      AJAX_EXPLORATORY_BUDGET_MINUTES: "12",
    },
  });
  assert.equal(result.status, 0, result.stderr);

  const prompt = readFileSync(join(resultsDir, "prompt.txt"), "utf8");
  assert.match(prompt, /## Assigned mission \(primary\)/);
  assert.match(prompt, /garbage-hashes/);
  assert.match(prompt, /abc fix\(web\): routes/);
  assert.doesNotMatch(prompt, /minutes minimum/);
  assert.match(prompt, /12 minutes maximum/i);
  assert.match(prompt, /two.*successful reset\/reproduction cycles/i);
});

test("assertWebkitMcpConfig accepts webkit and rejects chromium config", async () => {
  const { assertWebkitMcpConfig } = await import("./exploratory/assert-webkit.mjs");
  const webkitConfig = {
    mcpServers: {
      playwright: {
        args: ["--browser", "webkit", "--headless"],
      },
    },
  };
  assert.doesNotThrow(() => assertWebkitMcpConfig(webkitConfig));

  const chromiumConfig = {
    mcpServers: {
      playwright: {
        args: ["--browser", "chromium", "--no-sandbox"],
      },
    },
  };
  assert.throws(() => assertWebkitMcpConfig(chromiumConfig), /--browser must be exactly webkit/);
});

test("mcp.json is WebKit-only and assert-webkit --config-only passes", () => {
  const mcp = JSON.parse(
    readFileSync(join(root, ".github/exploratory/mcp.json"), "utf8"),
  );
  const args = mcp.mcpServers.playwright.args;
  const browserIdx = args.indexOf("--browser");
  assert.equal(args[browserIdx + 1], "webkit");
  assert.ok(!args.includes("chromium"));
  assert.ok(!args.includes("firefox"));
  assert.ok(!args.includes("--no-sandbox"));

  const result = spawnSync(
    process.execPath,
    ["scripts/exploratory/assert-webkit.mjs", "--config-only"],
    { cwd: root, encoding: "utf8" },
  );
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /ok/);
});

test("run-agent.sh runs exactly one explorer process", () => {
  const script = readFileSync(join(root, "scripts/exploratory/run-agent.sh"), "utf8");
  assert.match(script, /MAX_ATTEMPTS=1/);
  assert.doesNotMatch(script, /MAX_ATTEMPTS=2/);
  assert.doesNotMatch(script, /relaunch/i);
  assert.doesNotMatch(script, /Continuation:/);
  assert.match(script, /assert-webkit\.mjs/);
});

test("exploratory workflow is schedule-only with validate before memory update", () => {
  const workflow = readFileSync(
    join(root, ".github/workflows/exploratory-testing.yml"),
    "utf8",
  );
  assert.match(workflow, /cron: "17 6 \* \* \*"/);
  assert.doesNotMatch(workflow, /workflow_dispatch/);
  assert.doesNotMatch(workflow, /budget_minutes/);
  assert.match(workflow, /playwright install --with-deps webkit/);
  assert.doesNotMatch(workflow, /playwright install --with-deps chromium/);

  const validateIdx = workflow.indexOf("validate-run.mjs");
  const classifyIdx = workflow.indexOf("classify-findings.mjs");
  const fileIdx = workflow.indexOf("file-issues.mjs");
  const memoryIdx = workflow.indexOf("update-memory.mjs");
  const seedIdx = workflow.indexOf("plan-mission.mjs --seed");
  const preflightIdx = workflow.indexOf("preflight-fake-acp.mjs");
  assert.ok(validateIdx > 0 && classifyIdx > validateIdx && fileIdx > classifyIdx && memoryIdx > fileIdx);
  assert.ok(seedIdx > 0 && preflightIdx > seedIdx);
});

test("cli.json enables sandbox with network denied while keeping WebKit MCP", () => {
  const cli = JSON.parse(readFileSync(join(root, ".github/exploratory/cli.json"), "utf8"));
  const allow = cli.permissions.allow.join("\n");
  assert.match(allow, /Mcp\(playwright/);
  assert.doesNotMatch(allow, /Shell\(curl/);
  assert.doesNotMatch(allow, /Shell\(rg/);
  assert.equal(cli.sandbox?.mode, "enabled");
  assert.equal(cli.sandbox?.networkAccess, "deny");
});

test("agent stub delegates acp launches to fake fixture wrapper", () => {
  const agentStub = readFileSync(join(root, "scripts/exploratory/agent-stubs/agent"), "utf8");
  assert.match(agentStub, /AJAX_EXPLORATORY_FAKE_ACP/);
  assert.match(agentStub, /acp/);
});

test("fake-acp agent wrapper points at fixture without copying it", () => {
  const wrapper = readFileSync(join(root, "scripts/exploratory/agent-stubs/fake-acp"), "utf8");
  assert.match(wrapper, /fake_acp\.js/);
  assert.doesNotMatch(wrapper, /sleep infinity/);
});

test("charter defines stopping criteria and maximum budget", () => {
  const charter = readFileSync(join(root, ".github/exploratory/charter.md"), "utf8");
  assert.match(charter, /## Stopping criteria/);
  assert.match(charter, /stop-reason\.json/);
  assert.match(charter, /\*\*maximum\*\*/i);
  assert.doesNotMatch(charter, /minimum\*\*, not a target/);
  assert.doesNotMatch(charter, /finish checklist is not permission to stop/i);
  assert.doesNotMatch(charter, /keep exploring until the runner stops you/i);
  assert.doesNotMatch(charter, /run it for several minutes/i);
});
