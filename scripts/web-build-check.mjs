#!/usr/bin/env node
// Build-layout check. Runs the production build and asserts the
// emitted shell is deterministic and serving-compatible:
//   - shell, terminal chunk, stylesheet, and both Ghostty WASM assets exist
//   - terminal chunk includes ghostty-web but not the inactive wterm adapter path
//   - the HTML keeps the __AJAX_APP_VERSION__ placeholder Rust replaces
//   - exactly one local module script and one local stylesheet
// Run via `npm run web:build:check`. Exits non-zero on any violation.

import { execFileSync } from "node:child_process";
import { readFileSync, existsSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const distDir = join(repoRoot, "crates/ajax-web/web/dist");
const failures = [];

function check(condition, message) {
  if (!condition) failures.push(message);
}

execFileSync("npm", ["run", "web:build"], { cwd: repoRoot, stdio: "inherit" });

for (const name of ["index.html", "app.js", "terminal.js", "app.css"]) {
  const assetPath = join(distDir, name);
  if (!existsSync(assetPath)) continue;
  const contents = readFileSync(assetPath, "utf8");
  const normalized = contents.replace(/[ \t]+$/gm, "");
  if (contents !== normalized) writeFileSync(assetPath, normalized);
}

for (const name of ["index.html", "app.js", "terminal.js", "app.css", "ghostty-vt.wasm", "wterm-ghostty-vt.wasm"]) {
  check(existsSync(join(distDir, name)), `missing dist/${name}`);
}

const jsFiles = existsSync(distDir)
  ? readdirSync(distDir).filter((name) => name.endsWith(".js"))
  : [];
check(
  jsFiles.length === 2 && jsFiles.includes("app.js") && jsFiles.includes("terminal.js"),
  `expected exactly dist/app.js and dist/terminal.js, found ${jsFiles.join(", ") || "none"}`,
);

if (existsSync(join(distDir, "app.js")) && existsSync(join(distDir, "terminal.js"))) {
  const app = readFileSync(join(distDir, "app.js"), "utf8");
  const terminal = readFileSync(join(distDir, "terminal.js"), "utf8");
  check(!app.includes("/ghostty-vt.wasm"), "dist/app.js still contains the Ghostty terminal runtime");
  check(!app.includes('from"./terminal.js"'), "dist/app.js statically imports the terminal chunk");
  check(app.includes('import("./terminal.js")'), "dist/app.js is missing the lazy terminal import");
  check(terminal.includes("/ghostty-vt.wasm"), "dist/terminal.js is missing the Ghostty WASM path");
  check(
    !terminal.includes("/wterm-ghostty-vt.wasm"),
    "dist/terminal.js still references the inactive Wterm Ghostty runtime",
  );
  check(app.length < terminal.length, "dist/app.js should be smaller than the deferred terminal chunk");
}

if (existsSync(join(distDir, "ghostty-vt.wasm"))) {
  check(
    statSync(join(distDir, "ghostty-vt.wasm")).size > 0,
    "dist/ghostty-vt.wasm is empty",
  );
}

if (
  existsSync(join(distDir, "ghostty-vt.wasm")) &&
  existsSync(join(distDir, "wterm-ghostty-vt.wasm"))
) {
  const ghostty = readFileSync(join(distDir, "ghostty-vt.wasm"));
  const wterm = readFileSync(join(distDir, "wterm-ghostty-vt.wasm"));
  check(wterm.length > 0, "dist/wterm-ghostty-vt.wasm is empty");
  check(
    !ghostty.equals(wterm),
    "dist/wterm-ghostty-vt.wasm must differ from ghostty-web ghostty-vt.wasm",
  );
}

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
    !html.includes('href="/terminal.js"'),
    "built index.html should not preload the deferred terminal chunk",
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
