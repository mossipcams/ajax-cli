// Shared paths and helpers for CI-only Ajax Web exploratory testing.

import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

export const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const exploratoryDir = join(repoRoot, ".github", "exploratory");
export const resultsDir = join(repoRoot, "exploratory-results");
export const instanceDir = join(repoRoot, "target", "exploratory-instance");
export const memoryPath = join(repoRoot, "exploratory-memory", "memory.json");
export const BASE_URL = process.env.AJAX_EXPLORATORY_BASE_URL ?? "https://127.0.0.1:18790";
export const PORT = Number(process.env.AJAX_EXPLORATORY_PORT ?? 18790);

export function ensureDir(path) {
  mkdirSync(path, { recursive: true });
}

export function readJson(path, fallback = null) {
  if (!existsSync(path)) return fallback;
  return JSON.parse(readFileSync(path, "utf8"));
}

export function writeJson(path, value) {
  ensureDir(dirname(path));
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

export function emptyFindings() {
  return { version: 1, findings: [] };
}

export function emptyObservations() {
  return { version: 1, observations: [] };
}

export function emptyMemory() {
  return {
    version: 1,
    updatedAt: null,
    lastRunSha: null,
    runs: [],
    areas: {
      cockpit: { visits: 0, lastVisitedAt: null },
      session: { visits: 0, lastVisitedAt: null },
      terminal: { visits: 0, lastVisitedAt: null },
      settings: { visits: 0, lastVisitedAt: null },
      "diff-review": { visits: 0, lastVisitedAt: null },
      "new-task": { visits: 0, lastVisitedAt: null },
      navigation: { visits: 0, lastVisitedAt: null },
      network: { visits: 0, lastVisitedAt: null },
      other: { visits: 0, lastVisitedAt: null },
    },
    confirmedFindings: [],
    observations: [],
    dullActions: [],
  };
}

export function seedResultsSkeleton(runMeta) {
  ensureDir(join(resultsDir, "traces"));
  ensureDir(join(resultsDir, "screenshots"));
  ensureDir(join(resultsDir, "logs"));
  writeJson(join(resultsDir, "run.json"), runMeta);
  writeJson(join(resultsDir, "findings.json"), emptyFindings());
  writeJson(join(resultsDir, "observations.json"), emptyObservations());
  writeJson(join(resultsDir, "memory-delta.json"), {
    version: 1,
    areasVisited: [],
    dullActions: [],
    confirmedFindingFingerprints: [],
    recommendedFocus: [],
    notes: "",
  });
}

const FINDING_AREAS = new Set([
  "cockpit",
  "session",
  "terminal",
  "settings",
  "diff-review",
  "new-task",
  "navigation",
  "network",
  "other",
]);
const STATUSES = new Set(["confirmed", "observation", "rejected"]);
const CONFIDENCE = new Set(["low", "medium", "high"]);
const SEVERITY = new Set(["low", "medium", "high", "critical"]);

export function validateFindingsDocument(doc) {
  const problems = [];
  if (!doc || typeof doc !== "object") {
    return ["findings document must be an object"];
  }
  if (doc.version !== 1) problems.push("findings.version must be 1");
  if (!Array.isArray(doc.findings)) {
    problems.push("findings.findings must be an array");
    return problems;
  }

  doc.findings.forEach((finding, index) => {
    const prefix = `findings[${index}]`;
    for (const key of [
      "id",
      "title",
      "status",
      "confidence",
      "area",
      "severity",
      "reproductionAttempts",
      "reproductionSuccesses",
      "steps",
      "expected",
      "actual",
      "evidence",
    ]) {
      if (finding?.[key] === undefined || finding?.[key] === null) {
        problems.push(`${prefix}.${key} is required`);
      }
    }
    if (finding?.title !== undefined && String(finding.title).trim() === "") {
      problems.push(`${prefix}.title must be non-empty`);
    }
    if (finding?.status && !STATUSES.has(finding.status)) {
      problems.push(`${prefix}.status invalid`);
    }
    if (finding?.confidence && !CONFIDENCE.has(finding.confidence)) {
      problems.push(`${prefix}.confidence invalid`);
    }
    if (finding?.area && !FINDING_AREAS.has(finding.area)) {
      problems.push(`${prefix}.area invalid`);
    }
    if (finding?.severity && !SEVERITY.has(finding.severity)) {
      problems.push(`${prefix}.severity invalid`);
    }
    if (!Array.isArray(finding?.steps) || finding.steps.length < 1) {
      problems.push(`${prefix}.steps must be a non-empty array`);
    }
    if (
      typeof finding?.reproductionAttempts === "number" &&
      typeof finding?.reproductionSuccesses === "number" &&
      finding.reproductionSuccesses > finding.reproductionAttempts
    ) {
      problems.push(`${prefix}.reproductionSuccesses exceeds attempts`);
    }
    if (finding?.status === "confirmed" && finding.reproductionSuccesses < 1) {
      problems.push(`${prefix}: confirmed findings need ≥1 successful reproduction`);
    }
  });

  return problems;
}

export function simulatedFinding() {
  return {
    id: "sim-reconnect-composer",
    title: "Session becomes unusable after reconnect",
    status: "confirmed",
    confidence: "high",
    area: "session",
    severity: "medium",
    reproductionAttempts: 2,
    reproductionSuccesses: 2,
    steps: [
      "Create a new session",
      "Disconnect",
      "Reconnect",
      "Send another message",
    ],
    expected: "Message should be sent successfully",
    actual: "Composer remains pending indefinitely",
    evidence: {
      url: `${BASE_URL}/`,
      trace: "exploratory-results/traces/sim-reconnect.zip",
      screenshots: ["exploratory-results/screenshots/sim-reconnect.png"],
      consoleErrors: [],
      networkFailures: [],
      notes: "Synthetic fixture used to validate artifact schema.",
    },
    fingerprint: "session|composer-pending-after-reconnect",
  };
}
