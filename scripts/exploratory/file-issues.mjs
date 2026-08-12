#!/usr/bin/env node
// File duplicate-aware GitHub Defect issues for confirmed exploratory findings.

import { execFileSync } from "node:child_process";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { emptyFindings, readJson, resultsDir, writeJson } from "./lib.mjs";

const DEFAULT_REPO = "mossipcams/ajax-cli";
const FINGERPRINT_COMMENT_RE = /<!--\s*exploratory-fingerprint:\s*(.+?)\s*-->/;

export function parseArgs(argv) {
  return {
    dryRun: argv.includes("--dry-run"),
    force: argv.includes("--force"),
  };
}

export function fingerprintForFinding(finding) {
  if (finding.fingerprint) return finding.fingerprint;
  const slug = String(finding.title)
    .toLowerCase()
    .trim()
    .replace(/\s+/g, "-");
  return `${finding.area}|${slug}`;
}

export function mapSeverity(severity) {
  if (severity === "critical") return "blocker";
  if (severity === "high" || severity === "medium" || severity === "low") {
    return severity;
  }
  return "medium";
}

export function buildIssueTitle(finding) {
  return `[defect] Web Cockpit ${finding.title}`;
}

export function formatSteps(steps) {
  return steps.map((step, index) => `${index + 1}. ${step}`).join("\n");
}

export function formatEvidenceNotes(evidence = {}) {
  const lines = [];
  if (evidence.notes) lines.push(evidence.notes);
  if (Array.isArray(evidence.consoleErrors) && evidence.consoleErrors.length > 0) {
    lines.push(`Console errors: ${evidence.consoleErrors.join("; ")}`);
  }
  if (Array.isArray(evidence.networkFailures) && evidence.networkFailures.length > 0) {
    lines.push(`Network failures: ${evidence.networkFailures.join("; ")}`);
  }
  return lines.join("\n");
}

export function buildIssueBody(finding, { fingerprint, version, runUrl, artifactName }) {
  const evidenceNotes = formatEvidenceNotes(finding.evidence);
  const notes = [
    `- Exploratory fingerprint: \`${fingerprint}\``,
    `- Actions run: ${runUrl || "unknown"}`,
    `- Artifact: ${artifactName}`,
  ];
  if (evidenceNotes) {
    notes.push(`- ${evidenceNotes.replace(/\n/g, "\n- ")}`);
  }

  return `### Summary
${finding.title}

### Surface
Web Cockpit

### Steps to reproduce
${formatSteps(finding.steps)}

### Expected
${finding.expected}

### Actual
${finding.actual}

### Version / commit
${version}

### Severity
${mapSeverity(finding.severity)}

### Notes
${notes.join("\n")}
<!-- exploratory-fingerprint: ${fingerprint} -->
`;
}

export function extractFingerprintFromBody(body) {
  const match = FINGERPRINT_COMMENT_RE.exec(body ?? "");
  return match ? match[1].trim() : null;
}

export function titleContainsFinding(issueTitle, findingTitle) {
  return issueTitle.toLowerCase().includes(findingTitle.toLowerCase());
}

export function isEligibleFinding(finding) {
  return finding.status === "confirmed" && finding.reproductionSuccesses >= 1;
}

export function findDuplicate(openIssues, finding, fingerprint) {
  const issueTitle = buildIssueTitle(finding);
  for (const issue of openIssues) {
    const bodyFingerprint = extractFingerprintFromBody(issue.body);
    if (bodyFingerprint === fingerprint) {
      return issue;
    }
    if (titleContainsFinding(issue.title, finding.title)) {
      return issue;
    }
    if (issue.title.toLowerCase() === issueTitle.toLowerCase()) {
      return issue;
    }
  }
  return null;
}

export function defaultGhExec(args) {
  return execFileSync("gh", args, { encoding: "utf8" });
}

export function listOpenBugIssues(execGh, repo) {
  const output = execGh([
    "issue",
    "list",
    "--repo",
    repo,
    "--label",
    "bug",
    "--state",
    "open",
    "--limit",
    "100",
    "--json",
    "number,title,body,url",
  ]);
  return JSON.parse(output);
}

export function parseIssueCreateOutput(output) {
  const url = output.trim();
  const match = /\/issues\/(\d+)\s*$/.exec(url);
  return {
    issueUrl: url,
    issueNumber: match ? Number(match[1]) : null,
  };
}

export function createGhIssue(execGh, repo, title, body, { dryRun = false } = {}) {
  if (dryRun) {
    return { issueUrl: null, issueNumber: null };
  }
  const output = execGh([
    "issue",
    "create",
    "--repo",
    repo,
    "--label",
    "bug",
    "--title",
    title,
    "--body",
    body,
  ]);
  return parseIssueCreateOutput(output);
}

export function resolveVersion(run, env) {
  return run.headSha || run.repoSha || env.GITHUB_SHA || "unknown";
}

export function resolveRunUrl(env) {
  return env.AJAX_EXPLORATORY_RUN_URL || null;
}

export function resolveArtifactName(env, run) {
  const runId = env.GITHUB_RUN_ID || run.runId || "unknown";
  return `exploratory-results-${runId}`;
}

export function shouldFileIssues(env, force) {
  return force || env.GITHUB_ACTIONS === "true";
}

export function fileIssues(options = {}) {
  const {
    execGh = defaultGhExec,
    env = process.env,
    argv = process.argv.slice(2),
    findingsPath = join(resultsDir, "findings.json"),
    runPath = join(resultsDir, "run.json"),
    issuesPath = join(resultsDir, "issues.json"),
  } = options;

  const args = parseArgs(argv);
  const findingsDoc = readJson(findingsPath, emptyFindings());
  const run = readJson(runPath, {});
  const eligible = (findingsDoc.findings ?? []).filter(isEligibleFinding);

  const repo = env.GH_REPO || DEFAULT_REPO;
  const filingEnabled = shouldFileIssues(env, args.force);
  const version = resolveVersion(run, env);
  const runUrl = resolveRunUrl(env);
  const artifactName = resolveArtifactName(env, run);

  const issues = [];
  const summary = { created: 0, duplicate: 0, failed: 0, skipped: 0 };

  if (!filingEnabled) {
    console.log("GitHub issue filing skipped (not running in GitHub Actions; use --force to file).");
    for (const finding of eligible) {
      const fingerprint = fingerprintForFinding(finding);
      issues.push({
        fingerprint,
        title: buildIssueTitle(finding),
        action: "skipped",
        issueUrl: null,
        issueNumber: null,
      });
      summary.skipped += 1;
    }
    writeJson(issuesPath, issues);
    run.issues = summary;
    writeJson(runPath, run);
    return 0;
  }

  let openIssues = [];
  try {
    openIssues = listOpenBugIssues(execGh, repo);
  } catch (error) {
    console.error(`failed to list open issues: ${error.message}`);
    return 1;
  }

  for (const finding of eligible) {
    const fingerprint = fingerprintForFinding(finding);
    const title = buildIssueTitle(finding);
    const body = buildIssueBody(finding, {
      fingerprint,
      version,
      runUrl,
      artifactName,
    });

    const duplicate = findDuplicate(openIssues, finding, fingerprint);
    if (duplicate) {
      issues.push({
        fingerprint,
        title,
        action: "duplicate",
        issueUrl: duplicate.url,
        issueNumber: duplicate.number,
      });
      summary.duplicate += 1;
      continue;
    }

    if (args.dryRun) {
      issues.push({
        fingerprint,
        title,
        action: "created",
        issueUrl: null,
        issueNumber: null,
      });
      summary.created += 1;
      continue;
    }

    try {
      const created = createGhIssue(execGh, repo, title, body);
      issues.push({
        fingerprint,
        title,
        action: "created",
        issueUrl: created.issueUrl,
        issueNumber: created.issueNumber,
      });
      summary.created += 1;
      openIssues.push({
        number: created.issueNumber,
        title,
        body,
        url: created.issueUrl,
      });
    } catch (error) {
      issues.push({
        fingerprint,
        title,
        action: "failed",
        issueUrl: null,
        issueNumber: null,
        error: error.message,
      });
      summary.failed += 1;
    }
  }

  writeJson(issuesPath, issues);
  run.issues = summary;
  writeJson(runPath, run);

  if (summary.failed > 0) {
    return 1;
  }
  return 0;
}

export function main() {
  const exitCode = fileIssues();
  process.exit(exitCode);
}

const isMain =
  process.argv[1] &&
  import.meta.url === pathToFileURL(process.argv[1]).href;

if (isMain && !process.env.NODE_TEST_CONTEXT) {
  main();
}
