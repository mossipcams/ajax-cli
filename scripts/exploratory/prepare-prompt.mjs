#!/usr/bin/env node
// Assemble the Cursor Agent prompt from charter + memory + recent changes.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  BASE_URL,
  emptyMemory,
  exploratoryDir,
  memoryPath,
  readJson,
  repoRoot,
  resultsDir,
  writeJson,
} from "./lib.mjs";

function recentChanges(lastRunSha) {
  if (!lastRunSha) {
    return {
      available: false,
      summary: "No previous exploratory SHA in memory; treat this as a broad first pass.",
      commits: [],
    };
  }

  try {
    const range = `${lastRunSha}..HEAD`;
    const log = execFileSync(
      "git",
      [
        "log",
        "--oneline",
        "--no-merges",
        range,
        "--",
        "crates/ajax-web",
        "crates/ajax-cli/src/web_backend.rs",
        "crates/ajax-cli/src/cockpit_backend.rs",
        "docs/architecture/web-cockpit.md",
      ],
      { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
    )
      .trim()
      .split("\n")
      .filter(Boolean)
      .slice(0, 40);

    return {
      available: true,
      summary:
        log.length === 0
          ? `No web-related commits since ${lastRunSha}; still explore broadly and sample underexplored areas.`
          : `Web-related commits since ${lastRunSha} (influence priority, do not limit exploration):`,
      commits: log,
    };
  } catch {
    return {
      available: false,
      summary:
        "Could not compute recent changes from memory SHA; explore broadly without change bias.",
      commits: [],
    };
  }
}

function underexploredAreas(memory) {
  return Object.entries(memory.areas ?? {})
    .map(([area, info]) => ({ area, visits: info?.visits ?? 0 }))
    .sort((a, b) => a.visits - b.visits)
    .slice(0, 5);
}

function main() {
  const charter = readFileSync(join(exploratoryDir, "charter.md"), "utf8");
  const memory = readJson(memoryPath, emptyMemory());
  const changes = recentChanges(memory.lastRunSha);
  const budgetMinutes = Number(process.env.AJAX_EXPLORATORY_BUDGET_MINUTES ?? 25);

  const underexplored = underexploredAreas(memory);
  const promptPath = join(resultsDir, "prompt.txt");

  const prompt = `${charter}

---

## Run context

- Base URL: ${BASE_URL}
- Explore for about ${budgetMinutes} minutes, then stop and finalize artifacts.
- Use the Playwright MCP browser tools. Start at ${BASE_URL}/ (HTTPS; ignore certificate warnings).
- Application under test is an isolated Ajax instance for this CI run only.
- Repository checkout is read-only for product source. Write only under exploratory-results/.

## Exploration memory (adaptive hints)

\`\`\`json
${JSON.stringify(
  {
    lastRunSha: memory.lastRunSha,
    underexploredAreas: underexplored,
    recentConfirmedFindings: (memory.confirmedFindings ?? []).slice(-10),
    dullActions: (memory.dullActions ?? []).slice(-20),
    recentObservations: (memory.observations ?? []).slice(-10),
  },
  null,
  2,
)}
\`\`\`

## Recent changes

${changes.summary}
${changes.commits.map((line) => `- ${line}`).join("\n")}

## Required finish checklist

1. Ensure exploratory-results/findings.json and observations.json are valid.
2. Write exploratory-results/memory-delta.json with areas visited and next-run focus.
3. Update exploratory-results/run.json agent.status to completed (or failed with error).
4. Stop without modifying product source or git history.
`;

  writeFileSync(promptPath, prompt);
  writeJson(join(resultsDir, "prompt-meta.json"), {
    budgetMinutes,
    baseUrl: BASE_URL,
    memoryLoaded: Boolean(memory.updatedAt || memory.lastRunSha),
    recentChangeCount: changes.commits.length,
  });

  console.log(promptPath);
}

main();
