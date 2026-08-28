// Prove the fake ACP wrapper handles Ajax argv and can complete a prompt.

import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { bootstrapBrowserSession, curlJson, isApiSuccess } from "./http-api.mjs";
import { BASE_URL, readJson, repoRoot, resultsDir } from "./lib.mjs";

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

export async function main() {
  const missionDoc = readJson(join(resultsDir, "mission.json"), null);
  const result = await preflightFakeAcp({
    mission: missionDoc?.primary ?? null,
    verifyServer: Boolean(missionDoc?.primary?.seed),
  });
  console.log(JSON.stringify(result, null, 2));
  process.exit(result.status === "ok" ? 0 : 1);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
