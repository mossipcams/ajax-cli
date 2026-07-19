#!/usr/bin/env node
// Build-layout check. Runs the production build and asserts the
// emitted shell is deterministic and serving-compatible:
//   - shell, stylesheet, and app script exist
//   - the HTML keeps the __AJAX_APP_VERSION__ placeholder Rust replaces
//   - exactly one local module script and one local stylesheet
// Run via `npm run web:build:check`. Exits non-zero on any violation.

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, writeFileSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const distDir = join(repoRoot, "crates/ajax-web/web/dist");
const failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

execFileSync("npm", ["run", "web:build"], { cwd: repoRoot, stdio: "inherit" });

for (const name of ["index.html", "app.js", "app.css", "terminal.js"]) {
  const assetPath = join(distDir, name);
  if (!existsSync(assetPath)) continue;
  const contents = readFileSync(assetPath, "utf8");
  const normalized = contents.replace(/[ \t]+$/gm, "");
  if (contents !== normalized) writeFileSync(assetPath, normalized);
}

for (const name of ["index.html", "app.js", "app.css", "terminal.js"]) {
  check(existsSync(join(distDir, name)), `missing dist/${name}`);
}

const jsFiles = existsSync(distDir)
  ? readdirSync(distDir).filter((name) => name.endsWith(".js")).sort()
  : [];
check(
  jsFiles.length === 2 && jsFiles[0] === "app.js" && jsFiles[1] === "terminal.js",
  `expected exactly dist/app.js + dist/terminal.js, found ${jsFiles.join(", ") || "none"}`,
);

check(!existsSync(join(distDir, "ghostty-vt.wasm")), "stale dist/ghostty-vt.wasm must not be emitted");

if (existsSync(join(distDir, "index.html"))) {
  const html = readFileSync(join(distDir, "index.html"), "utf8");
  check(
    html.includes("__AJAX_APP_VERSION__"),
    "built index.html dropped the __AJAX_APP_VERSION__ placeholder",
  );
  check(
    !html.includes('href="/manifest.webmanifest"'),
    "built index.html should not advertise a web manifest",
  );
  check(
    !html.includes('rel="apple-touch-icon"'),
    "built index.html should not advertise Home Screen icons",
  );
  check(
    !html.includes('href="/terminal.js"') && !html.includes('src="/terminal.js"'),
    "built index.html must not preload or synchronously load terminal.js (lazy import only)",
  );
  const scripts = html.match(/<script[^>]*(?<![a-z-])src=/g) ?? [];
  check(scripts.length === 1, `expected one local script, found ${scripts.length}`);
  const styles = html.match(/<link[^>]*rel="stylesheet"/g) ?? [];
  check(styles.length === 1, `expected one local stylesheet, found ${styles.length}`);
}

if (failures.length) {
  console.error("web build check failed:");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exit(1);
}
console.log("web build check passed: deterministic shell with version placeholder.");
