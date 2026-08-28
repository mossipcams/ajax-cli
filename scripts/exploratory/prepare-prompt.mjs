#!/usr/bin/env node
// Assemble the Cursor Agent prompt from charter + mission + oracles + memory.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { emptyOracles } from "./prepare-oracles.mjs";
import {
  BASE_URL,
  computeAgentBudget,
  emptyMemory,
  exploratoryDir,
  memoryPath,
  readJson,
  resultsDir,
  writeJson,
} from "./lib.mjs";

function loadOracles() {
  return readJson(join(resultsDir, "oracles.json"), emptyOracles());
}

function loadMission() {
  return readJson(join(resultsDir, "mission.json"), null);
}

function main() {
  const charter = readFileSync(join(exploratoryDir, "charter.md"), "utf8");
  const memory = readJson(memoryPath, emptyMemory());
  const oracles = loadOracles();
  const mission = loadMission();
  const budgetMinutes = Number(process.env.AJAX_EXPLORATORY_BUDGET_MINUTES ?? 12);
  const finalizationReserveMinutes = Number(process.env.AJAX_EXPLORATORY_FINALIZATION_MINUTES ?? 2);
  const { explorationMinutes, agentTimeoutSeconds } = computeAgentBudget({
    budgetMinutes,
    finalizationReserveMinutes,
  });
  const promptPath = join(resultsDir, "prompt.txt");

  if (!mission?.primary) {
    console.error("missing mission.json — run plan-mission.mjs first");
    process.exit(1);
  }

  const recentCommits = oracles.recentWebCommits ?? [];
  const commitsSummary =
    recentCommits.length === 0
      ? "No recent web commits in oracle pack; still execute the assigned mission."
      : "Recent web-related commits (bias probes within the mission, do not wander):";

  const prompt = `${charter}

---

## Run context

- Base URL: ${BASE_URL}
- Time budget: **${budgetMinutes} minutes maximum** nightly budget, enforced as **${explorationMinutes} minutes** of agent runtime after reserving **${finalizationReserveMinutes} minutes** for artifact finalization (findings, observations, memory-delta, run.json). Stop active exploration after ~${explorationMinutes} minutes or when stopping criteria apply.
- Use the Playwright MCP **WebKit** browser tools only (already launched for this run). Start at ${BASE_URL}/ (HTTPS; ignore certificate warnings).
- Optimize for information gained per action; reuse observations, memory, and oracles instead of rediscovering the same state.
- This workflow runs nightly with persisted memory — one run executes one assigned mission.
- Repository checkout is read-only for product source. Write only under exploratory-results/.
- Finalize artifacts incrementally (findings, observations, memory-delta) as you go so a budget stop still leaves useful output.
- In memory-delta.json, \`areasVisited\` must be an array of area **name strings** (\`cockpit\`, \`session\`, \`terminal\`, \`settings\`, \`diff-review\`, \`new-task\`, \`navigation\`, \`network\`, \`other\`).

## Assigned mission (primary)

Execute **one** mission this run. Do not rotate through every charter.

\`\`\`json
${JSON.stringify(mission.primary, null, 2)}
\`\`\`

Fallback mission (only if the primary is blocked by infrastructure during probing):

\`\`\`json
${JSON.stringify(mission.fallback, null, 2)}
\`\`\`

Mission selection since \`${mission.sinceSha ?? "first run"}\` at \`${mission.headSha ?? "unknown"}\`.

## Oracles (this run)

\`\`\`json
${JSON.stringify(oracles, null, 2)}
\`\`\`

## Exploration memory (adaptive hints)

\`\`\`json
${JSON.stringify(
  {
    lastRunSha: memory.lastRunSha,
    recentConfirmedFindings: (memory.confirmedFindings ?? []).slice(-10),
    dullActions: (memory.dullActions ?? []).slice(-20),
    recentObservations: (memory.observations ?? []).slice(-10),
    missionHistory: memory.missions ?? {},
  },
  null,
  2,
)}
\`\`\`

## Recent web commits

${commitsSummary}
${recentCommits.map((line) => `- ${line}`).join("\n")}

## Required finish checklist

Complete these when stopping — including an early stop when stopping criteria apply:

1. Ensure exploratory-results/findings.json and observations.json are valid.
2. Write exploratory-results/memory-delta.json with areas visited and next-run focus.
3. Update exploratory-results/run.json agent.status to completed (or failed with error).
4. Do not modify product source or git history.
5. Confirmed findings require **two** successful reset/reproduction cycles, a fingerprint, and evidence paths under exploratory-results/.
`;

  writeFileSync(promptPath, prompt);
  writeJson(join(resultsDir, "prompt-meta.json"), {
    budgetMinutes,
    finalizationReserveMinutes,
    explorationMinutes,
    agentTimeoutSeconds,
    baseUrl: BASE_URL,
    missionPrimary: mission.primary.id,
    missionFallback: mission.fallback.id,
    memoryLoaded: Boolean(memory.updatedAt || memory.lastRunSha),
    recentCommitCount: recentCommits.length,
    openBugCount: (oracles.openBugs ?? []).length,
    boundaryHashCount: (oracles.boundaryHashes ?? []).length,
  });

  console.log(promptPath);
}

main();
