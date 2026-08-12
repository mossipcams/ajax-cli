#!/usr/bin/env node
// Assemble the Cursor Agent prompt from charter + oracles + memory.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { emptyOracles } from "./prepare-oracles.mjs";
import {
  BASE_URL,
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

function routingOrBannerBugs(openBugs) {
  const pattern = /routing|route|hash|banner|#\/|navigate|404|redirect/i;
  return (openBugs ?? []).some((bug) => pattern.test(bug.title ?? ""));
}

function startingCharter(oracles) {
  if (routingOrBannerBugs(oracles.openBugs)) {
    return "**Garbage hashes** or **Contradiction** (open bugs mention routing/banners).";
  }
  return "**Happy path**, then rotate through the other charters.";
}

function main() {
  const charter = readFileSync(join(exploratoryDir, "charter.md"), "utf8");
  const memory = readJson(memoryPath, emptyMemory());
  const oracles = loadOracles();
  const budgetMinutes = Number(process.env.AJAX_EXPLORATORY_BUDGET_MINUTES ?? 25);
  const promptPath = join(resultsDir, "prompt.txt");

  const recentCommits = oracles.recentWebCommits ?? [];
  const commitsSummary =
    recentCommits.length === 0
      ? "No recent web commits in oracle pack; still run full charters."
      : "Recent web-related commits (bias charter focus, do not limit exploration):";

  const prompt = `${charter}

---

## Run context

- Base URL: ${BASE_URL}
- Time budget: **${budgetMinutes} minutes minimum**. Work one charter at a time for several minutes each; keep going until the runner stops you. Never spend the whole budget on a dashboard click-tour.
- Use the Playwright MCP browser tools. Start at ${BASE_URL}/ (HTTPS; ignore certificate warnings).
- Application under test is an isolated Ajax instance for this CI run only.
- Repository checkout is read-only for product source. Write only under exploratory-results/.
- Finalize artifacts incrementally (findings, observations, memory-delta) as you go so a budget stop still leaves useful output.
- In memory-delta.json, \`areasVisited\` must be an array of area **name strings** (\`cockpit\`, \`session\`, \`terminal\`, \`settings\`, \`diff-review\`, \`new-task\`, \`navigation\`, \`network\`, \`other\`). Do not replace \`run.headSha\` or wipe \`run.json\`; only update agent/summary fields you own.

## Oracles (this run)

\`\`\`json
${JSON.stringify(oracles, null, 2)}
\`\`\`

## Charter start

Start with ${startingCharter(oracles)} Then pick the next charter from oracles + what you just observed — not from a coverage checklist.

If this is a relaunch, read existing \`exploratory-results/\` (findings, observations, memory-delta) and **continue the current charter** or start the **next** one. Do not restart a coverage tour.

## Exploration memory (adaptive hints)

\`\`\`json
${JSON.stringify(
  {
    lastRunSha: memory.lastRunSha,
    recentConfirmedFindings: (memory.confirmedFindings ?? []).slice(-10),
    dullActions: (memory.dullActions ?? []).slice(-20),
    recentObservations: (memory.observations ?? []).slice(-10),
  },
  null,
  2,
)}
\`\`\`

## Recent web commits

${commitsSummary}
${recentCommits.map((line) => `- ${line}`).join("\n")}

## Required finish checklist

Keep these current as you go. They are not permission to stop early; the runner decides when time is up.

1. Ensure exploratory-results/findings.json and observations.json are valid.
2. Write exploratory-results/memory-delta.json with areas visited and next-run focus.
3. Update exploratory-results/run.json agent.status to completed (or failed with error).
4. Do not modify product source or git history.
`;

  writeFileSync(promptPath, prompt);
  writeJson(join(resultsDir, "prompt-meta.json"), {
    budgetMinutes,
    baseUrl: BASE_URL,
    memoryLoaded: Boolean(memory.updatedAt || memory.lastRunSha),
    recentCommitCount: recentCommits.length,
    openBugCount: (oracles.openBugs ?? []).length,
    boundaryHashCount: (oracles.boundaryHashes ?? []).length,
  });

  console.log(promptPath);
}

main();
