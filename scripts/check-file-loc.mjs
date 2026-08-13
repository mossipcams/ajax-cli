// Enforces per-file total line counts on files changed in a PR.
// Warn at WARN_AT lines; fail at FAIL_AT lines.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

export const PR_LOC_WARN_AT = 800;
export const PR_LOC_FAIL_AT = 1500;
export const CHANGED_FILE_LOC_WARN_AT = 400;
export const CHANGED_FILE_LOC_FAIL_AT = 600;
export const WARN_AT = 800;
export const FAIL_AT = 1000;

export const SOURCE_EXTENSIONS = new Set([
  ".rs",
  ".ts",
  ".tsx",
  ".js",
  ".mjs",
  ".svelte",
]);

const EXCLUDED_PATH_PARTS = [
  "/dist/",
  "/node_modules/",
  "/target/",
];

/** wc-style line count for a UTF-8 string. */
export function countLines(text) {
  if (!text) {
    return 0;
  }

  let lines = 0;
  for (let i = 0; i < text.length; i++) {
    if (text[i] === "\n") {
      lines++;
    }
  }

  if (!text.endsWith("\n")) {
    lines++;
  }

  return lines;
}

export function isScannedSourcePath(path) {
  const normalized = path.replaceAll("\\", "/");

  if (EXCLUDED_PATH_PARTS.some((part) => normalized.includes(part))) {
    return false;
  }

  const dot = normalized.lastIndexOf(".");
  if (dot === -1) {
    return false;
  }

  return SOURCE_EXTENSIONS.has(normalized.slice(dot));
}

function evaluateMetric(path, lines, { warnAt, failAt, label }) {
  if (lines >= failAt) {
    return {
      level: "error",
      path,
      lines,
      message:
        `${path} is ${lines} ${label} (limit ${failAt}). ` +
        "Split the file before merging.",
    };
  }

  if (lines >= warnAt) {
    return {
      level: "warning",
      path,
      lines,
      message:
        `${path} is ${lines} ${label} (warning at ${warnAt}, ` +
        `hard limit ${failAt}). Consider splitting it.`,
    };
  }

  return null;
}

/** Classifies one changed file by its total line count at HEAD. */
export function evaluateFileLoc(
  path,
  lines,
  { warnAt = WARN_AT, failAt = FAIL_AT } = {},
) {
  const finding = evaluateMetric(path, lines, {
    warnAt,
    failAt,
    label: "lines",
  });
  return finding ? [finding] : [];
}

export function evaluateChangedLoc(
  path,
  lines,
  { warnAt = CHANGED_FILE_LOC_WARN_AT, failAt = CHANGED_FILE_LOC_FAIL_AT } = {},
) {
  return evaluateMetric(path, lines, {
    warnAt,
    failAt,
    label: "changed lines",
  });
}

export function evaluatePrLoc(
  lines,
  { warnAt = PR_LOC_WARN_AT, failAt = PR_LOC_FAIL_AT } = {},
) {
  return evaluateMetric("PR", lines, {
    warnAt,
    failAt,
    label: "changed lines",
  });
}

export function evaluateChangedFiles(files, lineCountAtHead, options = {}) {
  const findings = [];

  for (const path of [...files].sort()) {
    if (!isScannedSourcePath(path)) {
      continue;
    }

    findings.push(...evaluateFileLoc(path, lineCountAtHead(path), options));
  }

  return findings;
}

export function formatAnnotation(finding) {
  const title =
    finding.level === "error" ? "File too large" : "Large file";

  return `::${finding.level} file=${finding.path},line=1::${finding.message}`;
}

export function summarizeFindings(findings) {
  const warnings = findings.filter((f) => f.level === "warning");
  const errors = findings.filter((f) => f.level === "error");
  return { warnings, errors };
}

export function parseNameOnlyList(text) {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
}

export function parseNumstat(text) {
  return text
    .split("\n")
    .map((line) => line.split("\t"))
    .filter(([additions, deletions]) => /^\d+$/.test(additions) && /^\d+$/.test(deletions))
    .map(([additions, deletions, ...pathParts]) => ({
      path: pathParts.join("\t"),
      additions: Number(additions),
      deletions: Number(deletions),
    }));
}

export function evaluateStagedFiles(entries, lineCountAtIndex) {
  const findings = [];
  let changedLines = 0;

  for (const { path, additions, deletions } of entries) {
    if (!isScannedSourcePath(path)) {
      continue;
    }

    const fileChangedLines = additions + deletions;
    changedLines += fileChangedLines;

    const changedFinding = evaluateChangedLoc(path, fileChangedLines);
    if (changedFinding) {
      findings.push(changedFinding);
    }

    findings.push(...evaluateFileLoc(path, lineCountAtIndex(path)));
  }

  const prFinding = evaluatePrLoc(changedLines);
  if (prFinding) {
    findings.push(prFinding);
  }

  return findings;
}

export async function listChangedFiles(base, head, runGit) {
  const output = await runGit([
    "diff",
    "--name-only",
    "--diff-filter=ACMR",
    base,
    head,
  ]);
  return parseNameOnlyList(output);
}

export async function inspectChangedFileLoc(
  base,
  head,
  { root = ".", runGit, readHead = (path) => readFileSync(join(root, path), "utf8") } = {},
) {
  const files = await listChangedFiles(base, head, runGit);
  const lineCountAtHead = (path) => countLines(readHead(path));
  const findings = evaluateChangedFiles(files, lineCountAtHead);

  return { files, ...summarizeFindings(findings) };
}

export async function inspectStagedFileLoc({
  runGit,
  readIndex = (path) => runGit(["show", `:${path}`]),
} = {}) {
  const entries = parseNumstat(
    await runGit([
      "diff",
      "--cached",
      "--no-renames",
      "--numstat",
      "--diff-filter=ACMR",
    ]),
  );
  const lineCounts = new Map(
    await Promise.all(
      entries
        .filter(({ path }) => isScannedSourcePath(path))
        .map(async ({ path }) => [path, countLines(await readIndex(path))]),
    ),
  );
  const findings = evaluateStagedFiles(
    entries,
    (path) => lineCounts.get(path) ?? 0,
  );

  return {
    files: entries.filter(({ path }) => isScannedSourcePath(path)).map(({ path }) => path),
    ...summarizeFindings(findings),
  };
}

function resolveRefs() {
  const event = process.env.GITHUB_EVENT_NAME;

  if (event === "pull_request" || event === "merge_group") {
    const base = process.env.GITHUB_BASE_SHA;
    const head = process.env.GITHUB_HEAD_SHA ?? process.env.GITHUB_SHA;
    if (base && head) {
      return { base, head };
    }
  }

  const base = process.env.FILE_LOC_BASE;
  const head = process.env.FILE_LOC_HEAD;
  if (base && head) {
    return { base, head };
  }

  return null;
}

async function runGit(args) {
  const { spawnSync } = await import("node:child_process");
  const result = spawnSync("git", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });

  if (result.status !== 0) {
    const detail = (result.stderr || result.stdout || "").trim();
    throw new Error(
      `git ${args.join(" ")} failed${detail ? `: ${detail}` : ""}`,
    );
  }

  return result.stdout;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const refs = resolveRefs();

  const root = join(fileURLToPath(import.meta.url), "..", "..");
  const result = refs
    ? await inspectChangedFileLoc(refs.base, refs.head, { root, runGit })
    : process.argv.includes("--staged")
      ? await inspectStagedFileLoc({
          runGit,
          readIndex: (path) => runGit(["show", `:${path}`]),
        })
      : null;

  if (!result) {
    console.log("File LOC check skipped: no base/head refs for this event.");
    process.exit(0);
  }

  const { files, warnings, errors } = result;

  const scanned = files.filter(isScannedSourcePath);
  console.log(
    refs
      ? `Checked ${scanned.length} changed source file(s) between ${refs.base.slice(0, 7)} and ${refs.head.slice(0, 7)}.`
      : `Checked ${scanned.length} staged source file(s).`,
  );

  for (const finding of [...warnings, ...errors]) {
    console.log(finding.message);
    console.log(formatAnnotation(finding));
  }

  if (warnings.length === 0 && errors.length === 0) {
    console.log(
      `No LOC threshold was reached (PR ${PR_LOC_WARN_AT}/${PR_LOC_FAIL_AT}, ` +
        `changed file ${CHANGED_FILE_LOC_WARN_AT}/${CHANGED_FILE_LOC_FAIL_AT}, ` +
        `total file ${WARN_AT}/${FAIL_AT}).`,
    );
  } else {
    console.log(`${warnings.length} warning(s), ${errors.length} error(s).`);
  }

  process.exit(errors.length > 0 ? 1 : 0);
}
