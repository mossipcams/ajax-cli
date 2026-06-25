import fs from "node:fs";

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

const packageJson = JSON.parse(fs.readFileSync("package.json", "utf8"));
const verify = packageJson.scripts?.verify ?? "";
assert(
  verify.includes("npm run web:test -- --run"),
  "npm run verify must execute the one-shot frontend test suite",
);
assert(
  packageJson.scripts?.["verify:web-contract-gate"] ===
    "node scripts/verify-web-contract-gate.mjs",
  "package.json must expose npm run verify:web-contract-gate",
);

const workflow = fs.readFileSync(".github/workflows/ci.yml", "utf8");
assert(
  workflow.includes("name: Web") &&
    workflow.includes("run: npm run web:check") &&
    workflow.includes("run: npm run web:test -- --run"),
  "CI must include a Web job that runs type checks and one-shot frontend tests",
);
assert(
  /needs:[\s\S]*?\n\s+- web(?:\n|$)/.test(workflow),
  "the aggregate CI job must require the Web job",
);

console.log("web contract gate verified");
