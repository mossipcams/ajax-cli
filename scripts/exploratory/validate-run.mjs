#!/usr/bin/env node
// Validate exploratory outputs, enforce read-only source, and support fixtures.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  emptyFindings,
  emptyObservations,
  normalizeFindingsDocument,
  readJson,
  repoRoot,
  resultsDir,
  simulatedFinding,
  validateFindingsDocument,
  writeJson,
} from "./lib.mjs";

function parseArgs(argv) {
  return {
    fixture: argv.includes("--fixture"),
    checkReadonly: !argv.includes("--skip-readonly"),
  };
}

function forbiddenDirtyPaths(porcelain) {
  const allowedPrefixes = [
    "exploratory-results/",
    "exploratory-memory/",
    "target/",
    ".cursor/cli.json",
    ".cursor/mcp.json",
  ];
  return porcelain
    .split("\n")
    .map((line) => line.trimEnd())
    .filter(Boolean)
    .map((line) => line.slice(3).trim())
    .filter((path) => !allowedPrefixes.some((prefix) => path.startsWith(prefix)));
}

function summarizeFindings(doc) {
  const summary = { confirmed: 0, observation: 0, rejected: 0 };
  for (const finding of doc.findings ?? []) {
    if (summary[finding.status] !== undefined) summary[finding.status] += 1;
  }
  return summary;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const problems = [];

  if (args.fixture) {
    mkdirSync(join(resultsDir, "traces"), { recursive: true });
    mkdirSync(join(resultsDir, "screenshots"), { recursive: true });
    mkdirSync(join(resultsDir, "logs"), { recursive: true });
    writeFileSync(
      join(resultsDir, "traces", "sim-reconnect.zip"),
      "simulated-trace\n",
    );
    writeFileSync(
      join(resultsDir, "screenshots", "sim-reconnect.png"),
      "simulated-screenshot\n",
    );
    writeJson(join(resultsDir, "findings.json"), {
      version: 1,
      findings: [simulatedFinding()],
    });
    writeJson(join(resultsDir, "observations.json"), emptyObservations());
    writeJson(join(resultsDir, "run.json"), {
      version: 1,
      startedAt: new Date().toISOString(),
      infrastructure: { status: "ok", error: null },
      agent: { status: "fixture", exitCode: 0 },
      findingsSummary: { confirmed: 1, observation: 0, rejected: 0 },
    });
  }

  if (!existsSync(join(resultsDir, "findings.json"))) {
    problems.push("missing exploratory-results/findings.json");
  }
  if (!existsSync(join(resultsDir, "observations.json"))) {
    problems.push("missing exploratory-results/observations.json");
  }
  if (!existsSync(join(resultsDir, "run.json"))) {
    problems.push("missing exploratory-results/run.json");
  }

  const findingsPath = join(resultsDir, "findings.json");
  const rawFindings = readJson(findingsPath, emptyFindings());
  const findings = normalizeFindingsDocument(rawFindings);
  writeJson(findingsPath, findings);
  const findingProblems = validateFindingsDocument(findings);
  problems.push(...findingProblems);

  const run = readJson(join(resultsDir, "run.json"), {});
  run.findingsSummary = summarizeFindings(findings);
  run.validatedAt = new Date().toISOString();
  writeJson(join(resultsDir, "run.json"), run);

  if (args.checkReadonly) {
    const porcelain = execFileSync("git", ["status", "--porcelain"], {
      cwd: repoRoot,
      encoding: "utf8",
    });
    const forbidden = forbiddenDirtyPaths(porcelain);
    if (forbidden.length > 0) {
      problems.push(
        `product source became dirty during exploration: ${forbidden.join(", ")}`,
      );
    }
  }

  if (problems.length > 0) {
    for (const problem of problems) console.error(problem);
    process.exit(1);
  }

  console.log(
    JSON.stringify(
      {
        ok: true,
        findingsSummary: run.findingsSummary,
        fixture: args.fixture,
      },
      null,
      2,
    ),
  );
}

main();
