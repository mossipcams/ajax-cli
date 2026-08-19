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
const RELEASE_SKIP_JOBS = [
  "file-loc",
  "invariants",
  "rust-lint",
  "rust-test",
  "rust-docs",
  "audit",
  "web-unit",
  "web-e2e",
];

const PATH_FILTERED_JOBS = [
  "rust-lint",
  "rust-test",
  "rust-docs",
  "audit",
  "web-unit",
  "web-e2e",
];

const RETIRED_JOB_IDS = [
  "format",
  "check",
  "clippy",
  "test",
  "docs",
  "web",
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
  const exploratory = load("exploratory-testing.yml");

  if (ci) {
    verifyCi(ci, fail, root);
  }

  if (releasePlease) {
    verifyReleasePlease(releasePlease, fail);
  }

  if (exploratory) {
    verifyExploratory(exploratory, fail);
  } else {
    fail("exploratory-testing.yml must exist.");
  }

  return problems;
}

function verifyExploratory(workflow, fail) {
  const on = workflow.on ?? workflow.true;
  const triggers = Object.keys(on ?? {});

  if (!triggers.includes("schedule")) {
    fail("exploratory-testing.yml must run on a schedule.");
  }
  if (!triggers.includes("workflow_dispatch")) {
    fail("exploratory-testing.yml must allow workflow_dispatch.");
  }
  if (triggers.includes("push") || triggers.includes("pull_request")) {
    fail(
      "exploratory-testing.yml must not run on push/pull_request; it is CI-only " +
        "exploratory coverage, not a PR gate.",
    );
  }

  const dispatchInputs = on?.workflow_dispatch?.inputs ?? {};
  if (dispatchInputs.budget_minutes?.default !== "12") {
    fail("exploratory workflow_dispatch budget_minutes default must be \"12\".");
  }

  const budgetEnv = workflow.env?.AJAX_EXPLORATORY_BUDGET_MINUTES ?? "";
  if (!String(budgetEnv).includes("github.event.inputs.budget_minutes")) {
    fail("exploratory-testing.yml must preserve budget_minutes workflow_dispatch override.");
  }
  if (!String(budgetEnv).includes("'12'")) {
    fail("exploratory-testing.yml AJAX_EXPLORATORY_BUDGET_MINUTES fallback must be '12'.");
  }

  const jobs = workflow.jobs ?? {};
  const explore = jobs.explore;
  if (!explore) {
    fail("exploratory-testing.yml must define the explore job.");
    return;
  }

  if (explore["runs-on"] !== "ubuntu-latest") {
    fail("exploratory explore job must use runs-on: ubuntu-latest only.");
  }

  const timeout = explore["timeout-minutes"];
  if (typeof timeout !== "number" || timeout > 45) {
    fail("exploratory explore job must set timeout-minutes ≤ 45.");
  }

  const text = JSON.stringify(explore);
  for (const needle of [
    "CURSOR_API_KEY",
    "actions/upload-artifact@v4",
    "always()",
    "scripts/exploratory/run-agent.sh",
    "scripts/exploratory/prepare-oracles.mjs",
    "scripts/exploratory/file-issues.mjs",
    "npx playwright install --with-deps webkit",
    "apt-get install -y tmux",
  ]) {
    if (!text.includes(needle)) {
      fail(`exploratory explore job must include ${needle}.`);
    }
  }

  if (
    /playwright install[^\n]*chromium/i.test(text) ||
    /playwright install[^\n]*firefox/i.test(text)
  ) {
    fail("exploratory explore job must not install chromium or firefox.");
  }

  if (text.includes("self-hosted") || text.includes("macos-") || text.includes("windows-")) {
    fail("exploratory explore job must stay on GitHub-hosted ubuntu-latest.");
  }

  const permissions = workflow.permissions ?? {};
  if (permissions.contents && permissions.contents !== "read") {
    fail("exploratory-testing.yml contents permission must stay read.");
  }
  if (permissions.issues !== "write") {
    fail("exploratory-testing.yml must grant issues: write for defect filing.");
  }
}

function verifyCi(ci, fail, root) {
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

  for (const job of RETIRED_JOB_IDS) {
    if (jobs[job]) {
      fail(`ci.yml must not define the retired ${job} job.`);
    }
  }

  const changes = jobs.changes ?? {};
  const changeOutputs = changes.outputs ?? {};
  for (const output of ["rust", "web", "lockfile", "full"]) {
    if (!String(changeOutputs[output] ?? "").includes("steps.paths.outputs")) {
      fail(`ci.yml changes job must emit the ${output} output.`);
    }
  }

  const changesSteps = JSON.stringify(changes.steps ?? []);
  if (!changesSteps.includes("scripts/ci-changed-paths.mjs")) {
    fail("ci.yml changes job must run scripts/ci-changed-paths.mjs.");
  }

  const invariants = jobs.invariants ?? {};
  const invariantsSteps = JSON.stringify(invariants.steps ?? []);
  if (!invariantsSteps.includes("npm run ci:verify")) {
    fail("ci.yml invariants job must run npm run ci:verify.");
  }

  const webUnit = jobs["web-unit"] ?? {};
  const webUnitSteps = JSON.stringify(webUnit.steps ?? []);
  if (!webUnitSteps.includes("git diff --exit-code crates/ajax-web/web/dist")) {
    fail(
      "ci.yml web-unit job must fail when crates/ajax-web/web/dist is stale after web:build.",
    );
  }
  for (const command of ["web:check", "web:lint", "web:sg", "web:test"]) {
    if (!webUnitSteps.includes(command)) {
      fail(`ci.yml web-unit job must run npm run ${command}.`);
    }
  }

  const webE2e = jobs["web-e2e"] ?? {};
  const webE2eSteps = JSON.stringify(webE2e.steps ?? []);
  const webE2eText = JSON.stringify(webE2e);

  if (webE2e["timeout-minutes"] !== 20) {
    fail("ci.yml web-e2e job must set timeout-minutes: 20.");
  }

  const container = webE2e.container ?? {};
  if (container.image !== "mcr.microsoft.com/playwright:v1.61.1-noble") {
    fail(
      "ci.yml web-e2e job must run in mcr.microsoft.com/playwright:v1.61.1-noble.",
    );
  }

  if (container.options !== "--ipc=host") {
    fail("ci.yml web-e2e job container must set options: --ipc=host.");
  }

  if (!webE2eText.includes("safe.directory")) {
    fail("ci.yml web-e2e job must configure git safe.directory for container checkout.");
  }

  if (!webE2eSteps.includes("/root")) {
    fail("ci.yml web-e2e job end-to-end step must set HOME=/root.");
  }

  for (const step of webE2e.steps ?? []) {
    const name = String(step.name ?? "").toLowerCase();
    if (name.includes("smoke")) {
      fail("ci.yml web-e2e job step name must not call the full suite smoke.");
    }
  }

  if (
    webE2eSteps.includes("playwright install-deps") ||
    webE2eSteps.includes("playwright install webkit") ||
    webE2eSteps.includes("playwright-cache")
  ) {
    fail(
      "ci.yml web-e2e job must not install or cache Playwright when using the container image.",
    );
  }

  const rustLintSteps = JSON.stringify(jobs["rust-lint"]?.steps ?? []);
  if (!rustLintSteps.includes("cargo fmt --check")) {
    fail("ci.yml rust-lint job must run cargo fmt --check.");
  }
  if (!rustLintSteps.includes("cargo clippy --locked")) {
    fail("ci.yml rust-lint job must run cargo clippy --locked.");
  }

  const rustTestSteps = JSON.stringify(jobs["rust-test"]?.steps ?? []);
  if (!rustTestSteps.includes("cargo nextest run --all-features")) {
    fail("ci.yml rust-test job must run cargo nextest run --all-features.");
  }
  if (rustTestSteps.includes("--test-threads=1")) {
    fail("ci.yml rust-test job must not force --test-threads=1.");
  }

  for (const job of RELEASE_SKIP_JOBS) {
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

  for (const job of PATH_FILTERED_JOBS) {
    const condition = String(jobs[job]?.if ?? "");
    if (!condition.includes("needs.changes.outputs")) {
      fail(`ci.yml job ${job} must key off needs.changes.outputs.*.`);
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

  for (const job of [
    ...RELEASE_SKIP_JOBS,
    "changes",
    "release-candidate",
    "pr-title",
  ]) {
    if (!(aggregate.needs ?? []).includes(job)) {
      fail(`Aggregate ci job must need ${job}.`);
    }
  }

  const verify = JSON.stringify(aggregate.steps ?? []);

  if (!verify.includes("needs.release-candidate.result")) {
    fail("Aggregate ci job must require release-candidate success on release PRs.");
  }

  if (!verify.includes("needs.changes.result")) {
    fail("Aggregate ci job must fail when changes detection fails.");
  }

  for (const output of ["needs.changes.outputs.full", "needs.changes.outputs.rust"]) {
    if (!verify.includes(output)) {
      fail(`Aggregate ci job must branch on ${output}.`);
    }
  }

  for (const job of ["rust-lint", "rust-test", "rust-docs", "web-unit", "web-e2e", "audit"]) {
    if (!verify.includes(`needs.${job}.result`)) {
      fail(`Aggregate ci job must enforce ${job} when its lane is needed.`);
    }
  }

  const playwrightConfig = readFileSync(
    join(root, "crates", "ajax-web", "web", "playwright.config.mts"),
    "utf8",
  );
  if (!/workers:\s*process\.env\.CI\s*\?\s*4/.test(playwrightConfig)) {
    fail("playwright.config.mts must run 4 workers in CI.");
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
