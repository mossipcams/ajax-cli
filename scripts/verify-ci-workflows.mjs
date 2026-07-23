// Guards the trigger matrix the CI and Release Please workflows encode. These
// are structural properties GitHub will not tell you about until a run misfires
// in production — a Release Please PR quietly running the full suite again, or a
// normal PR having its expensive jobs skipped by a stale `if:`.
//
// Replaces the old scripts/verify-ci-release-pr-bypass.mjs, which asserted the
// blanket skip-CI bypass this pipeline no longer has. Parses the YAML rather
// than regexing it, so reindentation cannot make an assertion silently vacuous.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";
import { parse } from "yaml";

const RELEASE_BRANCH = "release-please--branches--main";

// Jobs that must never run on the generated release PR: its commits were all
// tested on the PRs that produced them.
const HEAVY_JOBS = [
  "format",
  "web",
  "check",
  "clippy",
  "test",
  "docs",
  "audit",
];

export function verifyWorkflows(root) {
  const problems = [];
  const fail = (message) => problems.push(message);

  const load = (name) => {
    const path = join(root, ".github", "workflows", name);
    try {
      return parse(readFileSync(path, "utf8"));
    } catch (error) {
      fail(`${name}: invalid YAML — ${error.message}`);
      return null;
    }
  };

  const ci = load("ci.yml");
  const releasePlease = load("release-please.yml");

  if (ci) {
    verifyCi(ci, fail);
  }

  if (releasePlease) {
    verifyReleasePlease(releasePlease, fail);
  }

  return problems;
}

function verifyCi(ci, fail) {
  const on = ci.on ?? ci.true; // `on:` is YAML-truthy unless quoted.
  const triggers = Object.keys(on ?? {});

  if (!triggers.includes("pull_request")) {
    fail("ci.yml must run on pull_request.");
  }

  if (triggers.includes("push")) {
    fail(
      "ci.yml must not run on push. Integration safety comes from the strict " +
        "required-status-check rule on main; a push run re-tests a tree that " +
        "already passed.",
    );
  }

  if (!triggers.includes("merge_group")) {
    fail("ci.yml must keep merge_group support for a future merge queue.");
  }

  if (!ci.concurrency?.group?.includes("github.event.pull_request.number")) {
    fail("ci.yml concurrency must be keyed by PR number.");
  }

  if (!String(ci.concurrency?.["cancel-in-progress"] ?? "").includes("pull_request")) {
    fail(
      "ci.yml must cancel superseded pull_request runs only — merge_group runs " +
        "test exact merge candidates and must not be cancelled.",
    );
  }

  const jobs = ci.jobs ?? {};

  for (const job of HEAVY_JOBS) {
    if (!jobs[job]) {
      fail(`ci.yml must define the ${job} job.`);
      continue;
    }

    const condition = String(jobs[job].if ?? "");

    if (!condition.includes(RELEASE_BRANCH) || !condition.includes("!startsWith")) {
      fail(
        `ci.yml job ${job} must be skipped on ${RELEASE_BRANCH}* via ` +
          "`if: !startsWith(github.head_ref, ...)`.",
      );
    }
  }

  const candidate = jobs["release-candidate"];

  if (!candidate) {
    fail("ci.yml must define the release-candidate job.");
  } else {
    const condition = String(candidate.if ?? "");

    if (!condition.includes(`startsWith(github.head_ref, '${RELEASE_BRANCH}')`)) {
      fail("release-candidate must run only on the generated release branch.");
    }

    const steps = candidate.steps ?? [];
    const text = JSON.stringify(steps);

    const required = [
      ["github.event.pull_request.head.sha", "check out the exact PR head SHA"],
      ["fetch --no-tags origin main", "fetch current origin/main"],
      ["merge-tree --write-tree origin/main HEAD", "detect merge conflicts explicitly"],
      ["scripts/check-release-version.mjs", "verify release version consistency"],
      ["cargo check --locked -p ajax-cli", "prove Cargo.lock records the bumped version"],
    ];

    for (const [needle, description] of required) {
      if (!text.includes(needle)) {
        fail(`release-candidate must ${description} (missing: ${needle}).`);
      }
    }

    for (const forbidden of [
      "cargo fmt",
      "cargo clippy",
      "cargo nextest",
      "cargo doc",
      "cargo audit",
      "playwright",
    ]) {
      if (text.toLowerCase().includes(forbidden)) {
        fail(
          `release-candidate must stay lightweight; it runs ${forbidden}, which ` +
            "already ran on the PRs being released.",
        );
      }
    }
  }

  const aggregate = jobs.ci;

  if (!aggregate) {
    fail("ci.yml must define the aggregate ci job (the required check).");
    return;
  }

  if (aggregate.name !== "CI") {
    fail("The aggregate job must stay named CI — the ruleset requires that context.");
  }

  for (const job of [...HEAVY_JOBS, "release-candidate", "pr-title"]) {
    if (!(aggregate.needs ?? []).includes(job)) {
      fail(`Aggregate ci job must need ${job}.`);
    }
  }

  const verify = JSON.stringify(aggregate.steps ?? []);

  if (!verify.includes("needs.release-candidate.result")) {
    fail("Aggregate ci job must require release-candidate success on release PRs.");
  }

  for (const job of HEAVY_JOBS) {
    if (!verify.includes(`needs.${job}.result`)) {
      fail(`Aggregate ci job must require ${job} success on normal PRs.`);
    }
  }
}

function verifyReleasePlease(workflow, fail) {
  if (!workflow.concurrency?.group) {
    fail(
      "release-please.yml must set concurrency so two main merges cannot race " +
        "on the same rolling release PR.",
    );
  }

  if (workflow.concurrency?.["cancel-in-progress"] !== false) {
    fail(
      "release-please.yml must not cancel in progress: a half-finished run can " +
        "leave the release branch inconsistent.",
    );
  }

  const steps = JSON.stringify(workflow.jobs?.["release-please"]?.steps ?? []);

  if (steps.includes("cargo update")) {
    fail(
      "release-please.yml must not push a follow-up Cargo.lock commit; the " +
        "lockfile is bumped in-place by the extra-files entry in " +
        "release-please-config.json.",
    );
  }

  if (!steps.includes("RELEASE_PLEASE_TOKEN")) {
    fail(
      "release-please.yml must use RELEASE_PLEASE_TOKEN: a PR opened with the " +
        "default GITHUB_TOKEN never reports the required CI check.",
    );
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const problems = verifyWorkflows(
    join(fileURLToPath(import.meta.url), "..", ".."),
  );

  if (problems.length > 0) {
    for (const problem of problems) {
      console.error(problem);
    }
    process.exit(1);
  }

  console.log("CI and Release Please workflow invariants hold.");
}
