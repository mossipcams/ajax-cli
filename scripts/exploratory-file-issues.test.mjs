import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

function writeResults({ findings = [], run = {} } = {}) {
  const dir = mkdtempSync(join(tmpdir(), "expl-issues-"));
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    join(dir, "findings.json"),
    JSON.stringify({ version: 1, findings }),
  );
  writeFileSync(
    join(dir, "run.json"),
    JSON.stringify({ version: 1, headSha: "abc123def", ...run }),
  );
  return {
    findingsPath: join(dir, "findings.json"),
    runPath: join(dir, "run.json"),
    issuesPath: join(dir, "issues.json"),
  };
}

function fileOpts(paths, extra = {}) {
  return {
    findingsPath: paths.findingsPath,
    runPath: paths.runPath,
    issuesPath: paths.issuesPath,
    argv: [],
    ...extra,
  };
}

function baseFinding(overrides = {}) {
  return {
    id: "f1",
    title: "Composer remains pending after reconnect",
    status: "confirmed",
    confidence: "high",
    area: "session",
    severity: "critical",
    reproductionAttempts: 2,
    reproductionSuccesses: 2,
    steps: ["Create a session", "Reconnect", "Send a message"],
    expected: "Message sends",
    actual: "Composer stays pending",
    evidence: {
      notes: "Reproduced twice in headless Chromium.",
      consoleErrors: ["TypeError: socket closed"],
      networkFailures: ["POST /api/messages 500"],
    },
    fingerprint: "session|composer-pending-after-reconnect",
    ...overrides,
  };
}

function fakeGh({ listIssues = [], createShouldFail = false } = {}) {
  const calls = [];
  const exec = (args) => {
    calls.push(args);
    if (args[0] === "issue" && args[1] === "list") {
      return `${JSON.stringify(listIssues)}\n`;
    }
    if (args[0] === "issue" && args[1] === "create") {
      if (createShouldFail) {
        throw new Error("gh issue create failed");
      }
      return "https://github.com/mossipcams/ajax-cli/issues/99\n";
    }
    throw new Error(`unexpected gh args: ${args.join(" ")}`);
  };
  return { exec, calls };
}

test("confirmed finding creates issue body with fingerprint and mapped severity", async () => {
  const {
    buildIssueBody,
    buildIssueTitle,
    fileIssues,
    mapSeverity,
  } = await import("./exploratory/file-issues.mjs");

  const finding = baseFinding();
  assert.equal(mapSeverity("critical"), "blocker");
  assert.equal(buildIssueTitle(finding), "[defect] Web Cockpit Composer remains pending after reconnect");

  const body = buildIssueBody(finding, {
    fingerprint: finding.fingerprint,
    version: "abc123def",
    runUrl: "https://github.com/mossipcams/ajax-cli/actions/runs/1",
    artifactName: "exploratory-results-1",
  });
  assert.match(body, /### Summary/);
  assert.match(body, /### Surface\nWeb Cockpit/);
  assert.match(body, /### Steps to reproduce/);
  assert.match(body, /### Expected/);
  assert.match(body, /### Actual/);
  assert.match(body, /### Version \/ commit\nabc123def/);
  assert.match(body, /### Severity\nblocker/);
  assert.match(body, /<!-- exploratory-fingerprint: session\|composer-pending-after-reconnect -->/);
  assert.match(body, /Reproduced twice in headless Chromium/);
  assert.match(body, /Console errors:/);
  assert.match(body, /Network failures:/);

  const paths = writeResults({ findings: [finding] });
  const gh = fakeGh();
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: {
        GITHUB_ACTIONS: "true",
        GH_REPO: "mossipcams/ajax-cli",
        AJAX_EXPLORATORY_RUN_URL: "https://github.com/mossipcams/ajax-cli/actions/runs/1",
        GITHUB_RUN_ID: "1",
      },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.filter((args) => args[1] === "create").length, 1);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "created");
  assert.equal(issues[0].issueNumber, 99);
});

test("duplicate by fingerprint avoids create", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const finding = baseFinding();
  const paths = writeResults({ findings: [finding] });
  const gh = fakeGh({
    listIssues: [
      {
        number: 12,
        title: "[defect] Web Cockpit unrelated title",
        body: "Some notes\n<!-- exploratory-fingerprint: session|composer-pending-after-reconnect -->",
        url: "https://github.com/mossipcams/ajax-cli/issues/12",
      },
    ],
  });
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "true", GH_REPO: "mossipcams/ajax-cli" },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.filter((args) => args[1] === "create").length, 0);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "duplicate");
  assert.equal(issues[0].issueNumber, 12);
});

test("duplicate by title avoids create", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const finding = baseFinding({ fingerprint: undefined });
  const paths = writeResults({ findings: [finding] });
  const gh = fakeGh({
    listIssues: [
      {
        number: 15,
        title: "[defect] Web Cockpit Composer remains pending after reconnect",
        body: "No fingerprint comment",
        url: "https://github.com/mossipcams/ajax-cli/issues/15",
      },
    ],
  });
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "true", GH_REPO: "mossipcams/ajax-cli" },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.filter((args) => args[1] === "create").length, 0);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "duplicate");
});

test("duplicate by relatedIssues avoids create even when titles differ", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const finding = baseFinding({
    title: "Totally different title from open issue",
    fingerprint: "session|unrelated-fingerprint",
    relatedIssues: [810],
  });
  const paths = writeResults({ findings: [finding] });
  const gh = fakeGh({
    listIssues: [
      {
        number: 810,
        title: "[defect] Web Cockpit unrelated existing issue",
        body: "No fingerprint comment",
        url: "https://github.com/mossipcams/ajax-cli/issues/810",
      },
    ],
  });
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "true", GH_REPO: "mossipcams/ajax-cli" },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.filter((args) => args[1] === "create").length, 0);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "duplicate");
  assert.equal(issues[0].issueNumber, 810);
});

test("observation and rejected findings are not filed", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const paths = writeResults({
    findings: [
      baseFinding({ id: "obs", status: "observation", reproductionSuccesses: 1 }),
      baseFinding({ id: "rej", status: "rejected", reproductionSuccesses: 1 }),
    ],
  });
  const gh = fakeGh();
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "true", GH_REPO: "mossipcams/ajax-cli" },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.length, 1);
  assert.equal(gh.calls[0][1], "list");
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues.length, 0);
});

test("outside GitHub Actions without force skips filing", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const paths = writeResults({ findings: [baseFinding()] });
  const gh = fakeGh();
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "false" },
    }),
  );
  assert.equal(exitCode, 0);
  assert.equal(gh.calls.length, 0);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "skipped");
});

test("create failure exits 1", async () => {
  const { fileIssues } = await import("./exploratory/file-issues.mjs");
  const paths = writeResults({ findings: [baseFinding()] });
  const gh = fakeGh({ createShouldFail: true });
  const exitCode = fileIssues(
    fileOpts(paths, {
      execGh: gh.exec,
      env: { GITHUB_ACTIONS: "true", GH_REPO: "mossipcams/ajax-cli" },
    }),
  );
  assert.equal(exitCode, 1);
  const issues = JSON.parse(readFileSync(paths.issuesPath, "utf8"));
  assert.equal(issues[0].action, "failed");
  assert.match(issues[0].error, /gh issue create failed/);
  const run = JSON.parse(readFileSync(paths.runPath, "utf8"));
  assert.equal(run.issues.failed, 1);
});
