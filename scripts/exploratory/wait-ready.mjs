#!/usr/bin/env node
// Poll isolated Ajax Web health until ready or timeout.

import { join } from "node:path";
import { BASE_URL, readJson, resultsDir, writeJson } from "./lib.mjs";

const timeoutMs = Number(process.env.AJAX_EXPLORATORY_READY_TIMEOUT_MS ?? 120_000);
const intervalMs = 2_000;

async function check() {
  const url = `${BASE_URL}/api/health`;
  const response = await fetch(url, {
    signal: AbortSignal.timeout(5_000),
    // Node fetch against self-signed HTTPS needs undici dispatcher; use curl fallback.
  }).catch(() => null);
  if (response?.ok) return true;
  return false;
}

async function checkWithCurl() {
  const { execFileSync } = await import("node:child_process");
  try {
    const out = execFileSync(
      "curl",
      ["-sk", "--max-time", "5", `${BASE_URL}/api/health`],
      { encoding: "utf8" },
    );
    return out.includes("ok") || out.includes("\"status\"");
  } catch {
    return false;
  }
}

async function main() {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if ((await checkWithCurl()) || (await check())) {
      const run = readJson(join(resultsDir, "run.json"), {});
      run.infrastructure = { status: "ready", error: null, readyAt: new Date().toISOString() };
      writeJson(join(resultsDir, "run.json"), run);
      console.log(`ready: ${BASE_URL}/api/health`);
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }

  const run = readJson(join(resultsDir, "run.json"), {});
  run.infrastructure = {
    status: "failed",
    error: `Ajax Web did not become ready within ${timeoutMs}ms at ${BASE_URL}`,
  };
  writeJson(join(resultsDir, "run.json"), run);
  console.error(run.infrastructure.error);
  process.exit(1);
}

main();
