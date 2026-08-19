import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyChangedPaths,
  classifyPath,
  detectChangedPathLanes,
  formatGithubOutput,
  isFullTriggerPath,
  isLockfilePath,
  isRustPath,
  isWebPath,
  resolveRefs,
} from "./ci-changed-paths.mjs";

test("crates/ajax-web/web is web, not rust", () => {
  assert.equal(isWebPath("crates/ajax-web/web/src/App.tsx"), true);
  assert.equal(isRustPath("crates/ajax-web/web/src/App.tsx"), false);
});

test("crates/ajax-web/src is rust, not web", () => {
  assert.equal(isRustPath("crates/ajax-web/src/runtime.rs"), true);
  assert.equal(isWebPath("crates/ajax-web/src/runtime.rs"), false);
});

test("Cargo.lock sets rust and lockfile", () => {
  assert.deepEqual(classifyPath("Cargo.lock"), {
    rust: true,
    web: false,
    lockfile: true,
    full: false,
  });
});

test("Cargo.toml and rust toolchain files are rust", () => {
  for (const path of [
    "Cargo.toml",
    "crates/ajax-core/Cargo.toml",
    "rust-toolchain.toml",
    "rustfmt.toml",
    "clippy.toml",
    ".config/nextest.toml",
  ]) {
    assert.equal(isRustPath(path), true, path);
  }
});

test("web paths include package manifests and Playwright configs", () => {
  for (const path of [
    "package.json",
    "package-lock.json",
    "crates/ajax-web/web/playwright.config.mts",
    "crates/ajax-web/web/playwright.rust-server.config.mts",
  ]) {
    assert.equal(isWebPath(path), true, path);
  }
});

test("CI script and workflow diffs force full", () => {
  for (const path of [
    ".github/workflows/ci.yml",
    "scripts/verify-ci-workflows.mjs",
    "scripts/ci-changed-paths.mjs",
    "scripts/ci-changed-paths.test.mjs",
  ]) {
    assert.equal(isFullTriggerPath(path), true, path);
  }
});

test("docs-only diffs leave every lane false", () => {
  assert.deepEqual(
    classifyChangedPaths(["README.md", "docs/agent/pull-requests.md"]),
    { rust: false, web: false, lockfile: false, full: false },
  );
});

test("rust-only and web-only diffs stay in their lanes", () => {
  assert.deepEqual(classifyChangedPaths(["crates/ajax-core/src/lib.rs"]), {
    rust: true,
    web: false,
    lockfile: false,
    full: false,
  });

  assert.deepEqual(classifyChangedPaths(["crates/ajax-web/web/src/App.tsx"]), {
    rust: false,
    web: true,
    lockfile: false,
    full: false,
  });
});

test("workflow diffs union into full without clearing other lanes", () => {
  assert.deepEqual(
    classifyChangedPaths([
      "crates/ajax-core/src/lib.rs",
      ".github/workflows/ci.yml",
    ]),
    { rust: true, web: false, lockfile: false, full: true },
  );
});

test("missing SHAs and workflow_dispatch force full", () => {
  assert.deepEqual(classifyChangedPaths([], { forceFull: true }), {
    rust: true,
    web: true,
    lockfile: true,
    full: true,
  });

  assert.equal(
    resolveRefs({ GITHUB_EVENT_NAME: "workflow_dispatch" }),
    null,
  );
  assert.equal(
    resolveRefs({ GITHUB_EVENT_NAME: "pull_request" }),
    null,
  );
  assert.deepEqual(
    resolveRefs({
      GITHUB_EVENT_NAME: "pull_request",
      GITHUB_BASE_SHA: "base",
      GITHUB_HEAD_SHA: "head",
    }),
    { base: "base", head: "head" },
  );
});

test("detectChangedPathLanes uses git diff output", async () => {
  const { files, flags } = await detectChangedPathLanes({
    env: {
      GITHUB_EVENT_NAME: "pull_request",
      GITHUB_BASE_SHA: "base",
      GITHUB_HEAD_SHA: "head",
    },
    runGit: async () => "crates/ajax-core/src/lib.rs\nCargo.lock\n",
  });

  assert.deepEqual(files, ["crates/ajax-core/src/lib.rs", "Cargo.lock"]);
  assert.deepEqual(flags, {
    rust: true,
    web: false,
    lockfile: true,
    full: false,
  });
});

test("formatGithubOutput matches GitHub Actions booleans", () => {
  assert.equal(
    formatGithubOutput({ rust: true, web: false, lockfile: true, full: false }),
    "rust=true\nweb=false\nlockfile=true\nfull=false",
  );
});
