import fs from "node:fs";
import path from "node:path";

const root = process.cwd();

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));
}

function readText(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const packageJson = readJson("package.json");
assert(
  packageJson.devDependencies?.["@nizos/probity"],
  "package.json must declare @nizos/probity as a dev dependency",
);
assert(
  packageJson.scripts?.["verify:probity"] === "node scripts/verify-probity.mjs",
  "package.json must expose npm run verify:probity",
);

const packageLock = readJson("package-lock.json");
assert(
  packageLock.packages?.[""]?.devDependencies?.["@nizos/probity"],
  "package-lock.json root package must lock @nizos/probity",
);
assert(
  packageLock.packages?.["node_modules/@nizos/probity"],
  "package-lock.json must include node_modules/@nizos/probity",
);

const config = readText("probity.config.ts");
assert(
  config.includes("defineConfig"),
  "probity.config.ts must use defineConfig",
);
assert(
  config.includes("enforceTdd"),
  "probity.config.ts must enable enforceTdd",
);
assert(
  config.includes("forbidCommandPattern"),
  "probity.config.ts must include command blockers",
);
assert(
  config.includes("crates/**/*.rs") &&
    config.includes("crates/**/tests/**/*.rs") &&
    config.includes("scripts/**/*.sh"),
  "probity.config.ts must scope TDD to Rust source, Rust tests, and shell scripts",
);
assert(
  config.includes("Write a failing behavior test before changing code"),
  "probity.config.ts must include Ajax TDD instructions",
);
assert(
  config.includes("Task N done. Continue?"),
  "probity.config.ts must preserve the task-by-task workflow prompt",
);
assert(
  config.includes("git\\s+reset\\s+--hard") &&
    config.includes("git\\s+checkout\\s+--") &&
    config.includes("rm\\s+-rf"),
  "probity.config.ts must block destructive git and remove commands",
);

const hook = readJson(".github/hooks/probity.json");
const preToolUse = hook.hooks?.preToolUse ?? [];
assert(hook.version === 1, ".github/hooks/probity.json must use version 1");
assert(
  preToolUse.some(
    (entry) =>
      entry.type === "command" &&
      entry.bash === "npx @nizos/probity --agent github-copilot-chat" &&
      entry.powershell === "npx @nizos/probity --agent github-copilot-chat",
  ),
  ".github/hooks/probity.json must wire GitHub Copilot Chat to probity",
);

console.log("probity integration verified");
