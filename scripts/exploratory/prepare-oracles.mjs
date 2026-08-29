#!/usr/bin/env node
// Build exploration oracles (open bugs, recent commits, routes, memory hints).

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { memoryPath, readJson, repoRoot, resultsDir, writeJson } from "./lib.mjs";

// Static list matching crates/ajax-web/web/src/shared/lib/routes.ts (do not parse TS).
export const ROUTES = [
  "#/",
  "#/settings",
  "#/p/<project>",
  "#/t/<handle>",
  "#/t/<handle>/diff",
  "#/t/<handle>/diff?pr=N",
];

// Defect neighborhood from routes.test.ts + known routing bugs (#810, #818, #821, #811, #835).
export const BOUNDARY_HASHES = [
  "#/garbage", // unknown hash → dashboard (#routes.test.ts)
  "#/t/", // empty task handle → dashboard (#810 slash-only)
  "#/t/%2F", // encoded slash-only handle
  "#/p/", // empty project → dashboard
  "#/p/%20", // whitespace-only project (#821)
  "#/t/missing%2Ftask-id", // task that does not exist (#835 class)
  "#/t/demo%2Fexploratory-test-task-alpha/diff/", // trailing slash on diff (#818)
  "#/t/demo%2Fexploratory-test-task-alpha/diff/extra", // nested path after /diff (#811)
  "#/t/demo%2Fexploratory-test-task-alpha/diff?pr=1", // diff with pr query
];

const WEB_LOG_PATHS = [
  "crates/ajax-web",
  "crates/ajax-cli/src/web_backend.rs",
  "crates/ajax-cli/src/cockpit_backend.rs",
  "docs/architecture/web-cockpit.md",
];

function defaultExecGh(repo) {
  return execFileSync(
    "gh",
    [
      "issue",
      "list",
      "--repo",
      repo,
      "--label",
      "bug",
      "--state",
      "open",
      "--limit",
      "40",
      "--json",
      "number,title,url",
    ],
    { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
}

function defaultExecGit() {
  return execFileSync(
    "git",
    ["log", "--oneline", "--no-merges", "-20", "--", ...WEB_LOG_PATHS],
    { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
}

function isPreferredBug(title) {
  const lower = title.toLowerCase();
  return lower.includes("web cockpit") || lower.includes("[defect]");
}

function selectOpenBugs(issues) {
  const preferred = issues.filter((issue) => isPreferredBug(issue.title));
  if (preferred.length >= 5) return preferred;
  const preferredNumbers = new Set(preferred.map((issue) => issue.number));
  const rest = issues.filter((issue) => !preferredNumbers.has(issue.number));
  return [...preferred, ...rest];
}

function memoryHints(memoryFile = memoryPath) {
  const memory = readJson(memoryFile, null);
  if (!memory) {
    return {
      dullActions: [],
      recommendedFocus: [],
      confirmedFingerprints: [],
    };
  }

  const lastRun = Array.isArray(memory.runs) ? memory.runs.at(-1) : null;
  return {
    dullActions: memory.dullActions ?? [],
    recommendedFocus: lastRun?.recommendedFocus ?? [],
    confirmedFingerprints: (memory.confirmedFindings ?? []).map((item) => item.fingerprint),
  };
}

export function emptyOracles() {
  return {
    version: 1,
    openBugs: [],
    recentWebCommits: [],
    routes: ROUTES,
    boundaryHashes: BOUNDARY_HASHES,
    memory: {
      dullActions: [],
      recommendedFocus: [],
      confirmedFingerprints: [],
    },
  };
}

export function buildOracles({
  execGh = defaultExecGh,
  execGit = defaultExecGit,
  repo = process.env.GH_REPO,
  memoryFile = memoryPath,
} = {}) {
  const oracles = emptyOracles();
  oracles.memory = memoryHints(memoryFile);

  if (repo) {
    try {
      const raw = execGh(repo);
      const issues = JSON.parse(raw.trim() || "[]");
      oracles.openBugs = selectOpenBugs(issues).map(({ number, title, url }) => ({
        number,
        title,
        url,
      }));
    } catch (error) {
      oracles.bugsError = String(error?.message ?? error);
      oracles.openBugs = [];
    }
  }

  try {
    oracles.recentWebCommits = execGit()
      .trim()
      .split("\n")
      .filter(Boolean);
  } catch (error) {
    oracles.commitsError = String(error?.message ?? error);
    oracles.recentWebCommits = [];
  }

  return oracles;
}

export function main() {
  const oracles = buildOracles();
  const outPath = join(resultsDir, "oracles.json");
  writeJson(outPath, oracles);
  console.log(outPath);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
