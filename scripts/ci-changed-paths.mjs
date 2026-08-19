// Classifies changed paths into CI lanes for path-filtered jobs.
// Emits rust/web/lockfile/full booleans to GITHUB_OUTPUT when present.

import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { listChangedFiles } from "./check-file-loc.mjs";

const FULL_TRIGGER_PATHS = new Set([
  "scripts/verify-ci-workflows.mjs",
  "scripts/ci-changed-paths.mjs",
  "scripts/ci-changed-paths.test.mjs",
]);

/** Normalize repo-relative paths to forward slashes. */
export function normalizePath(path) {
  return path.replaceAll("\\", "/");
}

export function isFullTriggerPath(path) {
  const normalized = normalizePath(path);

  if (normalized.startsWith(".github/workflows/")) {
    return true;
  }

  return FULL_TRIGGER_PATHS.has(normalized);
}

export function isRustPath(path) {
  const normalized = normalizePath(path);

  if (
    normalized.endsWith(".rs") &&
    normalized.startsWith("crates/") &&
    !normalized.startsWith("crates/ajax-web/web/")
  ) {
    return true;
  }

  if (normalized.endsWith("/Cargo.toml") || normalized === "Cargo.toml") {
    return true;
  }

  if (normalized === "Cargo.lock") {
    return true;
  }

  if (normalized.startsWith("rust-toolchain")) {
    return true;
  }

  if (normalized === "rustfmt.toml" || normalized === "clippy.toml") {
    return true;
  }

  if (normalized === ".config/nextest.toml") {
    return true;
  }

  return false;
}

export function isWebPath(path) {
  const normalized = normalizePath(path);

  if (normalized.startsWith("crates/ajax-web/web/")) {
    return true;
  }

  if (normalized === "package.json" || normalized === "package-lock.json") {
    return true;
  }

  if (/playwright[^/]*\.(mts|ts|mjs)$/.test(normalized)) {
    return true;
  }

  return false;
}

export function isLockfilePath(path) {
  return normalizePath(path) === "Cargo.lock";
}

export function classifyPath(path) {
  return {
    rust: isRustPath(path),
    web: isWebPath(path),
    lockfile: isLockfilePath(path),
    full: isFullTriggerPath(path),
  };
}

export function classifyChangedPaths(files, { forceFull = false } = {}) {
  if (forceFull) {
    return { rust: true, web: true, lockfile: true, full: true };
  }

  const result = { rust: false, web: false, lockfile: false, full: false };

  for (const file of files) {
    const flags = classifyPath(file);
    result.rust ||= flags.rust;
    result.web ||= flags.web;
    result.lockfile ||= flags.lockfile;
    result.full ||= flags.full;
  }

  return result;
}

export function resolveRefs(env = process.env) {
  if (env.GITHUB_EVENT_NAME === "workflow_dispatch") {
    return null;
  }

  if (env.GITHUB_EVENT_NAME === "pull_request" || env.GITHUB_EVENT_NAME === "merge_group") {
    const base = env.GITHUB_BASE_SHA;
    const head = env.GITHUB_HEAD_SHA ?? env.GITHUB_SHA;
    if (base && head) {
      return { base, head };
    }
  }

  const base = env.CI_CHANGED_PATHS_BASE;
  const head = env.CI_CHANGED_PATHS_HEAD;
  if (base && head) {
    return { base, head };
  }

  return null;
}

export function formatGithubOutput(flags) {
  return Object.entries(flags)
    .map(([key, value]) => `${key}=${value ? "true" : "false"}`)
    .join("\n");
}

export async function detectChangedPathLanes({
  env = process.env,
  runGit,
  listFiles = listChangedFiles,
} = {}) {
  const refs = resolveRefs(env);
  const forceFull = refs === null;

  const files = forceFull ? [] : await listFiles(refs.base, refs.head, runGit);
  const flags = classifyChangedPaths(files, { forceFull });

  return { files, flags, forceFull, refs };
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
  const { files, flags, forceFull, refs } = await detectChangedPathLanes({ runGit });
  const output = formatGithubOutput(flags);

  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `${output}\n`);
  }

  if (forceFull) {
    console.log("Path lanes: full suite (missing SHAs or workflow_dispatch).");
  } else {
    console.log(
      `Path lanes for ${files.length} changed file(s) between ` +
        `${refs.base.slice(0, 7)} and ${refs.head.slice(0, 7)}:`,
    );
  }

  for (const [lane, enabled] of Object.entries(flags)) {
    console.log(`  ${lane}=${enabled}`);
  }
}
