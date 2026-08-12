#!/usr/bin/env node
// Merge agent memory-delta into the durable exploration corpus.
// Cache restore failure is fine: missing memory starts empty.

import { join } from "node:path";
import {
  emptyMemory,
  memoryPath,
  readJson,
  resultsDir,
  writeJson,
} from "./lib.mjs";

function bumpArea(memory, area, at) {
  if (!memory.areas[area]) {
    memory.areas[area] = { visits: 0, lastVisitedAt: null };
  }
  memory.areas[area].visits += 1;
  memory.areas[area].lastVisitedAt = at;
}

function uniquePush(list, value, max) {
  if (!value) return;
  const next = list.filter((item) => item !== value);
  next.push(value);
  return next.slice(-max);
}

function main() {
  const memory = readJson(memoryPath, emptyMemory());
  const delta = readJson(join(resultsDir, "memory-delta.json"), {
    version: 1,
    areasVisited: [],
    dullActions: [],
    confirmedFindingFingerprints: [],
    recommendedFocus: [],
    notes: "",
  });
  const findingsDoc = readJson(join(resultsDir, "findings.json"), {
    version: 1,
    findings: [],
  });
  const run = readJson(join(resultsDir, "run.json"), {});
  const at = new Date().toISOString();
  const headSha = run.headSha ?? null;

  for (const area of delta.areasVisited ?? []) {
    bumpArea(memory, area, at);
  }

  for (const action of delta.dullActions ?? []) {
    memory.dullActions = uniquePush(memory.dullActions, action, 40);
  }

  for (const finding of findingsDoc.findings ?? []) {
    if (finding.status !== "confirmed") continue;
    const fingerprint =
      finding.fingerprint ||
      `${finding.area}|${String(finding.title).toLowerCase().replace(/\s+/g, "-")}`;
    const existing = memory.confirmedFindings.find(
      (item) => item.fingerprint === fingerprint,
    );
    if (existing) {
      existing.lastSeenAt = at;
      existing.title = finding.title;
      existing.area = finding.area;
    } else {
      memory.confirmedFindings.push({
        fingerprint,
        title: finding.title,
        area: finding.area,
        lastSeenAt: at,
      });
    }
  }
  memory.confirmedFindings = memory.confirmedFindings.slice(-50);

  for (const finding of findingsDoc.findings ?? []) {
    if (finding.status !== "observation") continue;
    const summary = finding.title;
    const existing = memory.observations.find((item) => item.summary === summary);
    if (existing) {
      existing.count += 1;
      existing.lastSeenAt = at;
    } else {
      memory.observations.push({
        summary,
        area: finding.area,
        count: 1,
        lastSeenAt: at,
      });
    }
  }
  memory.observations = memory.observations.slice(-50);

  memory.runs.push({
    at,
    sha: headSha,
    confirmed: (findingsDoc.findings ?? []).filter((f) => f.status === "confirmed")
      .length,
    observations: (findingsDoc.findings ?? []).filter((f) => f.status === "observation")
      .length,
    recommendedFocus: delta.recommendedFocus ?? [],
  });
  memory.runs = memory.runs.slice(-14);
  memory.lastRunSha = headSha;
  memory.updatedAt = at;

  writeJson(memoryPath, memory);
  writeJson(join(resultsDir, "memory.json"), memory);
  console.log(`updated exploration memory at ${memoryPath}`);
}

main();
