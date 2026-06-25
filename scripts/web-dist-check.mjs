#!/usr/bin/env node
// Build-freshness check. Rebuilds the web shell and fails if the
// committed `dist/` no longer matches `src/` — i.e. someone edited the frontend
// without regenerating the embedded bundle Rust serves. Run via
// `npm run web:dist:check`, including in CI.

import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const dist = "crates/ajax-web/web/dist";

execFileSync("npm", ["run", "web:build"], { cwd: repoRoot, stdio: "inherit" });

const diff = execFileSync("git", ["status", "--porcelain", "--", dist], {
  cwd: repoRoot,
  encoding: "utf8",
});

if (diff.trim()) {
  console.error(
    `\nweb dist is stale: rebuilding changed ${dist}.\n` +
      "Run `npm run web:build` and commit the updated dist/.\n\n" +
      diff,
  );
  process.exit(1);
}
console.log("web dist is fresh: committed bundle matches src.");
