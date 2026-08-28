#!/usr/bin/env node
// Classify validated findings as novel, known, or regression before filing/memory.

import { join } from "node:path";
import { fileURLToPath } from "node:url";
import {
  buildIssueTitle,
  extractFingerprintFromBody,
  fingerprintForFinding,
  normalizeIssueTitle,
} from "./file-issues.mjs";
import {
  emptyFindings,
  emptyVerifierDocument,
  hasIndependentVerifierEvidence,
  memoryPath,
  readJson,
  resultsDir,
  validateFindingsSchema,
  writeJson,
} from "./lib.mjs";

function issueMatches(finding, issue, fingerprint, normalizedTitle) {
  const bodyFingerprint = extractFingerprintFromBody(issue.body);
  if (bodyFingerprint && bodyFingerprint === fingerprint) return true;
  const issueTitle = normalizeIssueTitle(issue.title);
  if (issueTitle && issueTitle === normalizedTitle) return true;
  if (issue.title && issue.title.toLowerCase() === buildIssueTitle(finding).toLowerCase()) return true;
  return false;
}

export function classifyFinding(finding, { openBugs = [], closedBugs = [], memory = {} } = {}) {
  const fingerprint = finding.fingerprint || fingerprintForFinding(finding);
  const normalizedTitle = normalizeIssueTitle(finding.title);

  for (const issue of openBugs) {
    if (issueMatches(finding, issue, fingerprint, normalizedTitle)) return "known";
  }

  for (const issue of closedBugs) {
    if (issueMatches(finding, issue, fingerprint, normalizedTitle)) return "regression";
  }

  const memoryHit = (memory.confirmedFindings ?? []).some(
    (item) => item.fingerprint === fingerprint,
  );
  if (memoryHit) return "known";

  const regressionHit = (memory.regressions ?? []).some(
    (item) => item.fingerprint === fingerprint,
  );
  if (regressionHit) return "regression";

  return "novel";
}

export function classifyFindings(doc, { openBugs = [], closedBugs = [], memory = {}, verifierDoc = null } = {}) {
  const findings = (doc.findings ?? []).map((finding) => ({
    ...finding,
    classification:
      finding.status === "confirmed" && hasIndependentVerifierEvidence(finding, verifierDoc)
        ? classifyFinding(finding, { openBugs, closedBugs, memory })
        : undefined,
  }));
  return { version: 1, findings };
}

export function main() {
  const findingsDoc = readJson(join(resultsDir, "findings.json"), emptyFindings());
  const oracles = readJson(join(resultsDir, "oracles.json"), { openBugs: [], closedBugs: [] });
  const memory = readJson(memoryPath, {});
  const verifierDoc = readJson(join(resultsDir, "verifier.json"), emptyVerifierDocument());

  const classified = classifyFindings(findingsDoc, {
    openBugs: oracles.openBugs ?? [],
    closedBugs: oracles.closedBugs ?? [],
    memory,
    verifierDoc,
  });
  const schemaProblems = validateFindingsSchema(classified);
  if (schemaProblems.length > 0) {
    for (const problem of schemaProblems) console.error(problem);
    process.exit(1);
  }
  writeJson(join(resultsDir, "findings.json"), classified);

  const run = readJson(join(resultsDir, "run.json"), {});
  run.classification = {
    at: new Date().toISOString(),
    counts: classified.findings.reduce(
      (acc, finding) => {
        if (finding.classification) acc[finding.classification] += 1;
        return acc;
      },
      { novel: 0, known: 0, regression: 0 },
    ),
  };
  writeJson(join(resultsDir, "run.json"), run);
  console.log(JSON.stringify(run.classification, null, 2));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  main();
}
