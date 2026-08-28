// Shared paths and helpers for CI-only Ajax Web exploratory testing.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join, resolve, relative, isAbsolute } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import findingsSchema from "../../.github/exploratory/findings.schema.json" with { type: "json" };

export const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
export const exploratoryDir = join(repoRoot, ".github", "exploratory");
export const resultsDir =
  process.env.AJAX_EXPLORATORY_RESULTS || join(repoRoot, "exploratory-results");
export const instanceDir =
  process.env.AJAX_EXPLORATORY_INSTANCE ||
  join(repoRoot, "target", "exploratory-instance");
export const memoryPath =
  process.env.AJAX_EXPLORATORY_MEMORY ||
  join(repoRoot, "exploratory-memory", "memory.json");
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

export const MISSION_COOLDOWN_RUNS = 2;
export const MISSION_NO_SIGNAL_COOLDOWN_MS = 48 * 60 * 60 * 1000;

export function emptyMemory() {
  return {
    version: 1,
    updatedAt: null,
    lastRunSha: null,
    runs: [],
    missions: {},
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
    regressions: [],
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

export const FINDING_AREAS = new Set([
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
const IMAGE_EXT_RE = /\.(png|jpe?g|webp)$/i;

function normalizeStepsArray(value) {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed ? [trimmed] : [];
  }
  if (!Array.isArray(value)) return [];
  return value.map((step) => String(step).trim()).filter(Boolean);
}

function extractSteps(finding) {
  for (const key of ["steps", "reproSteps", "reproductionSteps"]) {
    const steps = normalizeStepsArray(finding[key]);
    if (steps.length > 0) return steps;
  }
  return [];
}

function normalizeEvidence(evidence) {
  if (evidence && typeof evidence === "object" && !Array.isArray(evidence)) {
    return {
      ...evidence,
      screenshots: Array.isArray(evidence.screenshots) ? evidence.screenshots : [],
      consoleErrors: Array.isArray(evidence.consoleErrors) ? evidence.consoleErrors : [],
      networkFailures: Array.isArray(evidence.networkFailures) ? evidence.networkFailures : [],
    };
  }
  if (Array.isArray(evidence)) {
    const screenshots = [];
    const otherNotes = [];
    for (const item of evidence) {
      const path = String(item).trim();
      if (!path) continue;
      if (IMAGE_EXT_RE.test(path)) {
        screenshots.push(path);
      } else {
        otherNotes.push(path);
      }
    }
    const result = { screenshots, consoleErrors: [], networkFailures: [] };
    if (otherNotes.length > 0) {
      result.notes = otherNotes.join("\n");
    }
    return result;
  }
  return {};
}

function nonEmptyString(value) {
  if (value === undefined || value === null) return "";
  const trimmed = String(value).trim();
  return trimmed;
}

function coerceExpectedActual(finding, status, evidence) {
  let expected = nonEmptyString(finding.expected);
  let actual = nonEmptyString(finding.actual);

  if (!actual) {
    if (nonEmptyString(finding.title)) {
      actual = nonEmptyString(finding.title);
    } else if (nonEmptyString(evidence?.notes)) {
      actual = nonEmptyString(evidence.notes);
    }
  }

  if (!expected) {
    if (status === "observation" || status === "rejected") {
      expected = "Not yet characterized (observation only; reproduction pending)";
    }
  }

  return { expected, actual };
}

export function normalizeFinding(finding) {
  if (!finding || typeof finding !== "object") return null;

  const steps = extractSteps(finding);

  let status = STATUSES.has(finding.status) ? finding.status : "observation";

  const confidence = CONFIDENCE.has(finding.confidence)
    ? finding.confidence
    : status === "confirmed"
      ? "high"
      : status === "rejected"
        ? "low"
        : "medium";

  const area = FINDING_AREAS.has(finding.area) ? finding.area : "other";
  const severity = SEVERITY.has(finding.severity) ? finding.severity : "medium";

  let reproductionAttempts =
    typeof finding.reproductionAttempts === "number"
      ? Math.max(0, Math.floor(finding.reproductionAttempts))
      : steps.length > 0
        ? 1
        : 0;

  let reproductionSuccesses =
    typeof finding.reproductionSuccesses === "number"
      ? Math.max(0, Math.floor(finding.reproductionSuccesses))
      : 0;

  if (status === "confirmed" && steps.length === 0) {
    status = "observation";
    reproductionAttempts = 0;
    reproductionSuccesses = 0;
  }

  if (status === "confirmed" && steps.length > 0 && reproductionSuccesses === 0) {
    reproductionAttempts = Math.max(reproductionAttempts, steps.length > 0 ? 1 : 0);
  }

  if (reproductionSuccesses > reproductionAttempts) {
    reproductionSuccesses = reproductionAttempts;
  }

  const evidence = normalizeEvidence(finding.evidence);
  const { expected, actual } = coerceExpectedActual(finding, status, evidence);

  const normalized = {
    id: finding.id,
    title: finding.title,
    status,
    confidence,
    area,
    severity,
    reproductionAttempts,
    reproductionSuccesses,
    steps,
    expected,
    actual,
    evidence,
  };

  if (finding.fingerprint) normalized.fingerprint = finding.fingerprint;

  if (
    Array.isArray(finding.relatedIssues) &&
    finding.relatedIssues.every((value) => typeof value === "number")
  ) {
    normalized.relatedIssues = finding.relatedIssues;
  }

  return normalized;
}

export function normalizeFindingsDocument(doc) {
  if (!doc || typeof doc !== "object") {
    return { version: 1, findings: [] };
  }
  if (!Array.isArray(doc.findings)) {
    return { version: 1, findings: [] };
  }
  return {
    version: 1,
    findings: doc.findings.map(normalizeFinding).filter(Boolean),
  };
}

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
    if (finding?.status === "confirmed" && (!Array.isArray(finding?.steps) || finding.steps.length < 1)) {
      problems.push(`${prefix}.steps must be a non-empty array`);
    } else if (finding?.steps != null && !Array.isArray(finding.steps)) {
      problems.push(`${prefix}.steps must be an array`);
    }
    if (
      typeof finding?.reproductionAttempts === "number" &&
      typeof finding?.reproductionSuccesses === "number" &&
      finding.reproductionSuccesses > finding.reproductionAttempts
    ) {
      problems.push(`${prefix}.reproductionSuccesses exceeds attempts`);
    }
    if (finding?.status === "confirmed" && finding.reproductionSuccesses < 2) {
      problems.push(`${prefix}: confirmed findings need ≥2 successful reproduction cycles`);
    }
    if (finding?.status === "confirmed") {
      const evidenceProblems = validateConfirmedEvidence(finding, prefix);
      problems.push(...evidenceProblems);
    }
  });

  return problems;
}

const EVIDENCE_PREFIX = "exploratory-results/";

function resolveEvidenceRelativePath(rawPath, prefix) {
  const problems = [];
  const path = String(rawPath ?? "").trim();
  if (!path) return { problems, relativePath: null, absolutePath: null };

  if (isAbsolute(path)) {
    problems.push(`${prefix}: evidence path must be relative, not absolute (${path})`);
    return { problems, relativePath: null, absolutePath: null };
  }
  if (path.includes("..")) {
    problems.push(`${prefix}: evidence path must not contain traversal segments (${path})`);
    return { problems, relativePath: null, absolutePath: null };
  }
  if (!path.startsWith(EVIDENCE_PREFIX)) {
    problems.push(`${prefix}: evidence paths must live under ${EVIDENCE_PREFIX}`);
    return { problems, relativePath: null, absolutePath: null };
  }

  const absolutePath = resolve(repoRoot, path);
  const rel = relative(resultsDir, absolutePath);
  if (rel.startsWith("..") || isAbsolute(rel)) {
    problems.push(`${prefix}: evidence path escapes exploratory-results (${path})`);
    return { problems, relativePath: null, absolutePath: null };
  }
  return { problems, relativePath: path, absolutePath: absolutePath };
}

function validateConfirmedEvidence(finding, prefix) {
  const problems = [];
  if (!finding.fingerprint || String(finding.fingerprint).trim() === "") {
    problems.push(`${prefix}.fingerprint is required for confirmed findings`);
  }

  const tracePath = finding.evidence?.trace;
  const screenshots = finding.evidence?.screenshots ?? [];
  const resolvedFiles = [];

  if (tracePath) {
    const resolved = resolveEvidenceRelativePath(tracePath, prefix);
    problems.push(...resolved.problems);
    if (resolved.absolutePath) resolvedFiles.push(resolved.absolutePath);
  }
  for (const screenshot of screenshots) {
    const resolved = resolveEvidenceRelativePath(screenshot, prefix);
    problems.push(...resolved.problems);
    if (resolved.absolutePath) resolvedFiles.push(resolved.absolutePath);
  }

  if (resolvedFiles.length === 0) {
    problems.push(`${prefix}: confirmed findings need at least one trace or screenshot file path`);
  } else {
    for (const absolutePath of resolvedFiles) {
      if (!existsSync(absolutePath)) {
        problems.push(`${prefix}: missing evidence file ${relative(repoRoot, absolutePath)}`);
      }
    }
  }

  return problems;
}

let findingsValidator = null;

function getFindingsValidator() {
  if (!findingsValidator) {
    const ajv = new Ajv2020({ allErrors: true, strict: false });
    findingsValidator = ajv.compile(findingsSchema);
  }
  return findingsValidator;
}

export function validateFindingsSchema(doc) {
  const validate = getFindingsValidator();
  const ok = validate(doc);
  if (ok) return [];
  return (validate.errors ?? []).map((error) => {
    const path = error.instancePath || "(root)";
    return `schema ${path}: ${error.message ?? "invalid"}`;
  });
}

export function evidenceSignature(finding) {
  const parts = [];
  const evidence = finding?.evidence ?? {};
  if (evidence.trace) parts.push(`trace:${evidence.trace}`);
  for (const shot of evidence.screenshots ?? []) parts.push(`shot:${shot}`);
  for (const err of evidence.consoleErrors ?? []) parts.push(`console:${err}`);
  for (const fail of evidence.networkFailures ?? []) parts.push(`net:${fail}`);
  if (evidence.notes) parts.push(`notes:${evidence.notes}`);
  return createHash("sha256").update(parts.join("|")).digest("hex").slice(0, 16);
}

export function missionAreaFromDoc(missionDoc, run = {}) {
  const missionId = run.mission?.primary ?? missionDoc?.primary?.id;
  if (!missionId) return null;
  return missionDoc?.primary?.area ?? null;
}

export function assessMissionCompletion({
  missionDoc = null,
  run = {},
  memoryDelta = {},
  findings = { findings: [] },
  observations = { observations: [] },
} = {}) {
  const missionArea = missionAreaFromDoc(missionDoc, run);
  if (!missionArea) {
    return { ok: true, reason: null };
  }

  if (memoryDelta.missionCompleted === true) {
    return { ok: true, reason: null };
  }

  const areas = (memoryDelta.areasVisited ?? [])
    .map((entry) => (typeof entry === "string" ? entry : entry?.area))
    .filter(Boolean);
  if (areas.includes(missionArea)) {
    return { ok: true, reason: null };
  }

  const findingCount = (findings.findings ?? []).length;
  const observationCount =
    (observations.observations ?? []).length +
    (findings.findings ?? []).filter((f) => f.status === "observation").length;
  if (findingCount > 0 || observationCount > 0) {
    return { ok: true, reason: null };
  }

  return {
    ok: false,
    reason: `mission ${run.mission?.primary ?? missionDoc?.primary?.id} incomplete (no areasVisited, findings, or observations)`,
  };
}

export function assessRunUsefulness({
  run = {},
  memoryDelta = {},
  findings = { findings: [] },
  observations = { observations: [] },
  missionDoc = null,
} = {}) {
  if (run.preflight?.status === "blocked") {
    return { ok: false, reason: run.preflight.error ?? "preflight blocked" };
  }
  if (run.agent?.status === "failed" || run.infrastructure?.status === "failed") {
    return { ok: false, reason: run.infrastructure?.error ?? "infrastructure failed" };
  }
  const areas = Array.isArray(memoryDelta.areasVisited) ? memoryDelta.areasVisited : [];
  const findingCount = findings.findings?.length ?? 0;
  const observationCount = observations.observations?.length ?? 0;
  const hasAreas = areas.length > 0;
  if (!hasAreas && findingCount === 0 && observationCount === 0) {
    return { ok: false, reason: "empty exploration skeleton (no areas, findings, or observations)" };
  }
  const mission = assessMissionCompletion({ missionDoc, run, memoryDelta, findings, observations });
  if (!mission.ok) {
    return mission;
  }
  return { ok: true, reason: null };
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
