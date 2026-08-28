#!/usr/bin/env node
// Select deterministic primary/fallback missions for this nightly run.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { bootstrapBrowserSession, curlJson, isApiSuccess } from "./http-api.mjs";
import { selectMission } from "./missions.mjs";
import { BASE_URL, memoryPath, readJson, repoRoot, resultsDir, writeJson, emptyMemory } from "./lib.mjs";

const WEB_LOG_PATHS = [
  "crates/ajax-web",
  "crates/ajax-cli/src/web_backend.rs",
  "crates/ajax-cli/src/cockpit_backend.rs",
];

function git(args) {
  try {
    return execFileSync("git", args, { cwd: repoRoot, encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

function listChangedPaths(sinceSha, headSha) {
  if (!headSha) return [];
  if (!sinceSha || sinceSha === headSha) {
    return git(["log", "--name-only", "--pretty=format:", "-5", headSha, "--", ...WEB_LOG_PATHS])
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }
  const diff = git(["diff", "--name-only", `${sinceSha}..${headSha}`, "--", ...WEB_LOG_PATHS]);
  if (diff) {
    return diff.split("\n").map((line) => line.trim()).filter(Boolean);
  }
  return git(["log", "--name-only", "--pretty=format:", "-5", headSha, "--", ...WEB_LOG_PATHS])
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

function resolveSinceSha(memory, run) {
  return memory.lastRunSha || run.sinceSha || null;
}

function resolveHeadSha(run) {
  const candidate = run.headSha;
  if (!candidate) return git(["rev-parse", "HEAD"]) || null;
  const known = git(["rev-parse", "--verify", `${candidate}^{commit}`]);
  return known || git(["rev-parse", "HEAD"]) || null;
}

function listChangedCommits(sinceSha, headSha) {
  if (!headSha) return [];
  const range = sinceSha && sinceSha !== headSha ? `${sinceSha}..${headSha}` : headSha;
  return git(["log", "--oneline", "--no-merges", "-20", range, "--", ...WEB_LOG_PATHS])
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function buildMissionPlan({ memoryFile = memoryPath, results = resultsDir } = {}) {
  const memory = readJson(memoryFile, emptyMemory());
  const run = readJson(join(results, "run.json"), {});
  const headSha = resolveHeadSha(run);
  const sinceSha = resolveSinceSha(memory, run);
  const changedPaths = listChangedPaths(sinceSha, headSha);
  const changedCommits = listChangedCommits(sinceSha, headSha);
  return selectMission({ memory, headSha, changedPaths, changedCommits });
}

export async function seedMissionState({ mission, results = resultsDir, baseUrl = BASE_URL } = {}) {
  if (!mission?.seed) return { status: "skipped", reason: "mission has no seed requirements" };
  const seed = mission.seed;
  if (seed.kind !== "task") return { status: "skipped", reason: `unsupported seed kind ${seed.kind}` };

  const cookie = bootstrapBrowserSession(baseUrl);
  const requestId = `exploratory-seed-${Date.now()}`;
  const create = curlJson("POST", "/api/tasks", {
    baseUrl, cookie,
    body: { repo: seed.repo, title: seed.title, agent: seed.agent, request_id: requestId, model: "composer-2.5" },
  });
  if (!isApiSuccess(create)) {
    return {
      status: "failed", handle: `${seed.repo}/${seed.title}`, requestId,
      httpStatus: create.status, response: create.json,
      error: create.json?.error ?? `POST /api/tasks returned ${create.status}`,
    };
  }

  const handle = `${seed.repo}/${seed.title}`;
  const detail = curlJson("GET", `/api/tasks/${encodeURIComponent(handle)}`, { baseUrl, cookie });
  if (!isApiSuccess(detail)) {
    return {
      status: "failed", handle, requestId, httpStatus: detail.status, response: detail.json,
      error: detail.json?.error ?? `seeded task ${handle} not readable after create`,
    };
  }

  const result = {
    status: "ok", handle, requestId, response: create.json, detail: detail.json,
    seededAt: new Date().toISOString(),
  };
  writeJson(join(results, "seed.json"), result);
  const run = readJson(join(results, "run.json"), {});
  run.seed = { status: result.status, handle, agent: seed.agent };
  writeJson(join(results, "run.json"), run);
  return result;
}

export function activateFallbackMission({ results = resultsDir } = {}) {
  const missionDoc = readJson(join(results, "mission.json"), null);
  if (!missionDoc?.fallback?.id) {
    return { ok: false, error: "no fallback mission declared" };
  }
  if (missionDoc.primary?.id === missionDoc.fallback.id) {
    return { ok: true, alreadyActive: true, mission: missionDoc.primary };
  }

  const run = readJson(join(results, "run.json"), {});
  run.mission = {
    ...(run.mission ?? {}),
    primary: run.mission?.primary ?? missionDoc.primary.id,
    fallback: run.mission?.fallback ?? missionDoc.fallback.id,
    active: missionDoc.fallback.id,
    fallbackActivated: true,
  };

  missionDoc.plannedPrimary = missionDoc.plannedPrimary ?? missionDoc.primary;
  missionDoc.primary = missionDoc.fallback;
  writeJson(join(results, "mission.json"), missionDoc);
  writeJson(join(results, "run.json"), run);

  return { ok: true, mission: missionDoc.primary };
}

export async function runMissionSeedWithFallback({
  results = resultsDir,
  baseUrl = BASE_URL,
  seed = seedMissionState,
} = {}) {
  const missionDoc = readJson(join(results, "mission.json"), null);
  if (!missionDoc?.primary) {
    return { status: "failed", error: "missing mission.json — run plan-mission.mjs first" };
  }

  let result = await seed({ mission: missionDoc.primary, results, baseUrl });
  if (result.status !== "failed") {
    return result;
  }

  const primarySeed = result;
  const activated = activateFallbackMission({ results });
  if (!activated.ok) {
    return { ...primarySeed, fallbackAttempted: false };
  }

  result = await seed({ mission: activated.mission, results, baseUrl });
  if (result.status === "failed") {
    return { ...result, primarySeed, fallbackActivated: true };
  }

  return { ...result, primarySeedFailed: primarySeed, fallbackActivated: true };
}

export function main() {
  if (process.argv.includes("--seed")) {
    runMissionSeedWithFallback()
      .then((result) => {
        if (result.status === "failed") {
          console.error(JSON.stringify(result, null, 2));
          process.exit(1);
        }
        console.log(JSON.stringify(result, null, 2));
      })
      .catch((error) => {
        console.error(error);
        process.exit(1);
      });
    return;
  }

  const plan = buildMissionPlan();
  writeJson(join(resultsDir, "mission.json"), plan);
  const run = readJson(join(resultsDir, "run.json"), {});
  run.mission = {
    primary: plan.primary.id,
    fallback: plan.fallback.id,
    active: plan.primary.id,
    fallbackActivated: false,
    sinceSha: plan.sinceSha,
  };
  writeJson(join(resultsDir, "run.json"), run);
  console.log(JSON.stringify({ ok: true, primary: plan.primary.id, fallback: plan.fallback.id }));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
