#!/usr/bin/env node
// Merge agent memory-delta into the durable exploration corpus.
// Cache restore failure is fine: missing memory starts empty.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import {
  emptyMemory,
  FINDING_AREAS,
  memoryPath,
  readJson,
  repoRoot,
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

function normalizeAreaName(entry) {
  if (typeof entry === "string") {
    const trimmed = entry.trim();
    return trimmed || null;
  }
  if (entry && typeof entry === "object" && typeof entry.area === "string") {
    const trimmed = entry.area.trim();
    return trimmed || null;
  }
  return null;
}

function resolveArea(memory, raw) {
  if (!raw || raw === "[object Object]") return null;
  if (FINDING_AREAS.has(raw)) return raw;
  if (memory.areas[raw]) return raw;
  return "other";
}

function uniquePush(list, value, max) {
  if (!value) return;
  const next = list.filter((item) => item !== value);
  next.push(value);
  return next.slice(-max);
}

function resolveHeadSha(run) {
  if (run.headSha) return run.headSha;
  if (run.repoSha) return run.repoSha;
  try {
    return execFileSync("git", ["rev-parse", "HEAD"], {
      cwd: repoRoot,
      encoding: "utf8",
    }).trim();
  } catch {
    return null;
  }
}

function mergeObservation(memory, summary, area, at) {
  if (!summary) return;
  const existing = memory.observations.find((item) => item.summary === summary);
  if (existing) {
    existing.count += 1;
    existing.lastSeenAt = at;
    if (area && FINDING_AREAS.has(area)) existing.area = area;
  } else {
    memory.observations.push({
      summary,
      area: area && FINDING_AREAS.has(area) ? area : "other",
      count: 1,
      lastSeenAt: at,
    });
  }
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
  const observationsDoc = readJson(join(resultsDir, "observations.json"), {
    version: 1,
    observations: [],
  });
  const run = readJson(join(resultsDir, "run.json"), {});
  const at = new Date().toISOString();
  const headSha = resolveHeadSha(run);
  const recommendedFocus =
    delta.recommendedFocus ?? delta.recommendedFocusNextRun ?? [];

  for (const entry of delta.areasVisited ?? []) {
    const raw = normalizeAreaName(entry);
    const area = resolveArea(memory, raw);
    if (area) bumpArea(memory, area, at);
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

  const observationSummaries = new Set();
  for (const finding of findingsDoc.findings ?? []) {
    if (finding.status !== "observation") continue;
    const summary = finding.title;
    observationSummaries.add(summary);
    mergeObservation(memory, summary, finding.area, at);
  }
  for (const item of observationsDoc.observations ?? []) {
    const summary = item?.summary ?? item?.title;
    if (!summary) continue;
    observationSummaries.add(summary);
    mergeObservation(memory, summary, item.area, at);
  }
  memory.observations = memory.observations.slice(-50);

  const observationFindingCount = (findingsDoc.findings ?? []).filter(
    (f) => f.status === "observation",
  ).length;
  const observationsJsonCount = (observationsDoc.observations ?? []).length;
  const observationsCount = observationSummaries.size || observationsJsonCount + observationFindingCount;

  memory.runs.push({
    at,
    sha: headSha,
    confirmed: (findingsDoc.findings ?? []).filter((f) => f.status === "confirmed")
      .length,
    observations: observationsCount,
    recommendedFocus,
  });
  memory.runs = memory.runs.slice(-14);
  memory.lastRunSha = headSha;
  memory.updatedAt = at;

  writeJson(memoryPath, memory);
  writeJson(join(resultsDir, "memory.json"), memory);
  console.log(`updated exploration memory at ${memoryPath}`);
}

main();
