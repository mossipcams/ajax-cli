import { readFileSync } from "node:fs";

const workflow = readFileSync(".github/workflows/ci.yml", "utf8");

function fail(message) {
  console.error(message);
  process.exitCode = 1;
}

function has(pattern, message) {
  if (!pattern.test(workflow)) {
    fail(message);
  }
}

function jobBlock(job) {
  const lines = workflow.split("\n");
  const start = lines.findIndex((line) => line === `  ${job}:`);
  if (start === -1) {
    return null;
  }

  const end = lines.findIndex(
    (line, index) => index > start && /^  [a-zA-Z0-9_-]+:$/.test(line),
  );
  return lines.slice(start, end === -1 ? undefined : end).join("\n");
}

has(
  /^\s+release-pr-bypass:\n/m,
  "CI workflow must define a release-pr-bypass classifier job.",
);
has(
  /outputs:\n\s+skip-ci:\s+\$\{\{\s*steps\.classify\.outputs\.skip-ci\s*\}\}/m,
  "Classifier job must expose steps.classify.outputs.skip-ci.",
);
has(
  /github\.event_name\s*==\s*'pull_request'/,
  "Classifier must limit bypass detection to pull_request events.",
);
has(
  /github\.head_ref,\s*'release-please--branches--main'/,
  "Classifier must identify Release Please pull request branches.",
);

for (const job of ["format", "web", "check", "clippy", "test", "docs", "audit"]) {
  const block = jobBlock(job);

  if (!block) {
    fail(`CI workflow must define ${job} job.`);
    continue;
  }

  if (!/needs:\s+release-pr-bypass/.test(block)) {
    fail(`${job} job must depend on release-pr-bypass.`);
  }

  if (!/if:\s+\$\{\{\s*needs\.release-pr-bypass\.outputs\.skip-ci\s*!=\s*'true'\s*\}\}/.test(block)) {
    fail(`${job} job must skip only when release-pr-bypass says skip-ci=true.`);
  }
}

const aggregateJob = jobBlock("ci");

if (!aggregateJob) {
  fail("CI workflow must define aggregate ci job.");
} else {
  if (!/- release-pr-bypass/.test(aggregateJob)) {
    fail("Aggregate ci job must need release-pr-bypass.");
  }

  if (!/needs\.release-pr-bypass\.outputs\.skip-ci\s*==\s*'true'/.test(aggregateJob)) {
    fail("Aggregate ci job must allow skipped dependencies only for Release Please PR bypasses.");
  }

  if (!/CI intentionally bypassed for Release Please PR\./.test(aggregateJob)) {
    fail("Aggregate ci job must explain intentional Release Please PR bypasses.");
  }
}

if (process.exitCode) {
  process.exit(process.exitCode);
}

console.log("CI Release Please PR bypass workflow assertions passed.");
