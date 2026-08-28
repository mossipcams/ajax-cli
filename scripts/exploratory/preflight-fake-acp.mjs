// Prove the fake ACP wrapper handles Ajax argv and can complete a prompt.

import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { bootstrapBrowserSession, curlJson, isApiSuccess } from "./http-api.mjs";
import { activateFallbackMission, seedMissionState } from "./plan-mission.mjs";
import { BASE_URL, readJson, repoRoot, resultsDir, writeJson } from "./lib.mjs";

export function fakeAcpFixturePath(root = repoRoot) {
  return join(root, "crates", "ajax-web", "tests", "fixtures", "fake_acp.js");
}

export function fakeAcpWrapperPath(root = repoRoot) {
  return join(root, "scripts", "exploratory", "agent-stubs", "fake-acp");
}

export function agentStubPath(root = repoRoot) {
  return join(root, "scripts", "exploratory", "agent-stubs", "agent");
}

export function agentStubsDir(root = repoRoot) {
  return join(root, "scripts", "exploratory", "agent-stubs");
}

function acpProbePath(root = repoRoot) {
  return join(root, "scripts", "exploratory", "agent-stubs", "acp-probe");
}

function runAcpProbe({ root, cwd, ajaxArgv = ["--model", "composer-2.5", "acp"] }) {
  return spawnSync(
    process.execPath,
    [acpProbePath(root), agentStubsDir(root), fakeAcpWrapperPath(root), cwd, JSON.stringify(ajaxArgv)],
    { encoding: "utf8", maxBuffer: 1024 * 1024 },
  );
}

async function verifySeededTask({ mission, baseUrl, results = resultsDir }) {
  const existing = readJson(join(results, "seed.json"), null);
  const handle =
    existing?.handle ??
    (mission?.seed ? `${mission.seed.repo}/${mission.seed.title}` : null);
  if (!handle) {
    return { ok: false, error: "no seeded task handle to verify" };
  }
  if (existing?.status && existing.status !== "ok") {
    return { ok: false, error: existing.error ?? "prior seed failed", seed: existing };
  }

  const cookie = bootstrapBrowserSession(baseUrl);
  const encoded = encodeURIComponent(handle);
  const detail = curlJson("GET", `/api/tasks/${encoded}`, { baseUrl, cookie });
  if (!isApiSuccess(detail)) {
    return {
      ok: false,
      error: detail.json?.error ?? `seeded task ${handle} not readable`,
      handle,
      detail,
    };
  }
  return { ok: true, handle, detail: detail.json };
}

export async function preflightFakeAcp({
  root = repoRoot,
  ajaxArgv = ["--model", "composer-2.5", "acp"],
  mission = null,
  baseUrl = BASE_URL,
  verifyServer = false,
} = {}) {
  const fixturePath = fakeAcpFixturePath(root);
  const wrapperPath = fakeAcpWrapperPath(root);
  const agentPath = agentStubPath(root);
  if (!existsSync(fixturePath)) {
    return { status: "blocked", error: `missing fixture: ${fixturePath}`, fixturePath };
  }
  if (!existsSync(wrapperPath)) {
    return { status: "blocked", error: `missing wrapper: ${wrapperPath}`, fixturePath };
  }
  if (!existsSync(agentPath)) {
    return { status: "blocked", error: `missing agent stub: ${agentPath}`, fixturePath };
  }

  const probeDir = mkdtempSync(join(tmpdir(), "ajax-fake-acp-preflight-"));
  const probe = runAcpProbe({ root, cwd: probeDir, ajaxArgv });
  rmSync(probeDir, { recursive: true, force: true });
  if (probe.status !== 0) {
    return {
      status: "blocked",
      error: probe.stderr?.trim() || probe.stdout?.trim() || "fake ACP probe failed",
      fixturePath,
      wrapperPath,
      agentPath,
    };
  }

  let parsed = {};
  try {
    parsed = JSON.parse(probe.stdout.trim() || "{}");
  } catch (error) {
    return { status: "blocked", error: `invalid probe output: ${error.message}`, fixturePath };
  }

  if (!parsed.initializeOk || !parsed.sessionOk || !parsed.promptOk) {
    return {
      status: "blocked",
      error: "fake ACP probe did not complete initialize/session/prompt",
      fixturePath,
      wrapperPath,
      agentPath,
      details: parsed,
    };
  }

  const result = {
    status: "ok",
    fixturePath,
    wrapperPath,
    agentPath,
    sessionId: parsed.sessionId,
    ajaxArgv,
    checkedAt: new Date().toISOString(),
  };

  const shouldVerifyServer =
    verifyServer || Boolean(mission?.seed) || Boolean(readJson(join(resultsDir, "mission.json"), {})?.primary?.seed);
  const activeMission = mission ?? readJson(join(resultsDir, "mission.json"), {})?.primary ?? null;
  if (shouldVerifyServer && activeMission?.seed) {
    try {
      const health = curlJson("GET", "/api/health", { baseUrl });
      if (!isApiSuccess(health)) {
        return { status: "blocked", error: `server not ready at ${baseUrl}`, ...result };
      }
      const seeded = await verifySeededTask({ mission: activeMission, baseUrl });
      if (!seeded.ok) {
        return { status: "blocked", error: seeded.error, seed: seeded.seed, ...result };
      }
      result.seed = { status: "ok", handle: seeded.handle };
    } catch (error) {
      return { status: "blocked", error: error.message, ...result };
    }
  }

  return result;
}

function missionNeedsPreflight(mission) {
  return Boolean(mission?.needsFakeAcp);
}

export async function runPreflightWithFallback({
  results = resultsDir,
  baseUrl = BASE_URL,
  root = repoRoot,
  preflight = preflightFakeAcp,
  seed = seedMissionState,
} = {}) {
  const missionDoc = readJson(join(results, "mission.json"), null);
  if (!missionDoc?.primary) {
    return recordPreflightResult(
      { status: "blocked", error: "missing mission.json — run plan-mission.mjs first" },
      { results },
    );
  }

  if (!missionNeedsPreflight(missionDoc.primary)) {
    return recordPreflightResult(
      { status: "skipped", reason: "mission does not require fake ACP" },
      { results },
    );
  }

  let mission = missionDoc.primary;
  let result = await preflight({
    mission,
    verifyServer: Boolean(mission?.seed),
    baseUrl,
    root,
  });
  if (result.status !== "blocked") {
    return recordPreflightResult(result, { results });
  }

  const run = readJson(join(results, "run.json"), {});
  if (run.mission?.fallbackActivated) {
    return recordPreflightResult(result, { results });
  }

  const primaryPreflight = result;
  const activated = activateFallbackMission({ results });
  if (!activated.ok) {
    return recordPreflightResult({ ...primaryPreflight, fallbackAttempted: false }, { results });
  }

  mission = activated.mission;
  if (mission?.seed) {
    const seedResult = await seed({ mission, results, baseUrl });
    if (seedResult.status === "failed") {
      return recordPreflightResult(
        {
          status: "blocked",
          error: seedResult.error ?? "fallback seed failed",
          primaryPreflight,
          fallbackSeed: seedResult,
          fallbackActivated: true,
        },
        { results },
      );
    }
  }

  result = await preflight({
    mission,
    verifyServer: Boolean(mission?.seed),
    baseUrl,
    root,
  });
  if (result.status === "blocked") {
    return recordPreflightResult({ ...result, primaryPreflight, fallbackActivated: true }, { results });
  }

  return recordPreflightResult(
    { ...result, primaryPreflightFailed: primaryPreflight, fallbackActivated: true },
    { results },
  );
}

function recordPreflightResult(result, { results = resultsDir } = {}) {
  const run = readJson(join(results, "run.json"), {});
  run.preflight = result;
  writeJson(join(results, "run.json"), run);
  return result;
}

export async function main() {
  const result = await runPreflightWithFallback();
  recordPreflightResult(result);
  console.log(JSON.stringify(result, null, 2));
  if (result.status === "blocked") {
    console.error(result.error ?? "preflight blocked");
    process.exit(1);
  }
  process.exit(0);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
