#!/usr/bin/env node
// Merge agent memory-delta into the durable exploration corpus.
// Cache restore failure is fine: missing memory starts empty.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import {
  emptyMemory,
  evidenceSignature,
  FINDING_AREAS,
  memoryPath,
  readJson,
  repoRoot,
  resolveActiveMissionId,
  resultsDir,
  writeJson,
} from "./lib.mjs";
import { extractFingerprintFromBody } from "./file-issues.mjs";
import {
  emptyMissionMemory,
  resolveMissionOutcome,
  touchMissionMemory,
} from "./missions.mjs";

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

function mergeRegressionFingerprints(memory, oracles) {
  const seen = new Set((memory.regressions ?? []).map((item) => item.fingerprint));
  for (const issue of oracles.closedBugs ?? []) {
    const fingerprint = extractFingerprintFromBody(issue.body);
    if (!fingerprint || seen.has(fingerprint)) continue;
    memory.regressions.push({
      fingerprint,
      issueNumber: issue.number,
      title: issue.title,
      lastSeenAt: new Date().toISOString(),
    });
    seen.add(fingerprint);
  }
  memory.regressions = memory.regressions.slice(-50);
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
  const oracles = readJson(join(resultsDir, "oracles.json"), { closedBugs: [] });
  const run = readJson(join(resultsDir, "run.json"), {});
  const missionDoc = readJson(join(resultsDir, "mission.json"), null);
  const at = new Date().toISOString();
  const headSha = resolveHeadSha(run);
  const recommendedFocus =
    delta.recommendedFocus ?? delta.recommendedFocusNextRun ?? [];

  if (!memory.missions || Object.keys(memory.missions).length === 0) {
    memory.missions = emptyMissionMemory();
  }
  const missionId = resolveActiveMissionId(run, missionDoc);
  const missionOutcome = resolveMissionOutcome({
    run,
    memoryDelta: delta,
    findings: findingsDoc,
    observations: observationsDoc,
  });
  if (missionId) {
    touchMissionMemory(memory, missionId, headSha, at, missionOutcome);
  }

  mergeRegressionFingerprints(memory, oracles);

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
    const signature = evidenceSignature(finding);
    const existing = memory.confirmedFindings.find(
      (item) => item.fingerprint === fingerprint,
    );
    if (existing) {
      existing.lastSeenAt = at;
      existing.title = finding.title;
      existing.area = finding.area;
      if (signature) {
        existing.evidenceSignatures = uniquePush(existing.evidenceSignatures ?? [], signature, 5);
      }
    } else {
      memory.confirmedFindings.push({
        fingerprint,
        title: finding.title,
        area: finding.area,
        lastSeenAt: at,
        evidenceSignatures: signature ? [signature] : [],
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
    mission: missionId ?? null,
    missionOutcome,
    confirmed: (findingsDoc.findings ?? []).filter((f) => f.status === "confirmed")
      .length,
    observations: observationsCount,
    recommendedFocus,
    classification: run.classification?.counts ?? null,
  });
  memory.runs = memory.runs.slice(-14);
  memory.lastRunSha = headSha;
  memory.updatedAt = at;

  writeJson(memoryPath, memory);
  writeJson(join(resultsDir, "memory.json"), memory);
  console.log(`updated exploration memory at ${memoryPath}`);
}

main();
