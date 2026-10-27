#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { memoryPath, readJson, repoRoot, resultsDir, writeJson } from "./lib.mjs";

export const ROUTES = [
  "#/",
  "#/settings",
  "#/p/<project>",
  "#/t/<handle>",
  "#/t/<handle>/diff",
  "#/t/<handle>/diff?pr=N",
];

export const BOUNDARY_HASHES = [
  "#/garbage",
  "#/t/",
  "#/t/%2F",
  "#/p/",
  "#/p/%20",
  "#/t/missing%2Ftask-id",
  "#/t/demo%2Fexploratory-test-task-alpha/diff/",
  "#/t/demo%2Fexploratory-test-task-alpha/diff/extra",
  "#/t/demo%2Fexploratory-test-task-alpha/diff?pr=1",
];

const WEB_LOG_PATHS = [
  "crates/ajax-web",
  "crates/ajax-cli/src/web_backend.rs",
  "crates/ajax-cli/src/cockpit_backend.rs",
  "docs/architecture/web-cockpit.md",
];

function defaultExecGh(repo, state = "open") {
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
      state,
      "--limit",
      "100",
      "--json",
      "number,title,body,url,state",
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
    closedBugs: [],
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
      const raw = execGh(repo, "open");
      const issues = JSON.parse(raw.trim() || "[]");
      oracles.openBugs = selectOpenBugs(issues).map(({ number, title, body, url, state }) => ({
        number,
        title,
        body,
        url,
        state,
      }));
    } catch (error) {
      oracles.bugsError = String(error?.message ?? error);
      oracles.openBugs = [];
    }
    try {
      const rawClosed = execGh(repo, "closed");
      const closed = JSON.parse(rawClosed.trim() || "[]");
      oracles.closedBugs = closed.map(({ number, title, body, url, state }) => ({
        number,
        title,
        body,
        url,
        state,
      }));
    } catch (error) {
      oracles.closedBugsError = String(error?.message ?? error);
      oracles.closedBugs = [];
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
